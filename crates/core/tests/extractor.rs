use anubis_extractor::{Config, Extractor, WhisperModel};

#[tokio::test]
async fn extractor_constructs_without_touching_disk() {
    let config = Config {
        cache_dir: std::env::temp_dir().join("anubis-extractor-test-construct"),
        whisper_model: WhisperModel::Tiny,
    };
    let ext = Extractor::new(config).expect("construct");
    let _ = ext; // smoke
}
