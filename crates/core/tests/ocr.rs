//! Real-model OCR test. Marked `#[ignore]` so default `cargo test` stays fast;
//! run with `cargo test -p anubis-extractor --test ocr -- --ignored` once the
//! fixtures and a writable cache dir are available.

#[test]
#[ignore = "downloads ~10MB of models"]
fn ocr_on_fixture_returns_some_text() {
    // Fixture will be added in a later task. Skeleton stays here so the test
    // file compiles end-to-end.
}
