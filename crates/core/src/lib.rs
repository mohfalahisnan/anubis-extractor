//! anubis-extractor — OCR and transcription as a library.
//!
//! See `Extractor` for the entry point.

#![deny(rust_2018_idioms)]

pub use error::ExtractorError;

mod error;

#[derive(Debug, Clone, Copy, Default)]
pub struct Placeholder;
