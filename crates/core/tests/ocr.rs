use anubis_extractor::{Config, Extractor, OcrOptions, WhisperModel};
use std::path::PathBuf;

#[tokio::test]
#[ignore = "downloads ~10MB of OCR models on first run"]
async fn ocr_on_fixture_recovers_text() {
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/.cache");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.png");

    let ext = Extractor::new(Config {
        cache_dir: cache,
        whisper_model: WhisperModel::Tiny,
    }).expect("construct extractor");

    let result = ext.ocr(&fixture, OcrOptions::default()).await.expect("ocr");

    let upper = result.text.to_ascii_uppercase();
    assert!(
        upper.contains("HELLO") || upper.contains("ANUBIS"),
        "expected HELLO/ANUBIS in output, got: {:?}",
        result.text
    );
}
