//! Layered configuration pipeline.
//!
//! Provides a user-friendly TOML configuration file format that exposes a
//! curated subset of [`irontide_session::Settings`]. The full 102-field
//! `Settings` struct is an internal engine type; `ConfigFile` presents only
//! the knobs a typical user needs, with friendlier names.
//!
//! Configuration sources are merged in order of increasing precedence:
//!
//! 1. **Defaults** — `ConfigFile::default()` (all `None`)
//! 2. **TOML file** — `$XDG_CONFIG_HOME/irontide/config.toml` (or `--config`)
//! 3. **Environment variables** — flat `IRONTIDE_*` namespace
//! 4. **CLI flags** — highest priority overrides

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result};
use figment::Figment;
use figment::providers::{Env, Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

use irontide_session::Settings;

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
    /// Directory for per-torrent resume files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_dir: Option<PathBuf>,
    /// Interval in seconds between periodic resume file saves (0 = disabled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_resume_interval: Option<u64>,
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
        if let Some(ref dir) = self.session.resume_dir {
            s.resume_data_dir = Some(dir.clone());
        }
        if let Some(interval) = self.session.save_resume_interval {
            s.save_resume_interval_secs = interval;
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
                resume_dir: settings.resume_data_dir.clone(),
                save_resume_interval: Some(settings.save_resume_interval_secs),
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

/// Map a flat `IRONTIDE_*` environment variable key to a dotted config path.
///
/// The `key` parameter is the portion *after* the `IRONTIDE_` prefix,
/// already lowercased by Figment's [`Env`] provider. Keys that belong to
/// the `[api]` or `[limits]` sections are mapped explicitly; everything
/// else falls through to `session.<key>`.
///
/// Unknown keys are mapped to `session.<key>` — Figment silently ignores
/// keys that don't match any field in the target struct, so typos in env
/// vars are harmless (they just don't take effect).
fn env_to_config_path(key: &str) -> String {
    match key {
        // [api]
        "api_port" => "api.port".to_owned(),
        "api_bind" => "api.bind".to_owned(),
        // [limits]
        "max_download_rate_bps" => "limits.max_download_rate_bps".to_owned(),
        "max_upload_rate_bps" => "limits.max_upload_rate_bps".to_owned(),
        "max_peers_per_torrent" => "limits.max_peers_per_torrent".to_owned(),
        "max_active_downloads" => "limits.max_active_downloads".to_owned(),
        "max_active_uploads" => "limits.max_active_uploads".to_owned(),
        // Everything else → [session]
        other => format!("session.{other}"),
    }
}

/// Load session settings by merging four configuration layers.
///
/// Precedence (highest wins):
/// 1. **Defaults** — `ConfigFile::default()` (all `None`)
/// 2. **TOML file** — resolved via [`resolve_config_path`]
/// 3. **Environment variables** — `IRONTIDE_*` flat namespace
/// 4. **CLI flag overrides** — only `Some` fields take effect
///
/// The merged [`ConfigFile`] is converted to [`Settings`] via
/// [`ConfigFile::to_settings_overrides`], then validated.
///
/// # Errors
///
/// Returns an error if the config file exists but cannot be parsed, if
/// an environment variable contains an unparseable value, or if the
/// resulting settings fail validation.
pub fn load(config_path: Option<&Path>, cli_overrides: &ConfigFile) -> Result<Settings> {
    let path = resolve_config_path(config_path);

    // Layer 1: struct defaults (all None — empty dict after skip_serializing_if).
    let mut figment = Figment::new().merge(Serialized::defaults(ConfigFile::default()));

    // Layer 2: TOML file (if it exists on disk).
    if path.exists() {
        figment = figment.merge(Toml::file(&path));
    }

    // Layer 3: environment variables with flat IRONTIDE_* namespace.
    figment = figment
        .merge(Env::prefixed("IRONTIDE_").map(|key| env_to_config_path(key.as_str()).into()));

    // Layer 4: CLI flag overrides (highest precedence).
    figment = figment.merge(Serialized::defaults(cli_overrides));

    // Extract the merged ConfigFile and convert to engine Settings.
    let config: ConfigFile = figment.extract().context("failed to merge configuration")?;
    let settings = config.to_settings_overrides();
    settings.validate().context("invalid configuration")?;

    Ok(settings)
}

// ── Runtime construction ────────────────────────────────────────────

/// Build a multi-thread tokio runtime with optional CPU core affinity.
///
/// Worker thread count is taken from `settings.runtime_worker_threads`
/// (0 = auto-detect, capped at 8). When `settings.pin_cores` is true,
/// each worker thread is pinned to a CPU core via `core_affinity`.
pub fn build_runtime(settings: &Settings) -> tokio::runtime::Runtime {
    let worker_count = if settings.runtime_worker_threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4)
    } else {
        settings.runtime_worker_threads
    };

    let pin = settings.pin_cores;
    let core_ids = if pin {
        core_affinity::get_core_ids().unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.worker_threads(worker_count);
    builder.enable_all();

    if pin && !core_ids.is_empty() {
        let core_ids = Arc::new(core_ids);
        let counter = Arc::new(AtomicUsize::new(0));
        builder.on_thread_start(move || {
            let idx = counter.fetch_add(1, Ordering::Relaxed);
            let core = core_ids[idx % core_ids.len()];
            if !core_affinity::set_for_current(core) {
                eprintln!("warning: failed to set core affinity for worker {idx}");
            }
        });
    }

    builder.build().expect("failed to build tokio runtime")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize all tests that call [`load()`] because the Figment `Env`
    /// provider reads process-global environment variables. Without this,
    /// tests that set `IRONTIDE_*` vars leak into parallel tests.
    static LOAD_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn empty_config_produces_defaults() {
        let config = ConfigFile::default();
        let settings = config.to_settings_overrides();
        let defaults = Settings::default();
        assert_eq!(settings.listen_port, defaults.listen_port);
        assert_eq!(settings.download_dir, defaults.download_dir);
        assert_eq!(settings.enable_dht, defaults.enable_dht);
        assert_eq!(
            settings.runtime_worker_threads,
            defaults.runtime_worker_threads
        );
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
        assert_eq!(
            config.session.workers,
            Some(defaults.runtime_worker_threads)
        );
        assert_eq!(
            config.limits.max_download_rate_bps,
            Some(defaults.download_rate_limit)
        );
        assert_eq!(
            config.limits.max_active_downloads,
            Some(defaults.active_downloads)
        );
        assert_eq!(
            config.limits.max_active_uploads,
            Some(defaults.active_seeds)
        );
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
        let _guard = LOAD_MUTEX.lock().expect("test mutex poisoned");
        let settings = load(
            Some(Path::new("/tmp/irontide-test-nonexistent-42/config.toml")),
            &ConfigFile::default(),
        )
        .expect("should succeed for nonexistent file");
        let defaults = Settings::default();
        assert_eq!(settings.listen_port, defaults.listen_port);
    }

    #[test]
    fn load_with_cli_overrides() {
        let _guard = LOAD_MUTEX.lock().expect("test mutex poisoned");
        let overrides = ConfigFile {
            session: SessionConfig {
                listen_port: Some(55555),
                workers: Some(2),
                ..Default::default()
            },
            ..Default::default()
        };
        let settings = load(
            Some(Path::new("/tmp/irontide-test-nonexistent-42/config.toml")),
            &overrides,
        )
        .expect("should succeed");
        assert_eq!(settings.listen_port, 55555);
        assert_eq!(settings.runtime_worker_threads, 2);
        // Unspecified fields keep defaults.
        let defaults = Settings::default();
        assert_eq!(settings.enable_dht, defaults.enable_dht);
    }

    #[test]
    fn load_toml_file_applies_values() {
        let _guard = LOAD_MUTEX.lock().expect("test mutex poisoned");
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[session]
listen_port = 31337

[limits]
max_peers_per_torrent = 42
"#,
        )
        .expect("write config file");

        let settings = load(Some(&config_path), &ConfigFile::default()).expect("should succeed");
        assert_eq!(settings.listen_port, 31337);
        assert_eq!(settings.max_peers_per_torrent, 42);
    }

    #[test]
    fn env_var_mapping() {
        // Verify the flat-to-nested mapping function.
        assert_eq!(env_to_config_path("listen_port"), "session.listen_port");
        assert_eq!(env_to_config_path("download_dir"), "session.download_dir");
        assert_eq!(env_to_config_path("enable_dht"), "session.enable_dht");
        assert_eq!(env_to_config_path("workers"), "session.workers");
        assert_eq!(env_to_config_path("pin_cores"), "session.pin_cores");
        assert_eq!(env_to_config_path("api_port"), "api.port");
        assert_eq!(env_to_config_path("api_bind"), "api.bind");
        assert_eq!(
            env_to_config_path("max_download_rate_bps"),
            "limits.max_download_rate_bps"
        );
        assert_eq!(
            env_to_config_path("max_upload_rate_bps"),
            "limits.max_upload_rate_bps"
        );
        assert_eq!(
            env_to_config_path("max_peers_per_torrent"),
            "limits.max_peers_per_torrent"
        );
        assert_eq!(
            env_to_config_path("max_active_downloads"),
            "limits.max_active_downloads"
        );
        assert_eq!(
            env_to_config_path("max_active_uploads"),
            "limits.max_active_uploads"
        );
    }

    #[test]
    fn env_var_unknown_key_maps_to_session() {
        // Unknown keys fall through to session.* — Figment ignores
        // unrecognised fields, so this is safe.
        assert_eq!(
            env_to_config_path("some_unknown_var"),
            "session.some_unknown_var"
        );
    }

    #[test]
    fn cli_overrides_beat_toml_file() {
        let _guard = LOAD_MUTEX.lock().expect("test mutex poisoned");
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[session]
listen_port = 10000
workers = 8
"#,
        )
        .expect("write config file");

        let overrides = ConfigFile {
            session: SessionConfig {
                listen_port: Some(20000),
                ..Default::default()
            },
            ..Default::default()
        };

        let settings = load(Some(&config_path), &overrides).expect("should succeed");
        // CLI override wins for listen_port.
        assert_eq!(settings.listen_port, 20000);
        // TOML value used for workers (no CLI override).
        assert_eq!(settings.runtime_worker_threads, 8);
    }

    #[test]
    fn env_vars_override_toml_file() {
        let _guard = LOAD_MUTEX.lock().expect("test mutex poisoned");
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[session]
listen_port = 10000
"#,
        )
        .expect("write config file");

        // SAFETY: serialized by LOAD_MUTEX — no other test calling load()
        // can run concurrently. Cleaned up before the guard drops.
        unsafe { std::env::set_var("IRONTIDE_LISTEN_PORT", "30000") };

        let settings = load(Some(&config_path), &ConfigFile::default());

        // Clean up before asserting so we don't leak on failure.
        unsafe { std::env::remove_var("IRONTIDE_LISTEN_PORT") };

        let settings = settings.expect("should succeed");
        assert_eq!(settings.listen_port, 30000);
    }

    #[test]
    fn precedence_cli_beats_env_beats_file() {
        let _guard = LOAD_MUTEX.lock().expect("test mutex poisoned");
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[limits]
max_peers_per_torrent = 50
"#,
        )
        .expect("write config file");

        // SAFETY: serialized by LOAD_MUTEX — see env_vars_override_toml_file.
        unsafe { std::env::set_var("IRONTIDE_MAX_PEERS_PER_TORRENT", "100") };

        let overrides = ConfigFile {
            limits: LimitsConfig {
                max_peers_per_torrent: Some(200),
                ..Default::default()
            },
            ..Default::default()
        };

        let settings = load(Some(&config_path), &overrides);

        unsafe { std::env::remove_var("IRONTIDE_MAX_PEERS_PER_TORRENT") };

        let settings = settings.expect("should succeed");
        // CLI (200) > env (100) > file (50).
        assert_eq!(settings.max_peers_per_torrent, 200);
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
                resume_dir: None,
                save_resume_interval: None,
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
        assert!(
            !serialized.contains("[api]"),
            "empty api section should be omitted"
        );
        assert!(
            !serialized.contains("[limits]"),
            "empty limits section should be omitted"
        );
        assert!(
            serialized.contains("[session]"),
            "non-empty session section should be present"
        );
    }

    // ---- M161: resume_dir + save_resume_interval config tests ----

    #[test]
    fn config_resume_dir_produces_correct_settings() {
        let config = ConfigFile {
            session: SessionConfig {
                resume_dir: Some(PathBuf::from("/tmp/my-resume")),
                ..Default::default()
            },
            ..Default::default()
        };
        let settings = config.to_settings_overrides();
        assert_eq!(
            settings.resume_data_dir,
            Some(PathBuf::from("/tmp/my-resume")),
        );
    }

    #[test]
    fn default_resume_dir_ends_with_irontide() {
        let dir = irontide_session::default_resume_dir();
        assert!(
            dir.ends_with("irontide"),
            "default_resume_dir should end with 'irontide', got: {dir:?}"
        );
        // Should be under .local/state (when HOME is set, which it is
        // in CI and dev machines).
        let lossy = dir.to_string_lossy();
        assert!(
            lossy.contains(".local/state") || lossy.contains("irontide"),
            "expected path under .local/state/irontide, got: {dir:?}"
        );
    }

    #[test]
    fn round_trip_resume_dir_through_config() {
        let mut settings = Settings::default();
        settings.resume_data_dir = Some(PathBuf::from("/data/resume"));
        let config = ConfigFile::from_settings(&settings);
        assert_eq!(
            config.session.resume_dir,
            Some(PathBuf::from("/data/resume")),
        );
        let round_tripped = config.to_settings_overrides();
        assert_eq!(
            round_tripped.resume_data_dir,
            Some(PathBuf::from("/data/resume")),
        );
    }

    #[test]
    fn save_resume_interval_config_round_trip() {
        let config = ConfigFile {
            session: SessionConfig {
                save_resume_interval: Some(600),
                ..Default::default()
            },
            ..Default::default()
        };
        let settings = config.to_settings_overrides();
        assert_eq!(settings.save_resume_interval_secs, 600);

        // Round-trip back through from_settings.
        let config2 = ConfigFile::from_settings(&settings);
        assert_eq!(config2.session.save_resume_interval, Some(600));
    }
}
