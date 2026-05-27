//! Resolve where model files and downloaded binaries live.
//!
//! Priority (highest first):
//!   1. Explicit `Some(path)` argument (the binary's --cache-dir flag)
//!   2. `ANUBIS_EXTRACTOR_CACHE_DIR` env var
//!   3. Per-user OS default

use std::path::PathBuf;

pub fn resolve_cache_dir(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(p) = explicit {
        return p;
    }
    if let Ok(env) = std::env::var("ANUBIS_EXTRACTOR_CACHE_DIR") {
        if !env.trim().is_empty() {
            return PathBuf::from(env);
        }
    }
    default_per_user_dir()
}

#[cfg(target_os = "windows")]
fn default_per_user_dir() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join("anubis-extractor");
    }
    std::env::temp_dir().join("anubis-extractor")
}

#[cfg(target_os = "macos")]
fn default_per_user_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join("Library/Caches/anubis-extractor");
    }
    std::env::temp_dir().join("anubis-extractor")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_per_user_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("anubis-extractor");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache/anubis-extractor");
    }
    std::env::temp_dir().join("anubis-extractor")
}

#[cfg(test)]
mod tests {
    use super::*;

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn env_override_takes_precedence_over_default() {
        let _guard = LOCK.lock().unwrap();
        std::env::set_var("ANUBIS_EXTRACTOR_CACHE_DIR", "X:\\custom");
        let dir = resolve_cache_dir(None);
        std::env::remove_var("ANUBIS_EXTRACTOR_CACHE_DIR");
        assert_eq!(dir, std::path::PathBuf::from("X:\\custom"));
    }

    #[test]
    fn explicit_arg_wins_over_env() {
        let _guard = LOCK.lock().unwrap();
        std::env::set_var("ANUBIS_EXTRACTOR_CACHE_DIR", "X:\\loses");
        let dir = resolve_cache_dir(Some(std::path::PathBuf::from("X:\\wins")));
        std::env::remove_var("ANUBIS_EXTRACTOR_CACHE_DIR");
        assert_eq!(dir, std::path::PathBuf::from("X:\\wins"));
    }

    #[test]
    fn default_is_per_user_dir_when_env_unset() {
        let _guard = LOCK.lock().unwrap();
        std::env::remove_var("ANUBIS_EXTRACTOR_CACHE_DIR");
        let dir = resolve_cache_dir(None);
        assert!(dir.to_string_lossy().contains("anubis-extractor"));
    }
}
