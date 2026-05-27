//! Resolve and cache the binaries + model file Whisper needs.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::download::{ensure_file, EventSink};
use crate::types::WhisperModel;
use crate::ExtractorError;

const WHISPER_BIN_URL_WIN64: &str =
    "https://github.com/ggerganov/whisper.cpp/releases/download/v1.7.6/whisper-bin-x64.zip";
const FFMPEG_URL_WIN64: &str =
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";

const WHISPER_DIR_NAME: &str = "transcription-whisper";
const FFMPEG_DIR_NAME: &str = "transcription-ffmpeg";

const FFMPEG_EVENT_ID: &str = "ffmpeg";
const FFMPEG_LABEL: &str = "ffmpeg (audio extractor ~80MB)";
const WHISPER_BIN_EVENT_ID: &str = "whisper-binary";
const WHISPER_BIN_LABEL: &str = "Whisper.cpp binary";
const WHISPER_MODEL_EVENT_ID: &str = "whisper-model";

#[derive(Clone)]
pub(crate) struct TranscribeBackend {
    inner: Arc<TranscribeInner>,
}

struct TranscribeInner {
    cache_dir: PathBuf,
    default_model: WhisperModel,
    cached: Mutex<Option<Artifacts>>,
}

#[derive(Clone)]
pub(crate) struct Artifacts {
    pub ffmpeg_exe: PathBuf,
    pub whisper_exe: PathBuf,
    pub whisper_model: PathBuf,
    pub model_variant: String,
}

impl TranscribeBackend {
    pub fn new(cache_dir: PathBuf, default_model: WhisperModel) -> Self {
        Self {
            inner: Arc::new(TranscribeInner {
                cache_dir,
                default_model,
                cached: Mutex::new(None),
            }),
        }
    }

    pub fn version_tag(&self, model: WhisperModel) -> String {
        format!("whisper-{}-v1.7.6", model.as_variant())
    }

    pub fn ensure(
        &self,
        override_model: Option<WhisperModel>,
        sink: Option<&EventSink<'_>>,
    ) -> Result<Artifacts, ExtractorError> {
        let wanted = override_model.unwrap_or(self.inner.default_model);
        let mut guard = self.inner.cached.lock().expect("artifacts mutex poisoned");
        if let Some(art) = guard.as_ref() {
            if art.model_variant == wanted.as_variant() {
                return Ok(art.clone());
            }
        }
        let built = self.build(wanted, sink)?;
        *guard = Some(built.clone());
        Ok(built)
    }

    fn build(
        &self,
        model: WhisperModel,
        sink: Option<&EventSink<'_>>,
    ) -> Result<Artifacts, ExtractorError> {
        let ffmpeg_dir = self.inner.cache_dir.join(FFMPEG_DIR_NAME);
        std::fs::create_dir_all(&ffmpeg_dir)?;
        let ffmpeg_exe = ensure_ffmpeg_binary(&ffmpeg_dir, sink)?;

        let whisper_dir = self.inner.cache_dir.join(WHISPER_DIR_NAME);
        std::fs::create_dir_all(&whisper_dir)?;
        let whisper_exe = ensure_whisper_binary(&whisper_dir, sink)?;

        let (model_file, model_url, model_label) = whisper_model_artifact(model.as_variant());
        let model_path = whisper_dir.join(&model_file);
        ensure_file(&model_path, &model_url, WHISPER_MODEL_EVENT_ID, &model_label, sink)
            .map_err(ExtractorError::Download)?;

        Ok(Artifacts {
            ffmpeg_exe,
            whisper_exe,
            whisper_model: model_path,
            model_variant: model.as_variant().to_string(),
        })
    }
}

fn whisper_model_artifact(variant: &str) -> (String, String, String) {
    let file = format!("ggml-{variant}.bin");
    let url = format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{file}");
    let size_hint = match variant {
        "tiny" | "tiny.en" => "~75MB",
        "base" | "base.en" => "~145MB",
        "small" | "small.en" => "~480MB",
        "medium" | "medium.en" => "~1.5GB",
        "large-v1" | "large-v2" | "large-v3" | "large" => "~3GB",
        _ => "unknown size",
    };
    let label = format!("Whisper model ({variant} multilingual {size_hint})");
    (file, url, label)
}

