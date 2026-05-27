//! Resilient model/binary downloader.
//!
//! Why we don't lean on `hf-hub`: it has hit `request error: timeout: global`
//! on slow links because ureq's defaults aren't tuned for large files over
//! flaky connections, and the timeout isn't user-configurable. Owning the
//! download ourselves also lets us stream progress to whatever the caller is.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Per-request timeout — generous for slow networks but not unbounded.
const DOWNLOAD_TIMEOUT_SECS: u64 = 30 * 60;
/// Hard cap on a single file. Guards against a hijacked endpoint streaming forever.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// How often to emit a `Downloading` event during a download.
const PROGRESS_TICK_MS: u128 = 200;

/// One callback fires per state transition. Callers translate these into MCP
/// progress notifications, Tauri events, or stderr lines as appropriate.
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Starting { id: String, label: String },
    Downloading { id: String, label: String, bytes: u64, total: Option<u64> },
    Ready { id: String, label: String },
    Error { id: String, label: String, message: String },
}

pub type EventSink<'a> = dyn Fn(DownloadEvent) + Send + Sync + 'a;

/// Skip download when the file is already present locally. Otherwise: stream
/// to a `.partial` next to the destination, then rename into place.
pub fn ensure_file(
    path: &Path,
    url: &str,
    event_id: &str,
    label: &str,
    sink: Option<&EventSink<'_>>,
) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create dir {parent:?}: {error}"))?;
    }
    tracing::info!("downloading {} -> {}", url, path.display());
    emit(sink, DownloadEvent::Starting { id: event_id.into(), label: label.into() });

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build();

    let response = agent.get(url).call().map_err(|error| {
        let msg = format!("download failed ({url}): {error}");
        emit(sink, DownloadEvent::Error { id: event_id.into(), label: label.into(), message: msg.clone() });
        msg
    })?;

    let total: Option<u64> = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());

    let mut reader = response.into_reader().take(MAX_FILE_BYTES);
    let mut bytes: Vec<u8> = Vec::with_capacity(total.unwrap_or(8 * 1024 * 1024) as usize);
    let mut buf = [0u8; 64 * 1024];
    let mut last_emit = Instant::now();

    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&buf[..n]);
                if last_emit.elapsed().as_millis() >= PROGRESS_TICK_MS {
                    emit(sink, DownloadEvent::Downloading {
                        id: event_id.into(),
                        label: label.into(),
                        bytes: bytes.len() as u64,
                        total,
                    });
                    last_emit = Instant::now();
                }
            }
            Err(error) => {
                let msg = format!("download read failed ({url}): {error}");
                emit(sink, DownloadEvent::Error { id: event_id.into(), label: label.into(), message: msg.clone() });
                return Err(msg);
            }
        }
    }

    if bytes.len() as u64 >= MAX_FILE_BYTES {
        let msg = format!("download {url} exceeded {MAX_FILE_BYTES} bytes; aborting");
        emit(sink, DownloadEvent::Error { id: event_id.into(), label: label.into(), message: msg.clone() });
        return Err(msg);
    }

    emit(sink, DownloadEvent::Downloading {
        id: event_id.into(),
        label: label.into(),
        bytes: bytes.len() as u64,
        total,
    });

    let tmp_path: PathBuf = path.with_extension(format!(
        "{}.partial",
        path.extension().and_then(|s| s.to_str()).unwrap_or("tmp")
    ));
    std::fs::write(&tmp_path, &bytes)
        .map_err(|error| format!("failed to write temp file: {error}"))?;
    std::fs::rename(&tmp_path, path).map_err(|error| format!("failed to install file: {error}"))?;

    tracing::info!("downloaded {} ({} bytes)", path.display(), bytes.len());
    emit(sink, DownloadEvent::Ready { id: event_id.into(), label: label.into() });
    Ok(())
}

/// Quiet variant used for tiny companion files.
pub fn ensure_file_quiet(path: &Path, url: &str) -> Result<(), String> {
    ensure_file(path, url, "", "", None)
}

fn emit(sink: Option<&EventSink<'_>>, event: DownloadEvent) {
    if let Some(f) = sink {
        f(event);
    }
}
