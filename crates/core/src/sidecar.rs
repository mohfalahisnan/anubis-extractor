//! Sidecar cache for preprocessing artifacts.
//!
//! Heavy preprocessing (Whisper transcription, image OCR, future scanned-PDF
//! OCR) writes its output as a `<stem>.anubis.txt` file next to the source
//! media. The indexing pass reads the sidecar instead of re-running the
//! preprocessing — see [`crate::engine::preprocess`] for how the pre-pass
//! produces them and [`crate::parser::video`] / [`crate::parser::image`] for
//! how the indexer consumes them.
//!
//! This module owns the entire sidecar contract end-to-end:
//!   - where a sidecar lives on disk (`<source-dir>/<stem>.anubis.txt`,
//!     overridable via `ANUBIS_TRANSCRIPT_DIR`)
//!   - what counts as a *fresh* cache hit (sidecar mtime ≥ source mtime)
//!   - how to write one atomically (`.tmp` → rename) so a crash mid-write
//!     can't leave a half-finished sidecar that later gets read as truth
//!
//! Before this module existed, the same three policies lived in three
//! near-identical copies inside `parser::video`, `parser::image`, and
//! `engine::preprocess`. Adding a fourth preprocessing kind would have
//! shipped a fourth copy. Consolidating concentrates the policy so
//! freshness bugs, atomic-write bugs, and path-resolution bugs all live in
//! one place.

use std::path::{Path, PathBuf};

/// A successfully-read sidecar: the path it was read from plus its
/// contents. The path is returned so callers can log which file they
/// hit (useful when `ANUBIS_TRANSCRIPT_DIR` relocates sidecars away from
/// the source).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    pub path: PathBuf,
    pub text: String,
}

