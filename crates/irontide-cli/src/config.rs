//! TOML configuration file support.
//!
//! Provides a user-friendly TOML configuration file format that exposes a
//! curated subset of [`irontide::session::Settings`]. The full 102-field
//! `Settings` struct is an internal engine type; `ConfigFile` presents only
//! the knobs a typical user needs, with friendlier names.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use irontide::session::Settings;

// ── TOML section structs ────────────────────────────────────────────

/// `[session]` — core session parameters.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Default download directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_dir: Option<PathBuf>,
    /// TCP listen port for incoming peer connections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listen_port: Option<u16>,
    /// Enable Kademlia DHT peer discovery (BEP 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_dht: Option<bool>,
    /// Enable Local Service Discovery (BEP 14).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_lsd: Option<bool>,
    /// Enable Peer Exchange (BEP 11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_pex: Option<bool>,
    /// Number of tokio worker threads (0 = auto).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workers: Option<usize>,
    /// Pin tokio worker threads to CPU cores.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_cores: Option<bool>,
}

/// `[api]` — HTTP API settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApiConfig {
    /// HTTP API bind address (e.g. "127.0.0.1" or "0.0.0.0").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind: Option<String>,
    /// HTTP API port (0 = disabled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// `[limits]` — rate limits, peer caps, and queue management.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LimitsConfig {
    /// Global download rate limit in bytes/sec (0 = unlimited).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_download_rate_bps: Option<u64>,
    /// Global upload rate limit in bytes/sec (0 = unlimited).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_upload_rate_bps: Option<u64>,
    /// Maximum peer connections per torrent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_peers_per_torrent: Option<usize>,
    /// Maximum concurrent auto-managed downloading torrents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_active_downloads: Option<i32>,
    /// Maximum concurrent auto-managed seeding torrents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_active_uploads: Option<i32>,
}

// ── Top-level config ────────────────────────────────────────────────

/// User-facing TOML configuration file.
///
/// All sections and fields are optional — only specified values override
/// the engine defaults. An empty file or absent file produces
/// `Settings::default()`.
///
/// # Example
///
/// ```toml
/// [session]
/// download_dir = "~/Downloads"
/// listen_port = 42020
/// workers = 4
///
/// [api]
/// bind = "127.0.0.1"
/// port = 9080
///
/// [limits]
/// max_download_rate_bps = 0
/// max_peers_per_torrent = 200
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Core session parameters.
    #[serde(default, skip_serializing_if = "section_is_default")]
    pub session: SessionConfig,
    /// HTTP API settings.
    #[serde(default, skip_serializing_if = "section_is_default")]
    pub api: ApiConfig,
    /// Rate limits and peer caps.
    #[serde(default, skip_serializing_if = "section_is_default")]
    pub limits: LimitsConfig,
}

/// Returns `true` when every field in a section is `None`, allowing
/// `skip_serializing_if` to omit the entire `[section]` header.
fn section_is_default<T: Serialize + Default + PartialEq>(val: &T) -> bool {
    *val == T::default()
}

// ── ConfigFile ↔ Settings conversions ───────────────────────────────

impl ConfigFile {
    /// Apply this config file's values on top of `Settings::default()`.
    ///
    /// Only fields explicitly set in the TOML override the defaults;
    /// unset fields (`None`) are left at their default values.
    #[must_use]
    pub fn to_settings_overrides(&self) -> Settings {
        let mut s = Settings::default();

        // [session]
        if let Some(ref dir) = self.session.download_dir {
            s.download_dir = dir.clone();
        }
        if let Some(port) = self.session.listen_port {
            s.listen_port = port;
        }
        if let Some(dht) = self.session.enable_dht {
            s.enable_dht = dht;
        }
        if let Some(lsd) = self.session.enable_lsd {
            s.enable_lsd = lsd;
        }
        if let Some(pex) = self.session.enable_pex {
            s.enable_pex = pex;
        }
        if let Some(workers) = self.session.workers {
            s.runtime_worker_threads = workers;
        }
        if let Some(pin) = self.session.pin_cores {
            s.pin_cores = pin;
        }

        // [limits]
        if let Some(rate) = self.limits.max_download_rate_bps {
            s.download_rate_limit = rate;
        }
        if let Some(rate) = self.limits.max_upload_rate_bps {
            s.upload_rate_limit = rate;
        }
        if let Some(max) = self.limits.max_peers_per_torrent {
            s.max_peers_per_torrent = max;
        }
        if let Some(n) = self.limits.max_active_downloads {
            s.active_downloads = n;
        }
        if let Some(n) = self.limits.max_active_uploads {
            s.active_seeds = n;
        }

        s
    }

