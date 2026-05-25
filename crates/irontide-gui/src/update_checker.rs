//! Auto-update checker (M209).
//!
//! Periodically queries the Codeberg releases API for newer versions.
//! When a newer release is found, pushes an update notification to the
//! Slint UI. Does not download or apply updates automatically — the
//! user clicks through to the release page.

use std::time::Duration;

use serde::Deserialize;

const RELEASES_URL: &str = "https://codeberg.org/api/v1/repos/alan090/irontide/releases?limit=1";
const CHECK_INTERVAL: Duration = Duration::from_hours(24);
const INITIAL_DELAY: Duration = Duration::from_mins(1);

/// A release from the Codeberg API.
#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
}

/// Parsed semantic version for comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    /// Major version number.
    pub major: u32,
    /// Minor version number.
    pub minor: u32,
    /// Patch version number.
    pub patch: u32,
}

impl SemVer {
    /// Parse a version string like "v0.208.0" or "0.208.0".
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let mut parts = s.split('.');
        Some(Self {
            major: parts.next()?.parse().ok()?,
            minor: parts.next()?.parse().ok()?,
            patch: parts.next()?.parse().ok()?,
        })
    }

    /// Returns `true` if `self` is strictly newer than `other`.
    #[must_use]
    pub fn is_newer_than(&self, other: &Self) -> bool {
        (self.major, self.minor, self.patch) > (other.major, other.minor, other.patch)
    }
}

/// Information about an available update.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// The new version string (e.g. "v0.209.0").
    pub version: String,
    /// URL to the release page.
    pub url: String,
}

/// Check for an update using a blocking HTTP client (no Tokio runtime needed).
fn check_for_update_blocking(current_version: &str) -> Option<UpdateInfo> {
    let current = SemVer::parse(current_version)?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(format!("irontide/{current_version}"))
        .build()
        .ok()?;

    let releases: Vec<Release> = client.get(RELEASES_URL).send().ok()?.json().ok()?;

    let latest = releases.into_iter().find(|r| !r.prerelease && !r.draft)?;
    let latest_ver = SemVer::parse(&latest.tag_name)?;

    if latest_ver.is_newer_than(&current) {
        Some(UpdateInfo {
            version: latest.tag_name,
            url: latest.html_url,
        })
    } else {
        None
    }
}

/// Spawn a background thread that periodically checks for updates and
/// pushes notifications to the Slint UI.
pub fn spawn_update_checker(weak: slint::Weak<crate::MainWindow>) {
    let version = env!("CARGO_PKG_VERSION").to_string();
    std::thread::Builder::new()
        .name("update-checker".into())
        .spawn(move || {
            std::thread::sleep(INITIAL_DELAY);
            loop {
                if let Some(info) = check_for_update_blocking(&version) {
                    let weak = weak.clone();
                    let msg: slint::SharedString =
                        format!("Update available: {} — {}", info.version, info.url).into();
                    let _ = weak.upgrade_in_event_loop(move |win| {
                        win.set_update_available(true);
                        win.set_update_version(info.version.as_str().into());
                        win.set_update_url(info.url.as_str().into());
                        win.set_update_message(msg);
                    });
                }
                std::thread::sleep(CHECK_INTERVAL);
            }
        })
        .expect("failed to spawn update checker thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_with_v_prefix() {
        let v = SemVer::parse("v0.208.0").unwrap();
        assert_eq!(
            v,
            SemVer {
                major: 0,
                minor: 208,
                patch: 0
            }
        );
    }

    #[test]
    fn parse_version_without_prefix() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(
            v,
            SemVer {
                major: 1,
                minor: 2,
                patch: 3
            }
        );
    }

    #[test]
    fn parse_invalid_version() {
        assert!(SemVer::parse("not-a-version").is_none());
        assert!(SemVer::parse("").is_none());
        assert!(SemVer::parse("1.2").is_none());
    }

    #[test]
    fn newer_version_detected() {
        let old = SemVer::parse("0.208.0").unwrap();
        let new = SemVer::parse("0.209.0").unwrap();
        assert!(new.is_newer_than(&old));
        assert!(!old.is_newer_than(&new));
    }

    #[test]
    fn same_version_not_newer() {
        let v = SemVer::parse("0.208.0").unwrap();
        assert!(!v.is_newer_than(&v));
    }

    #[test]
    fn patch_version_newer() {
        let old = SemVer::parse("0.208.0").unwrap();
        let new = SemVer::parse("0.208.1").unwrap();
        assert!(new.is_newer_than(&old));
    }

    #[test]
    fn major_version_newer() {
        let old = SemVer::parse("0.999.999").unwrap();
        let new = SemVer::parse("1.0.0").unwrap();
        assert!(new.is_newer_than(&old));
    }
}