/// Read the sidecar for `source` if it exists AND is at least as fresh
/// as the source. Returns `None` for any failure mode — missing
/// sidecar, stale mtime, unreadable file, unreadable metadata, or a
/// source path with no file stem. Callers treat "no fresh sidecar" as
/// the signal to run preprocessing.
///
/// An empty sidecar is *authoritative*: it means a previous preprocess
/// run produced no usable output (e.g. Whisper found no speech). The
/// caller can delete the sidecar file to force a retry.
pub fn read(source: &Path) -> Option<Sidecar> {
    let path = path_for(source)?;
    if !is_fresh_against(source, &path) {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    Some(Sidecar { path, text })
}

/// `true` iff the sidecar exists and its mtime is `>=` the source's.
/// Used by the preprocessing pre-pass to decide whether to skip a file.
/// Equivalent to `read(source).is_some()` but doesn't allocate or
/// read the file contents — cheap to call on every file in a batch.
pub fn is_fresh(source: &Path) -> bool {
    match path_for(source) {
        Some(sidecar_path) => is_fresh_against(source, &sidecar_path),
        None => false,
    }
}

/// Write `text` to `<stem>.anubis.txt` atomically. Stages to a `.tmp`
/// file in the same directory and renames into place on success. A
/// crash between `write` and `rename` leaves the `.tmp` behind (which
/// no other code reads) instead of a half-written sidecar.
///
/// Returns the path that was written so callers can log or pass it on.
pub fn write_atomic(source: &Path, text: &str) -> std::io::Result<PathBuf> {
    let final_path =
        path_for(source).ok_or_else(|| std::io::Error::other("source has no file stem"))?;
    let dir = final_path
        .parent()
        .ok_or_else(|| std::io::Error::other("sidecar path has no parent"))?;
    std::fs::create_dir_all(dir)?;

    let mut tmp_path = final_path.clone();
    let tmp_name = match final_path.file_name().and_then(|n| n.to_str()) {
        Some(name) => format!("{name}.tmp"),
        None => return Err(std::io::Error::other("sidecar path has no file name")),
    };
    tmp_path.set_file_name(tmp_name);

    std::fs::write(&tmp_path, text)?;
    std::fs::rename(&tmp_path, &final_path)?;
    Ok(final_path)
}

/// Resolve the canonical sidecar path for a source file. Returns `None`
/// when `source` has no extractable file stem.
pub fn path_for(source: &Path) -> Option<PathBuf> {
    let stem = source.file_stem().and_then(|s| s.to_str())?;
    let dir = sidecar_dir_for(source);
    Some(dir.join(format!("{stem}.anubis.txt")))
}

/// The directory a sidecar lives in. Defaults to the source's parent
/// directory; `ANUBIS_TRANSCRIPT_DIR` (a non-empty value) relocates
/// every sidecar to that directory instead — useful when source media
/// is on read-only storage.
fn sidecar_dir_for(source: &Path) -> PathBuf {
    if let Ok(env_dir) = std::env::var("ANUBIS_TRANSCRIPT_DIR") {
        if !env_dir.trim().is_empty() {
            return PathBuf::from(env_dir);
        }
    }
    source
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn is_fresh_against(source: &Path, sidecar: &Path) -> bool {
    let (Ok(src_meta), Ok(car_meta)) = (std::fs::metadata(source), std::fs::metadata(sidecar))
    else {
        return false;
    };
    match (src_meta.modified(), car_meta.modified()) {
        (Ok(src_mtime), Ok(car_mtime)) => car_mtime >= src_mtime,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    /// Write a file and force its modified time so the test doesn't depend on
    /// wall-clock granularity (Windows FAT/NTFS can be 2-second-coarse).
    fn write_with_mtime(path: &Path, contents: &str, mtime: SystemTime) {
        std::fs::write(path, contents).expect("write fixture");
        let file = std::fs::File::options()
            .write(true)
            .open(path)
            .expect("open for mtime");
        file.set_modified(mtime).expect("set mtime");
    }

    fn fresh_tmp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("anubis-sidecar-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_returns_sidecar_when_at_least_as_fresh_as_source() {
        let dir = fresh_tmp("fresh");
        let source = dir.join("clip.mp4");
        let sidecar = dir.join("clip.anubis.txt");
        let base = SystemTime::now() - Duration::from_secs(10);
        write_with_mtime(&source, "fake mp4", base);
        write_with_mtime(&sidecar, "transcript", base + Duration::from_secs(5));

        let got = read(&source).expect("should hit");
        assert_eq!(got.path, sidecar);
        assert_eq!(got.text, "transcript");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_returns_none_when_sidecar_older_than_source() {
        let dir = fresh_tmp("stale");
        let source = dir.join("clip.mp4");
        let sidecar = dir.join("clip.anubis.txt");
        let base = SystemTime::now() - Duration::from_secs(10);
        write_with_mtime(&sidecar, "stale", base);
        write_with_mtime(&source, "fresh mp4", base + Duration::from_secs(5));

        assert!(read(&source).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_returns_none_when_sidecar_missing() {
        let dir = fresh_tmp("missing");
        let source = dir.join("clip.mp4");
        write_with_mtime(&source, "fake", SystemTime::now());

        assert!(read(&source).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_fresh_mirrors_read_without_loading_content() {
        let dir = fresh_tmp("isfresh");
        let source = dir.join("clip.mp4");
        let sidecar = dir.join("clip.anubis.txt");
        let base = SystemTime::now() - Duration::from_secs(10);
        write_with_mtime(&source, "fake mp4", base);
        write_with_mtime(&sidecar, "transcript", base);

        assert!(is_fresh(&source));

        // Bump the source forward; sidecar is now stale.
        write_with_mtime(&source, "fake mp4 modified", base + Duration::from_secs(5));
        assert!(!is_fresh(&source));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_uses_tmp_rename_pattern() {
        let dir = fresh_tmp("atomic");
        let source = dir.join("photo.png");
        std::fs::write(&source, "fake png").unwrap();

        let written = write_atomic(&source, "ocr text").expect("atomic write");
        assert_eq!(written, dir.join("photo.anubis.txt"));
        assert_eq!(std::fs::read_to_string(&written).unwrap(), "ocr text");

        // The .tmp file must not be left behind.
        let tmp = dir.join("photo.anubis.txt.tmp");
        assert!(!tmp.exists(), "tmp staging file leaked: {}", tmp.display());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_overwrites_existing_sidecar() {
        let dir = fresh_tmp("overwrite");
        let source = dir.join("photo.png");
        let sidecar = dir.join("photo.anubis.txt");
        std::fs::write(&source, "fake png").unwrap();
        std::fs::write(&sidecar, "old").unwrap();

        write_atomic(&source, "new").expect("atomic write");
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), "new");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn path_for_returns_anubis_txt_next_to_source() {
        let source = Path::new("/anywhere/clip.mp4");
        let got = path_for(source).expect("path");
        assert_eq!(got, Path::new("/anywhere/clip.anubis.txt"));
    }
}