    /// Round-trip: produce a `ConfigFile` from a `Settings` reference.
    ///
    /// This is used by `config show` to display the current effective
    /// configuration. All fields are populated (no `None` values).
    #[must_use]
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            session: SessionConfig {
                download_dir: Some(settings.download_dir.clone()),
                listen_port: Some(settings.listen_port),
                enable_dht: Some(settings.enable_dht),
                enable_lsd: Some(settings.enable_lsd),
                enable_pex: Some(settings.enable_pex),
                workers: Some(settings.runtime_worker_threads),
                pin_cores: Some(settings.pin_cores),
            },
            api: ApiConfig {
                // API settings are not part of Settings — they live in the
                // CLI layer. Populated as None here; callers fill them in.
                bind: None,
                port: None,
            },
            limits: LimitsConfig {
                max_download_rate_bps: Some(settings.download_rate_limit),
                max_upload_rate_bps: Some(settings.upload_rate_limit),
                max_peers_per_torrent: Some(settings.max_peers_per_torrent),
                max_active_downloads: Some(settings.active_downloads),
                max_active_uploads: Some(settings.active_seeds),
            },
        }
    }
}

// ── Config path resolution ──────────────────────────────────────────

/// Resolve the configuration file path.
///
/// Precedence:
/// 1. If `explicit` is `Some`, use it (with tilde expansion).
/// 2. Otherwise, use the platform config directory:
///    `$XDG_CONFIG_HOME/irontide/config.toml` (Linux),
///    `~/Library/Application Support/irontide/config.toml` (macOS),
///    `%APPDATA%/irontide/config.toml` (Windows).
/// 3. Fallback (containers/CI where `ProjectDirs` returns `None`):
///    `./.irontide/config.toml`.
#[must_use]
pub fn resolve_config_path(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        let lossy = path.to_string_lossy();
        let expanded = shellexpand::tilde(&lossy);
        return PathBuf::from(expanded.as_ref());
    }

    if let Some(dirs) = directories::ProjectDirs::from("", "", "irontide") {
        dirs.config_dir().join("config.toml")
    } else {
        PathBuf::from("./.irontide/config.toml")
    }
}

