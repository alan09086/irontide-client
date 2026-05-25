//! First-run wizard detection and persistence (M210).
//!
//! Checks whether the user has completed the initial setup wizard.
//! The sentinel file `~/.config/irontide/.first-run-complete` marks
//! that the wizard has been completed.

use std::path::PathBuf;

/// Returns `true` if this is the first run (wizard not yet completed).
#[must_use]
pub fn is_first_run() -> bool {
    !sentinel_path().is_file()
}

/// Mark the first-run wizard as completed.
pub fn mark_complete() {
    let path = sentinel_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, b"done\n");
}

fn sentinel_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "irontide").map_or_else(
        || PathBuf::from("/tmp/irontide-first-run-complete"),
        |d| d.config_dir().join(".first-run-complete"),
    )
}

/// Default download directory for the wizard.
#[must_use]
pub fn default_download_dir() -> String {
    directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            format!("{home}/Downloads")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_download_dir_is_non_empty() {
        let dir = default_download_dir();
        assert!(!dir.is_empty());
    }

    #[test]
    fn sentinel_path_is_under_config() {
        let path = sentinel_path();
        let s = path.to_string_lossy();
        assert!(
            s.contains("irontide"),
            "sentinel path should contain 'irontide': {s}"
        );
    }

    #[test]
    fn mark_and_detect() {
        let tmp = tempfile::tempdir().unwrap();
        let sentinel = tmp.path().join(".first-run-complete");

        assert!(!sentinel.is_file());

        let parent = sentinel.parent().unwrap();
        std::fs::create_dir_all(parent).unwrap();
        std::fs::write(&sentinel, b"done\n").unwrap();

        assert!(sentinel.is_file());
    }
}
