//! anubis-extractor — OCR and transcription as a library.

#![deny(rust_2018_idioms)]

pub use error::ExtractorError;
pub use types::*;

pub mod cache;
pub mod download;
mod error;
pub mod sidecar;
mod types;
