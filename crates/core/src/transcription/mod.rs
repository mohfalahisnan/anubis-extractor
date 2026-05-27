//! Audio + video transcription via whisper.cpp invoked as a subprocess.

mod artifacts;
mod pipeline;

pub(crate) use artifacts::TranscribeBackend;
pub(crate) use pipeline::transcribe;