/// Load session settings from the TOML config file.
///
/// This is a placeholder for Phase 1 — it resolves the config path,
/// parses the TOML into a [`ConfigFile`], and applies overrides on top
/// of [`Settings::default()`]. The full Figment pipeline (env vars,
/// CLI merging) is wired in Phase 3.
///
/// # Errors
///
/// Returns an error if the config file exists but cannot be read or
/// parsed as valid TOML.
pub fn load(config_path: Option<&Path>) -> Result<Settings> {
    let path = resolve_config_path(config_path);

    if !path.exists() {
        return Ok(Settings::default());
    }

    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;

    let config: ConfigFile = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file: {}", path.display()))?;

    Ok(config.to_settings_overrides())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_produces_defaults() {
        let config = ConfigFile::default();
        let settings = config.to_settings_overrides();
        let defaults = Settings::default();
        assert_eq!(settings.listen_port, defaults.listen_port);
        assert_eq!(settings.download_dir, defaults.download_dir);
        assert_eq!(settings.enable_dht, defaults.enable_dht);
        assert_eq!(settings.runtime_worker_threads, defaults.runtime_worker_threads);
    }

    #[test]
    fn partial_config_overrides_only_specified_fields() {
        let toml_str = r#"
[session]
listen_port = 12345
workers = 2

[limits]
max_peers_per_torrent = 64
"#;
        let config: ConfigFile = toml::from_str(toml_str).expect("valid TOML");
        let settings = config.to_settings_overrides();

        assert_eq!(settings.listen_port, 12345);
        assert_eq!(settings.runtime_worker_threads, 2);
        assert_eq!(settings.max_peers_per_torrent, 64);
        // Unspecified fields keep defaults.
        assert!(settings.enable_dht);
        assert_eq!(settings.download_rate_limit, 0);
    }

    #[test]
    fn round_trip_settings_to_config() {
        let defaults = Settings::default();
        let config = ConfigFile::from_settings(&defaults);
        assert_eq!(config.session.listen_port, Some(defaults.listen_port));
        assert_eq!(config.session.workers, Some(defaults.runtime_worker_threads));
        assert_eq!(config.limits.max_download_rate_bps, Some(defaults.download_rate_limit));
        assert_eq!(config.limits.max_active_downloads, Some(defaults.active_downloads));
        assert_eq!(config.limits.max_active_uploads, Some(defaults.active_seeds));
    }

    #[test]
    fn resolve_config_path_explicit_tilde() {
        let path = resolve_config_path(Some(Path::new("~/myconfig.toml")));
        // Tilde should be expanded — the result must not start with "~/"
        // (unless $HOME is literally "~", which would be pathological).
        assert!(
            !path.to_string_lossy().starts_with("~/"),
            "tilde should be expanded: {path:?}"
        );
    }

    #[test]
    fn resolve_config_path_default_uses_project_dirs() {
        let path = resolve_config_path(None);
        // On most systems ProjectDirs succeeds; verify it ends with config.toml.
        assert!(
            path.to_string_lossy().ends_with("config.toml"),
            "default path should end with config.toml: {path:?}"
        );
    }

    #[test]
    fn load_nonexistent_returns_defaults() {
        let settings = load(Some(Path::new("/tmp/irontide-test-nonexistent-42/config.toml")))
            .expect("should succeed for nonexistent file");
        let defaults = Settings::default();
        assert_eq!(settings.listen_port, defaults.listen_port);
    }

    #[test]
    fn config_file_toml_serialization_round_trip() {
        let original = ConfigFile {
            session: SessionConfig {
                download_dir: Some(PathBuf::from("/tmp/downloads")),
                listen_port: Some(9999),
                enable_dht: Some(false),
                enable_lsd: None,
                enable_pex: None,
                workers: Some(4),
                pin_cores: Some(false),
            },
            api: ApiConfig {
                bind: Some("0.0.0.0".into()),
                port: Some(8080),
            },
            limits: LimitsConfig {
                max_download_rate_bps: Some(1_048_576),
                max_upload_rate_bps: Some(524_288),
                max_peers_per_torrent: Some(200),
                max_active_downloads: Some(5),
                max_active_uploads: Some(10),
            },
        };

        let serialized = toml::to_string_pretty(&original).expect("serialize");
        let deserialized: ConfigFile = toml::from_str(&serialized).expect("deserialize");

        assert_eq!(deserialized.session.listen_port, Some(9999));
        assert_eq!(deserialized.session.enable_dht, Some(false));
        assert_eq!(deserialized.session.workers, Some(4));
        assert_eq!(deserialized.api.bind, Some("0.0.0.0".into()));
        assert_eq!(deserialized.api.port, Some(8080));
        assert_eq!(deserialized.limits.max_download_rate_bps, Some(1_048_576));
        assert_eq!(deserialized.limits.max_active_uploads, Some(10));
    }

    #[test]
    fn empty_sections_omitted_in_serialization() {
        let config = ConfigFile {
            session: SessionConfig {
                listen_port: Some(42020),
                ..Default::default()
            },
            ..Default::default()
        };
        let serialized = toml::to_string_pretty(&config).expect("serialize");
        // api and limits should be omitted since they are all None.
        assert!(!serialized.contains("[api]"), "empty api section should be omitted");
        assert!(!serialized.contains("[limits]"), "empty limits section should be omitted");
        assert!(serialized.contains("[session]"), "non-empty session section should be present");
    }
}