#[cfg(target_os = "windows")]
fn ensure_ffmpeg_binary(
    dir: &Path,
    sink: Option<&EventSink<'_>>,
) -> Result<PathBuf, ExtractorError> {
    let exe = dir.join("ffmpeg.exe");
    if exe.exists() && validate_ffmpeg(&exe).is_ok() {
        return Ok(exe);
    }
    if exe.exists() {
        let _ = std::fs::remove_file(&exe);
    }
    if let Ok(system_exe) = which_ffmpeg() {
        if validate_ffmpeg(&system_exe).is_ok() {
            return Ok(system_exe);
        }
    }
    let zip_path = dir.join("ffmpeg-win64.zip");
    let _ = std::fs::remove_file(&zip_path);
    ensure_file(&zip_path, FFMPEG_URL_WIN64, FFMPEG_EVENT_ID, FFMPEG_LABEL, sink)
        .map_err(ExtractorError::Download)?;
    extract_specific_file(&zip_path, "bin/ffmpeg.exe", &exe)?;
    let _ = std::fs::remove_file(&zip_path);
    validate_ffmpeg(&exe).map_err(|e| {
        let _ = std::fs::remove_file(&exe);
        ExtractorError::Ffmpeg(format!("ffmpeg.exe downloaded but failed to execute ({e})."))
    })?;
    Ok(exe)
}

#[cfg(not(target_os = "windows"))]
fn ensure_ffmpeg_binary(
    _dir: &Path,
    _sink: Option<&EventSink<'_>>,
) -> Result<PathBuf, ExtractorError> {
    which_ffmpeg().map_err(|_| {
        ExtractorError::Ffmpeg(
            "ffmpeg not found on PATH. Install it (e.g. `brew install ffmpeg`) for transcription."
                .into(),
        )
    })
}

fn validate_ffmpeg(exe: &Path) -> Result<(), String> {
    let output = Command::new(exe)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("spawn {}: {e}", exe.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} -version exited {:?}",
            exe.display(),
            output.status.code()
        ));
    }
    Ok(())
}

fn which_ffmpeg() -> Result<PathBuf, String> {
    let exe_name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let path_var = std::env::var_os("PATH").ok_or("PATH not set")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(exe_name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!("{exe_name} not found on PATH"))
}

#[cfg(target_os = "windows")]
fn ensure_whisper_binary(
    dir: &Path,
    sink: Option<&EventSink<'_>>,
) -> Result<PathBuf, ExtractorError> {
    let exe = dir.join("whisper-cli.exe");
    if exe.exists() {
        return Ok(exe);
    }
    let zip_path = dir.join("whisper-bin-x64.zip");
    let _ = std::fs::remove_file(&zip_path);
    ensure_file(
        &zip_path,
        WHISPER_BIN_URL_WIN64,
        WHISPER_BIN_EVENT_ID,
        WHISPER_BIN_LABEL,
        sink,
    )
    .map_err(ExtractorError::Download)?;
    extract_all_to(&zip_path, dir)?;
    let _ = std::fs::remove_file(&zip_path);
    if !exe.exists() {
        return Err(ExtractorError::Whisper(format!(
            "whisper-cli.exe not found after extracting {}",
            zip_path.display()
        )));
    }
    Ok(exe)
}

#[cfg(not(target_os = "windows"))]
fn ensure_whisper_binary(
    _dir: &Path,
    _sink: Option<&EventSink<'_>>,
) -> Result<PathBuf, ExtractorError> {
    Err(ExtractorError::Whisper(
        "whisper.cpp binary auto-download only implemented for Windows; \
         build whisper.cpp from source and place `whisper-cli` on PATH."
            .into(),
    ))
}

#[cfg(target_os = "windows")]
fn extract_specific_file(
    zip_path: &Path,
    suffix: &str,
    dest: &Path,
) -> Result<(), ExtractorError> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| ExtractorError::Ffmpeg(format!("open zip: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ExtractorError::Ffmpeg(format!("read zip: {e}")))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ExtractorError::Ffmpeg(format!("zip index {i}: {e}")))?;
        if entry.name().replace('\\', "/").ends_with(suffix) {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(ExtractorError::Ffmpeg(format!(
        "entry ending in {suffix:?} not found in zip"
    )))
}

#[cfg(target_os = "windows")]
fn extract_all_to(zip_path: &Path, dest_dir: &Path) -> Result<(), ExtractorError> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| ExtractorError::Whisper(format!("open zip: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| ExtractorError::Whisper(format!("read zip: {e}")))?;
    std::fs::create_dir_all(dest_dir)?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ExtractorError::Whisper(format!("zip index {i}: {e}")))?;
        let outpath = match entry.enclosed_name() {
            Some(p) => dest_dir.join(p),
            None => continue,
        };
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut out = std::fs::File::create(&outpath)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}
