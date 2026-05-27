//! Public error type. Variants are added as backends come online.

#[derive(Debug, thiserror::Error)]
pub enum ExtractorError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("download failed: {0}")]
    Download(String),
    #[error("whisper: {0}")]
    Whisper(String),
    #[error("ffmpeg: {0}")]
    Ffmpeg(String),
    #[error("ocr: {0}")]
    Ocr(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("cancelled")]
    Cancelled,
}
