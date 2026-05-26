//! First-run wizard detection and persistence (M210).
//!
//! Checks whether the user has completed the initial setup wizard.
//! The sentinel file `~/.config/irontide/.first-run-complete` marks
//! that the wizard has been completed.
//!
//! M220 adds validators for the wizard's download-directory and listen-port
//! fields. `validate_download_dir` probes writability by atomically creating
//! a uniquely-named file via `O_CREAT|O_EXCL` semantics, so it never
//! truncates an existing user file.

use std::path::{Path, PathBuf};

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

/// Validate that `path` exists and is writable.
///
/// Returns `Ok(())` if `path` is a directory we can create and remove a file
/// in. Uses `O_CREAT|O_EXCL` semantics on a uniquely-named probe so we
/// never truncate an existing user file — even if a stale probe from a
/// crashed earlier run collides, we fail closed with `"Directory is not
/// writable."` rather than destroy data.
///
/// # Errors
/// Returns `Err("Directory does not exist.")` if the path is not a directory
/// (covers missing paths, files, broken symlinks). Returns
/// `Err("Directory is not writable.")` if probe creation fails.
pub fn validate_download_dir(path: &str) -> Result<(), &'static str> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err("Directory does not exist.");
    }
    let probe = p.join(format!(".irontide-probe-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(_) => Err("Directory is not writable."),
    }
}

/// Validate that `s` parses as a TCP/UDP port in the user-allowed range.
///
/// The wizard restricts to `1024..=65535`: below 1024 requires root on
/// Linux/Mac, and `0` (OS-assigned) isn't currently supported by the
/// session listener path.
///
/// # Errors
/// Returns `Err("Port must be a number.")` on parse failure (empty, "abc")
/// and `Err("Port must be 1024-65535.")` if out of range.
pub fn validate_listen_port(s: &str) -> Result<u16, &'static str> {
    let n: u32 = s.parse().map_err(|_| "Port must be a number.")?;
    if !(1024..=65535).contains(&n) {
        return Err("Port must be 1024-65535.");
    }
    // `n` is in 1024..=65535, which fits in u16 by construction.
    Ok(u16::try_from(n).expect("range-checked above"))
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

    #[test]
    fn validate_dir_existing_writable_ok() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(validate_download_dir(tmp.path().to_str().unwrap()).is_ok());
    }

    #[test]
    fn validate_dir_missing_returns_err() {
        let err = validate_download_dir("/nonexistent/path/should/not/exist/abc123")
            .expect_err("missing dir should fail");
        assert_eq!(err, "Directory does not exist.");
    }

    #[test]
    fn validate_dir_file_path_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("not-a-dir.txt");
        std::fs::write(&file, b"hi").unwrap();
        let err = validate_download_dir(file.to_str().unwrap())
            .expect_err("file path should fail");
        assert_eq!(err, "Directory does not exist.");
    }

    #[cfg(unix)]
    #[test]
    fn validate_dir_non_writable_returns_err() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let ro = tmp.path().join("readonly");
        std::fs::create_dir(&ro).unwrap();
        let mut perms = std::fs::metadata(&ro).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&ro, perms.clone()).unwrap();

        // Skip when running as root — root bypasses mode bits.
        if nix_is_root() {
            // Restore writable so tempdir cleanup succeeds.
            perms.set_mode(0o755);
            std::fs::set_permissions(&ro, perms).unwrap();
            return;
        }

        let result = validate_download_dir(ro.to_str().unwrap());

        // Restore writable so tempdir cleanup succeeds.
        let mut wperms = std::fs::metadata(&ro).unwrap().permissions();
        wperms.set_mode(0o755);
        std::fs::set_permissions(&ro, wperms).unwrap();

        assert_eq!(result.expect_err("readonly dir should fail"), "Directory is not writable.");
    }

    #[cfg(unix)]
    fn nix_is_root() -> bool {
        // SAFETY: `geteuid` is a leaf syscall with no preconditions.
        unsafe { libc::geteuid() == 0 }
    }

    #[test]
    fn validate_port_in_range_ok() {
        assert_eq!(validate_listen_port("1024"), Ok(1024));
        assert_eq!(validate_listen_port("6881"), Ok(6881));
        assert_eq!(validate_listen_port("65535"), Ok(65535));
    }

    #[test]
    fn validate_port_below_range_err() {
        assert_eq!(
            validate_listen_port("1023"),
            Err("Port must be 1024-65535.")
        );
        assert_eq!(validate_listen_port("0"), Err("Port must be 1024-65535."));
    }

    #[test]
    fn validate_port_above_range_err() {
        assert_eq!(
            validate_listen_port("65536"),
            Err("Port must be 1024-65535.")
        );
        assert_eq!(
            validate_listen_port("99999"),
            Err("Port must be 1024-65535.")
        );
    }

    #[test]
    fn validate_port_non_numeric_err() {
        assert_eq!(validate_listen_port(""), Err("Port must be a number."));
        assert_eq!(validate_listen_port("abc"), Err("Port must be a number."));
        assert_eq!(
            validate_listen_port("123abc"),
            Err("Port must be a number.")
        );
    }
}
