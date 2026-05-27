use anubis_extractor::{Config, Extractor, WhisperModel};
use std::path::PathBuf;

#[tokio::test]
async fn extractor_constructs_without_touching_disk() {
    let config = Config {
        cache_dir: PathBuf::from(std::env::temp_dir()).join("anubis-extractor-test-construct"),
        whisper_model: WhisperModel::Tiny,
    };
    let ext = Extractor::new(config).expect("construct");
    let _ = ext; // smoke
}
