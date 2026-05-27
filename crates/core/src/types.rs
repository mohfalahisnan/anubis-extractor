//! Public option / result / progress types.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WhisperModel {
    Tiny,
    Base,
    Small,
    #[default]
    Medium,
    LargeV3,
}

impl WhisperModel {
    pub fn as_variant(self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::LargeV3 => "large-v3",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Progress {
    Download {
        artifact: String,
        bytes: u64,
        total: Option<u64>,
    },
    Stage {
        stage: &'static str,
        message: String,
    },
    Segment {
        index: usize,
        total: Option<usize>,
        text: String,
    },
}

#[derive(Debug, Default)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub model: Option<WhisperModel>,
    pub write_sidecar: bool,
    pub force: bool,
    pub progress: Option<mpsc::Sender<Progress>>,
}

#[derive(Debug, Default)]
pub struct OcrOptions {
    pub write_sidecar: bool,
    pub force: bool,
    pub progress: Option<mpsc::Sender<Progress>>,
}

#[derive(Debug, Default)]
pub struct ExtractOptions {
    pub language: Option<String>,
    pub model: Option<WhisperModel>,
    pub write_sidecar: bool,
    pub force: bool,
    pub progress: Option<mpsc::Sender<Progress>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrLine {
    /// [x, y, width, height] in pixels.
    pub bbox: [u32; 4],
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscribeResult {
    pub text: String,
    pub segments: Vec<Segment>,
    pub language: Option<String>,
    pub sidecar_path: Option<PathBuf>,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub lines: Vec<OcrLine>,
    pub sidecar_path: Option<PathBuf>,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ExtractResult {
    Transcribe(TranscribeResult),
    Ocr(OcrResult),
}

#[derive(Debug, Clone)]
pub struct Config {
    pub cache_dir: PathBuf,
    pub whisper_model: WhisperModel,
}
