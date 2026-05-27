//! Input/output schemas for the three MCP tools.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct TranscribeInput {
    /// Absolute path to an audio or video file.
    pub path: String,
    /// ISO-639-1 hint (e.g. "id", "en"). Auto-detect if omitted.
    #[serde(default)]
    pub language: Option<String>,
    /// One of base | small | medium | large-v3.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub write_sidecar: bool,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct OcrInput {
    pub path: String,
    #[serde(default)]
    pub write_sidecar: bool,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ExtractInput {
    pub path: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub write_sidecar: bool,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ToolOutput {
    Transcribe(anubis_extractor::TranscribeResult),
    Ocr(anubis_extractor::OcrResult),
}
