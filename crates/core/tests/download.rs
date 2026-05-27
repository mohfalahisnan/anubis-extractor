use anubis_extractor::download::{ensure_file, DownloadEvent};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[test]
fn ensure_file_short_circuits_when_file_already_exists() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("already-here.bin");
    std::fs::write(&path, b"hello").unwrap();

    let events: Arc<Mutex<Vec<DownloadEvent>>> = Default::default();
    let sink = {
        let events = events.clone();
        move |ev: DownloadEvent| events.lock().unwrap().push(ev)
    };

    ensure_file(
        &path,
        "https://example.invalid/never-called",
        "evt",
        "label",
        Some(&sink),
    )
    .expect("short-circuit on existing file");

    assert!(
        events.lock().unwrap().is_empty(),
        "no events emitted when file present"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
}
