//! Public `Extractor` façade — the entry point for the library.

use std::path::{Path, PathBuf};

use tokio::task;

use crate::download::{DownloadEvent, EventSink};
use crate::ocr::OcrBackend;
use crate::sidecar;
use crate::transcription::{transcribe as transcribe_pipeline, TranscribeBackend};
use crate::types::{
    Config, ExtractOptions, ExtractResult, OcrOptions, OcrResult, Progress, TranscribeOptions,
    TranscribeResult,
};
use crate::ExtractorError;

pub struct Extractor {
    #[allow(dead_code)]
    cache_dir: PathBuf,
    ocr: OcrBackend,
    transcribe: TranscribeBackend,
}

impl Extractor {
    pub fn new(config: Config) -> Result<Self, ExtractorError> {
        std::fs::create_dir_all(&config.cache_dir)?;
        let ocr = OcrBackend::new(config.cache_dir.clone());
        let transcribe = TranscribeBackend::new(config.cache_dir.clone(), config.whisper_model);
        Ok(Self {
            cache_dir: config.cache_dir,
            ocr,
            transcribe,
        })
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub async fn ocr(&self, input: &Path, opts: OcrOptions) -> Result<OcrResult, ExtractorError> {
        let input = input.to_owned();
        let handles = self.clone_handles();
        run_blocking(move || handles.ocr_blocking(&input, opts)).await
    }

    pub async fn transcribe(
        &self,
        input: &Path,
        opts: TranscribeOptions,
    ) -> Result<TranscribeResult, ExtractorError> {
        let input = input.to_owned();
        let handles = self.clone_handles();
        run_blocking(move || handles.transcribe_blocking(&input, opts)).await
    }

    pub async fn extract_text(
        &self,
        input: &Path,
        opts: ExtractOptions,
    ) -> Result<ExtractResult, ExtractorError> {
        match classify(input)? {
            Kind::Image => {
                let r = self
                    .ocr(
                        input,
                        OcrOptions {
                            write_sidecar: opts.write_sidecar,
                            force: opts.force,
                            progress: opts.progress,
                        },
                    )
                    .await?;
                Ok(ExtractResult::Ocr(r))
            }
            Kind::Audiovisual => {
                let r = self
                    .transcribe(
                        input,
                        TranscribeOptions {
                            language: opts.language,
                            model: opts.model,
                            write_sidecar: opts.write_sidecar,
                            force: opts.force,
                            progress: opts.progress,
                        },
                    )
                    .await?;
                Ok(ExtractResult::Transcribe(r))
            }
        }
    }

    // Internal helper: build an `Arc`-shaped clone of the backend handles so
    // we can move into spawn_blocking. The Extractor itself isn't Clone, but
    // each backend uses Arc<Inner> and is therefore cheaply clonable.
    fn clone_handles(&self) -> Handles {
        Handles {
            ocr: self.ocr.clone(),
            transcribe: self.transcribe.clone(),
        }
    }
}

struct Handles {
    ocr: OcrBackend,
    transcribe: TranscribeBackend,
}

impl Handles {
    fn ocr_blocking(&self, input: &Path, opts: OcrOptions) -> Result<OcrResult, ExtractorError> {
        if !opts.force {
            if let Some(side) = sidecar::read(input) {
                return Ok(OcrResult {
                    text: side.text,
                    lines: vec![],
                    sidecar_path: Some(side.path),
                    cache_hit: true,
                });
            }
        }

        let bytes = std::fs::read(input)?;
        let sink: Option<Box<dyn Fn(DownloadEvent) + Send + Sync>> =
            make_download_sink(opts.progress.as_ref());
        let mut result = self
            .ocr
            .run(&bytes, sink.as_deref().map(|b| b as &EventSink<'_>))?;

        if opts.write_sidecar {
            let path = sidecar::write_atomic(input, &result.text).map_err(ExtractorError::Io)?;
            result.sidecar_path = Some(path);
        }
        Ok(result)
    }

    fn transcribe_blocking(
        &self,
        input: &Path,
        opts: TranscribeOptions,
    ) -> Result<TranscribeResult, ExtractorError> {
        if !opts.force {
            if let Some(side) = sidecar::read(input) {
                return Ok(TranscribeResult {
                    text: side.text,
                    segments: vec![],
                    language: None,
                    sidecar_path: Some(side.path),
                    cache_hit: true,
                });
            }
        }

        let sink: Option<Box<dyn Fn(DownloadEvent) + Send + Sync>> =
            make_download_sink(opts.progress.as_ref());
        let artifacts = self
            .transcribe
            .ensure(opts.model, sink.as_deref().map(|b| b as &EventSink<'_>))?;

        let mut result = transcribe_pipeline(
            input,
            &artifacts,
            opts.language.as_deref(),
            opts.progress.as_ref(),
        )?;

        if opts.write_sidecar {
            let path = sidecar::write_atomic(input, &result.text).map_err(ExtractorError::Io)?;
            result.sidecar_path = Some(path);
        }
        Ok(result)
    }
}

enum Kind {
    Image,
    Audiovisual,
}

fn classify(path: &Path) -> Result<Kind, ExtractorError> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            ExtractorError::UnsupportedFormat(format!("no extension: {}", path.display()))
        })?;
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "tiff" | "tif" | "bmp" => Ok(Kind::Image),
        "mp3" | "wav" | "m4a" | "flac" | "ogg" | "opus" | "mp4" | "mov" | "mkv" | "webm"
        | "avi" => Ok(Kind::Audiovisual),
        other => Err(ExtractorError::UnsupportedFormat(other.to_string())),
    }
}

/// `spawn_blocking` wrapper that flattens the `JoinError` into our error.
async fn run_blocking<F, T>(f: F) -> Result<T, ExtractorError>
where
    F: FnOnce() -> Result<T, ExtractorError> + Send + 'static,
    T: Send + 'static,
{
    match task::spawn_blocking(f).await {
        Ok(r) => r,
        Err(e) => Err(ExtractorError::Io(std::io::Error::other(format!(
            "join error: {e}"
        )))),
    }
}

/// Translate `Progress` events into `DownloadEvent`s for `ensure_file`.
fn make_download_sink(
    progress: Option<&tokio::sync::mpsc::Sender<Progress>>,
) -> Option<Box<dyn Fn(DownloadEvent) + Send + Sync>> {
    let tx = progress?.clone();
    Some(Box::new(move |ev| {
        let mapped = match ev {
            DownloadEvent::Downloading {
                label,
                bytes,
                total,
                ..
            } => Progress::Download {
                artifact: label,
                bytes,
                total,
            },
            DownloadEvent::Starting { label, .. } => Progress::Stage {
                stage: "download",
                message: format!("starting: {label}"),
            },
            DownloadEvent::Ready { label, .. } => Progress::Stage {
                stage: "download",
                message: format!("ready: {label}"),
            },
            DownloadEvent::Error { label, message, .. } => Progress::Stage {
                stage: "download",
                message: format!("error ({label}): {message}"),
            },
        };
        let _ = tx.try_send(mapped);
    }))
}
