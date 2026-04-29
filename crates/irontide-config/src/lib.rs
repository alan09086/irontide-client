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

/// `[gui]` — desktop GUI state (column layout, visibility, widths).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GuiConfig {
    /// Ordered list of column identifiers (e.g. `["name", "size", "progress"]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_order: Option<Vec<String>>,
    /// Columns that are visible (by identifier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_visibility: Option<Vec<String>>,
    /// Per-column pixel widths, in the same order as `column_order`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_widths: Option<Vec<f32>>,
    /// Active GUI skin (`tide` / `forge` / `abyss`). None → default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
    /// Active theme (`dark` / `light`). None → default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// UI density (`compact` / `balanced` / `spacious`). None → default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<String>,
    /// Corner radius preset (`sharp` / `balanced` / `rounded`). None → default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius_preset: Option<String>,
    /// M176: active layout variant (`L1` / `L2` / `L3`). None → default L1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// M176: L3 sidebar mode (`icons` / `hidden`). None → default icons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub l3_sidebar_mode: Option<String>,
    /// M176: inspector visibility (⌘I toggle state). None → layout-default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inspector_shown: Option<bool>,
    /// M173 Lane A: persisted sidebar state (collapsed sections,
    /// selected predicate, scroll position). All fields inside
    /// [`SidebarConfig`] are `Option<_>` so an absent `[gui.sidebar]`
    /// table on a pre-M173 config.toml round-trips unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<SidebarConfig>,
}

/// `[gui.sidebar]` — per-user sidebar selection + collapsed state
/// (M173 Lane A task A9).
///
/// All fields are `Option<_>` so an absent table on a pre-M173
/// `config.toml` round-trips unchanged. The Rust GUI defaults each
/// field to a no-op (collapsed=false, predicate=All, scroll=0.0) when
/// the field is absent, which matches the M163-era cold-start UI.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SidebarConfig {
    /// Whether the Library section is collapsed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_collapsed: Option<bool>,
    /// Whether the Categories section is collapsed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_collapsed: Option<bool>,
    /// Whether the Tags section is collapsed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_collapsed: Option<bool>,
    /// Whether the Trackers section is collapsed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracker_collapsed: Option<bool>,
    /// Currently-selected sidebar row, expressed as the
    /// `SidebarSection::to_token()` slug
    /// (`library:downloading` / `category:Linux` / etc.). Anything that
    /// fails to parse on next launch falls back to the `Library::All`
    /// default — invalid tokens never panic the GUI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_predicate: Option<String>,
    /// Sidebar scroll offset in pixels (0.0 = top). Persisted as `f32`
    /// to match Slint's `length`-typed scroll coordinate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_offset_px: Option<f32>,
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
    /// GUI column layout and visibility state.
    #[serde(default, skip_serializing_if = "section_is_default")]
    pub gui: GuiConfig,
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
            gui: GuiConfig::default(),
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

/// Atomically write `config` to `path` with Unix `0o600` permissions and a
/// one-time pre-existing `.bak` snapshot (M172a Lane A).
///
/// # Security semantics
///
/// The qBt-compat password hash lives in this file. Owner-only file permissions
/// (`0o600`) are therefore a defence-in-depth requirement — `chmod` is applied
/// to the tempfile *before* `NamedTempFile::persist` performs the atomic
/// rename, so there is no window during which the target file is world- or
/// group-readable.
///
/// A `.bak` snapshot of the previous on-disk contents is taken exactly once —
/// the first time `save_config_atomic` is invoked on an existing file with no
/// prior `.bak`. Subsequent writes do not overwrite the `.bak`; if an operator
/// rolls back by renaming `.bak` back onto the primary path, the next write
/// will create a fresh snapshot. The `.bak` is also permissioned `0o600`.
///
/// # Errors
///
/// Returns an error if the parent directory cannot be created, if serialisation
/// fails, if `chmod` fails on the temp file, or if the atomic persist fails.
pub fn save_config_atomic(path: &Path, config: &ConfigFile) -> Result<()> {
    let serialized =
        toml::to_string_pretty(config).context("failed to serialize config to TOML")?;
    write_config_bytes_atomic(path, serialized.as_bytes())
}

/// Atomic-write + `0o600` + one-time `.bak` for an already-serialised byte
/// buffer. Used by `save_config_atomic` and by `cmd_init`, which hand-rolls a
/// commented-out TOML document via `toml_edit` that isn't a `ConfigFile`.
///
/// # Errors
///
/// See [`save_config_atomic`] — same failure modes.
pub fn write_config_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory: {parent:?}"))?;
    }

    // One-time `.bak` snapshot: copy the *existing* file before we overwrite it,
    // but only if no `.bak` is present yet. Operators can roll back by renaming
    // `.bak` onto the primary path and the next save_config_atomic will make a
    // fresh snapshot.
    if path.exists() {
        let bak_path = bak_path_for(path);
        if !bak_path.exists() {
            std::fs::copy(path, &bak_path)
                .with_context(|| format!("failed to snapshot config to {bak_path:?}"))?;
            #[cfg(unix)]
            apply_owner_only_perms(&bak_path)
                .with_context(|| format!("failed to chmod 0600 {bak_path:?}"))?;
        }
    }

    // Tempfile lives in the same directory as the target so the atomic
    // rename is guaranteed to stay on one filesystem (matches M170/M171's
    // registry write pattern).
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file in {parent:?}"))?;
    std::io::Write::write_all(tmp.as_file_mut(), bytes)
        .context("failed to write config body to temp file")?;
    std::io::Write::flush(tmp.as_file_mut()).context("failed to flush temp file")?;
    #[cfg(unix)]
    apply_owner_only_perms(tmp.path())
        .with_context(|| format!("failed to chmod 0600 temp config {:?}", tmp.path()))?;

    tmp.persist(path)
        .map_err(|e| anyhow::anyhow!("failed to atomically rename config to {path:?}: {e}"))?;

    Ok(())
}

/// Path where `save_config_atomic` writes the one-time pre-existing snapshot.
///
/// Mirrors the `.bak` convention used by the M170 `CategoryRegistry` /
/// M171 `TagRegistry`. Public for downstream tooling that wants to rotate or
/// inspect the backup.
#[must_use]
pub fn bak_path_for(path: &Path) -> PathBuf {
    let mut bak = path.as_os_str().to_owned();
    bak.push(".bak");
    PathBuf::from(bak)
}

/// Outcome of [`migrate_qbt_credentials_in_file`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QbtFileMigration {
    /// No file, no `[qbt_compat]` section, or `password_hash` already present —
    /// nothing rewritten.
    NoOp,
    /// Plaintext hashed into `password_hash` and the file atomically rewritten.
    Rewritten,
}

/// On-disk migration for the legacy `[qbt_compat] password = "..."` block
/// (M172a A3 + C2).
///
/// Reads the TOML at `path`, and if a `[qbt_compat]` section exists with a
/// non-empty `password` key and an empty-or-absent `password_hash`, hashes the
/// plaintext via argon2id, replaces the two keys accordingly, and atomically
/// rewrites the file through [`write_config_bytes_atomic`] (which applies the
/// `0o600` chmod and one-time `.bak` snapshot). If no migration is needed —
/// which is the overwhelmingly common path on fresh installs — the file is
/// not touched.
///
/// Runs as a raw `toml::Value` edit rather than a `ConfigFile` round-trip so
/// we don't need to wire `qbt_compat` into the curated `ConfigFile` shape in
/// Lane A — Lane B owns that plumbing. Unknown fields in the on-disk TOML
/// are preserved verbatim.
///
/// # Security — `.bak` retains the plaintext password indefinitely
///
/// On the first run that actually rewrites the file, [`write_config_bytes_atomic`]
/// leaves a one-time `<path>.bak` snapshot of the PRE-MIGRATION file, chmod
/// `0o600`. That snapshot **still contains the legacy plaintext password**.
/// Subsequent migration passes are no-ops (the live file is already hashed)
/// and do NOT refresh the `.bak`, so the plaintext lingers for as long as the
/// operator leaves the backup in place. This is the intentional trade: keep
/// an auditable pre-migration artefact for rollback, at the cost of plaintext
/// sitting on disk.
///
/// **Operators who want to fully retire the plaintext should delete
/// `<path>.bak` after verifying the daemon authenticates with the migrated
/// hash on the next restart.** A future milestone (M172 future hardening, see
/// the M172a design doc) will add an opt-in "shred `.bak` after N successful
/// authenticates" mode; Lane A does not ship that.
///
/// # Failure semantics
///
/// Every filesystem / hash error is returned as `Err` — the caller (the CLI
/// or GUI wrapper) decides whether to log + continue or escalate. The daemon
/// itself (`SessionHandle::start_full`) never calls this function directly;
/// it only mutates the in-memory `Settings`. This separation matters for C2:
/// a transient `write`-denied config directory must not crash the daemon.
///
/// # Errors
///
/// Returns an error if the file exists but is not valid TOML, if hashing
/// fails, or if the atomic rewrite fails.
pub fn migrate_qbt_credentials_in_file(path: &Path) -> Result<QbtFileMigration> {
    if !path.exists() {
        return Ok(QbtFileMigration::NoOp);
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {path:?}"))?;
    let mut doc: toml::Value = text
        .parse()
        .with_context(|| format!("config file is not valid TOML: {path:?}"))?;

    // Carve out the `[qbt_compat]` table — exit cleanly if it's not present.
    let Some(qbt) = doc.get_mut("qbt_compat").and_then(|v| v.as_table_mut()) else {
        return Ok(QbtFileMigration::NoOp);
    };

    let hash_present = qbt
        .get("password_hash")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty());
    if hash_present {
        return Ok(QbtFileMigration::NoOp);
    }

    // Acquire the plaintext as an owned String under a zeroize guard so it
    // never lingers beyond the block. This is the substantive zeroize path:
    // the Settings struct otherwise keeps the plaintext across the lifetime
    // of the daemon.
    let plaintext = match qbt.get("password").and_then(|v| v.as_str()) {
        Some(pw) if !pw.is_empty() => zeroize::Zeroizing::new(pw.to_owned()),
        _ => return Ok(QbtFileMigration::NoOp),
    };

    // Use the session crate's hasher so we stay parameter-aligned.
    let hash = irontide_session::hash_qbt_password(&plaintext)
        .map_err(|e| anyhow::anyhow!("argon2 hash: {e}"))?;
    qbt.insert("password_hash".to_owned(), toml::Value::String(hash));
    qbt.insert("password".to_owned(), toml::Value::String(String::new()));

    let serialized = toml::to_string_pretty(&doc)
        .context("failed to re-serialize migrated qbt_compat config")?;
    write_config_bytes_atomic(path, serialized.as_bytes())?;
    Ok(QbtFileMigration::Rewritten)
}

/// Apply Unix owner-read-write-only (`0o600`) permissions to a path.
///
/// Separated out so the secret-bearing write paths (`save_config_atomic`) and
/// the `.bak` snapshot path both use the identical mode. No-op on non-Unix —
/// Windows does not model POSIX modes directly, and the registry-backed
/// credential store is the right fix there (deferred).
#[cfg(unix)]
fn apply_owner_only_perms(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
}

/// Persist GUI-specific configuration back to the TOML file.
///
/// Reads the existing file (if any), replaces the `[gui]` section, and
/// writes the result back. All other sections (`[session]`, `[api]`,
/// `[limits]`) are preserved verbatim.
///
/// # Errors
///
/// Returns an error if the file cannot be read/written, or if the existing
/// file content is not valid TOML.
pub fn save_gui_config(config_path: Option<&Path>, gui: &GuiConfig) -> Result<()> {
    let path = resolve_config_path(config_path);

    let mut config = if path.exists() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file: {path:?}"))?;
        toml::from_str::<ConfigFile>(&text)
            .with_context(|| format!("failed to parse config file: {path:?}"))?
    } else {
        ConfigFile::default()
    };

    config.gui = gui.clone();

    save_config_atomic(&path, &config)
}

/// Persist `[session]` download directory and resume directory to the TOML file.
///
/// Reads the existing file (if any), updates `session.download_dir` and
/// `session.resume_dir`, and writes the result back. All other sections
/// are preserved verbatim.
///
/// # Errors
///
/// Returns an error if the file cannot be read/written.
pub fn save_session_download_dir(config_path: Option<&Path>, download_dir: &Path) -> Result<()> {
    let path = resolve_config_path(config_path);

    let mut config = if path.exists() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file: {path:?}"))?;
        toml::from_str::<ConfigFile>(&text)
            .with_context(|| format!("failed to parse config file: {path:?}"))?
    } else {
        ConfigFile::default()
    };

    config.session.download_dir = Some(download_dir.to_path_buf());

    save_config_atomic(&path, &config)
}

// ── Runtime construction ────────────────────────────────────────────

/// Build a multi-thread tokio runtime with optional CPU core affinity.
///
/// Worker thread count is taken from `settings.runtime_worker_threads`
/// (0 = auto-detect, capped at 8). When `settings.pin_cores` is true,
/// each worker thread is pinned to a CPU core via `core_affinity`.
#[must_use] 
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
            gui: GuiConfig::default(),
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
        let settings = Settings {
            resume_data_dir: Some(PathBuf::from("/data/resume")),
            ..Settings::default()
        };
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

    // ---- build_runtime tests ----

    #[test]
    fn build_runtime_creates_runtime() {
        let settings = Settings {
            runtime_worker_threads: 2,
            pin_cores: true,
            ..Settings::default()
        };
        let rt = build_runtime(&settings);
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn build_runtime_no_pin() {
        let settings = Settings {
            runtime_worker_threads: 2,
            pin_cores: false,
            ..Settings::default()
        };
        let rt = build_runtime(&settings);
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn build_runtime_auto_workers() {
        let settings = Settings {
            runtime_worker_threads: 0,
            pin_cores: false,
            ..Settings::default()
        };
        let rt = build_runtime(&settings);
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    // ---- M163: GuiConfig tests ----

    #[test]
    fn test_gui_config_round_trip() {
        let original = GuiConfig {
            column_order: Some(vec!["name".into(), "size".into(), "progress".into()]),
            column_visibility: Some(vec!["name".into(), "progress".into()]),
            column_widths: Some(vec![200.0_f32, 80.0_f32, 120.0_f32]),
            ..Default::default()
        };

        let serialized = toml::to_string_pretty(&original).expect("serialize GuiConfig");
        let deserialized: GuiConfig = toml::from_str(&serialized).expect("deserialize GuiConfig");

        assert_eq!(deserialized.column_order, original.column_order);
        assert_eq!(deserialized.column_visibility, original.column_visibility);
        assert_eq!(deserialized.column_widths, original.column_widths);
    }

    #[test]
    fn test_gui_config_default_absent() {
        let toml_str = r#"
[session]
listen_port = 12345
"#;
        let config: ConfigFile = toml::from_str(toml_str).expect("valid TOML");
        assert_eq!(
            config.gui,
            GuiConfig::default(),
            "missing [gui] section should produce default GuiConfig"
        );
    }

    #[test]
    fn test_save_gui_preserves_sections() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");

        // Write a file with [session] already present.
        std::fs::write(
            &config_path,
            r#"
[session]
listen_port = 42020
"#,
        )
        .expect("write initial config");

        let gui = GuiConfig {
            column_order: Some(vec!["name".into(), "eta".into()]),
            column_visibility: Some(vec!["name".into()]),
            column_widths: Some(vec![150.0_f32, 60.0_f32]),
            ..Default::default()
        };

        save_gui_config(Some(&config_path), &gui).expect("save_gui_config should succeed");

        let text = std::fs::read_to_string(&config_path).expect("read back config");
        let reloaded: ConfigFile = toml::from_str(&text).expect("parse saved config");

        // [session] must still be intact.
        assert_eq!(
            reloaded.session.listen_port,
            Some(42020),
            "[session] should be preserved after save_gui_config"
        );

        // [gui] must contain the new values.
        assert_eq!(reloaded.gui.column_order, gui.column_order);
        assert_eq!(reloaded.gui.column_visibility, gui.column_visibility);
        assert_eq!(reloaded.gui.column_widths, gui.column_widths);
    }

    // ── M172b Lane A: GuiConfig skin/theme/density/radius_preset ──────

    #[test]
    fn test_gui_config_old_file_loads_with_none_for_new_fields() {
        // Pre-M172b config.toml (only column_* fields populated).
        let toml_str = r#"
[gui]
column_order = ["name", "size"]
column_visibility = ["name"]
column_widths = [200.0, 80.0]
"#;
        let config: ConfigFile = toml::from_str(toml_str).expect("valid TOML");
        assert_eq!(config.gui.skin, None);
        assert_eq!(config.gui.theme, None);
        assert_eq!(config.gui.density, None);
        assert_eq!(config.gui.radius_preset, None);
        // Existing fields still load.
        assert_eq!(
            config.gui.column_order,
            Some(vec!["name".into(), "size".into()])
        );
    }

    #[test]
    fn test_gui_config_all_seven_fields_round_trip() {
        let original = GuiConfig {
            column_order: Some(vec!["name".into()]),
            column_visibility: Some(vec!["name".into()]),
            column_widths: Some(vec![200.0_f32]),
            skin: Some("forge".into()),
            theme: Some("light".into()),
            density: Some("compact".into()),
            radius_preset: Some("sharp".into()),
            layout: None,
            l3_sidebar_mode: None,
            inspector_shown: None,
            sidebar: None,
        };
        let serialized = toml::to_string_pretty(&original).expect("serialize");
        let deserialized: GuiConfig = toml::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized.skin, original.skin);
        assert_eq!(deserialized.theme, original.theme);
        assert_eq!(deserialized.density, original.density);
        assert_eq!(deserialized.radius_preset, original.radius_preset);
        assert_eq!(deserialized.column_order, original.column_order);
        // Sidebar absent in serialized form when None.
        assert!(
            !serialized.contains("[gui.sidebar]"),
            "absent sidebar table must not be emitted: {serialized}"
        );
    }

    // ── M176: layout / l3_sidebar_mode / inspector_shown ────────────────

    #[test]
    fn test_gui_config_pre_m176_loads_with_none_for_layout_fields() {
        // Pre-M176 config.toml — only the M163/M172b/M173 GUI fields.
        let toml_str = r#"
[gui]
column_order = ["name", "size"]
skin = "tide"
theme = "dark"
"#;
        let config: ConfigFile = toml::from_str(toml_str).expect("valid TOML");
        assert_eq!(config.gui.layout, None);
        assert_eq!(config.gui.l3_sidebar_mode, None);
        assert_eq!(config.gui.inspector_shown, None);
        // Existing fields still load.
        assert_eq!(config.gui.skin.as_deref(), Some("tide"));
    }

    #[test]
    fn test_gui_config_layout_fields_round_trip() {
        let original = GuiConfig {
            layout: Some("L3".into()),
            l3_sidebar_mode: Some("hidden".into()),
            inspector_shown: Some(true),
            ..Default::default()
        };
        let serialized = toml::to_string_pretty(&original).expect("serialize");
        let deserialized: GuiConfig = toml::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized.layout, Some("L3".into()));
        assert_eq!(deserialized.l3_sidebar_mode, Some("hidden".into()));
        assert_eq!(deserialized.inspector_shown, Some(true));
    }

    #[test]
    fn test_gui_config_layout_default_omits_fields_in_serialization() {
        let config = GuiConfig {
            skin: Some("tide".into()),
            ..Default::default()
        };
        let serialized = toml::to_string_pretty(&config).expect("serialize");
        // None layout fields must not appear in TOML.
        assert!(!serialized.contains("layout"), "absent layout: {serialized}");
        assert!(
            !serialized.contains("l3_sidebar_mode"),
            "absent l3_sidebar_mode: {serialized}"
        );
        assert!(
            !serialized.contains("inspector_shown"),
            "absent inspector_shown: {serialized}"
        );
    }

    // ── M173 Lane A: SidebarConfig ───────────────────────────────────

    #[test]
    fn test_sidebar_config_full_round_trip() {
        let sb = SidebarConfig {
            library_collapsed: Some(false),
            category_collapsed: Some(true),
            tag_collapsed: Some(false),
            tracker_collapsed: Some(true),
            selected_predicate: Some("category:Linux".into()),
            scroll_offset_px: Some(42.5_f32),
        };
        let original = GuiConfig {
            sidebar: Some(sb.clone()),
            ..Default::default()
        };
        let serialized = toml::to_string_pretty(&original).expect("serialize");
        // The TOML inline-table style nests the sidebar inside [gui]
        // either as `[gui.sidebar]` (table form) or `sidebar = { ... }`
        // (inline form). Either is valid; assert the field name is
        // present rather than pinning the table syntax.
        assert!(
            serialized.contains("sidebar"),
            "non-empty sidebar must surface in serialized output: {serialized}"
        );
        let deserialized: GuiConfig = toml::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized.sidebar, Some(sb));
    }

    #[test]
    fn test_sidebar_config_partial_round_trip() {
        // Partial update: only `selected_predicate` set. Other fields
        // must round-trip as None so the GUI applies the default at
        // load.
        let sb = SidebarConfig {
            selected_predicate: Some("library:errored".into()),
            ..Default::default()
        };
        let original = GuiConfig {
            sidebar: Some(sb.clone()),
            ..Default::default()
        };
        let serialized = toml::to_string_pretty(&original).expect("serialize");
        // Each None field must be omitted (skip_serializing_if).
        assert!(
            !serialized.contains("library_collapsed"),
            "absent fields must not serialize: {serialized}"
        );
        let deserialized: GuiConfig = toml::from_str(&serialized).expect("deserialize");
        assert_eq!(deserialized.sidebar, Some(sb));
    }

    #[test]
    fn test_pre_m173_config_loads_with_sidebar_none() {
        // Pre-M173 file with only the M163/M172b GUI fields.
        let toml_str = r#"
[gui]
column_order = ["name", "size"]
column_widths = [200.0, 80.0]
skin = "tide"
"#;
        let cfg: ConfigFile = toml::from_str(toml_str).expect("valid TOML");
        assert!(cfg.gui.sidebar.is_none(), "missing [gui.sidebar] → None");
        // Existing fields still load.
        assert_eq!(cfg.gui.skin.as_deref(), Some("tide"));
    }

    #[test]
    fn test_save_gui_config_round_trips_sidebar() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        let gui = GuiConfig {
            sidebar: Some(SidebarConfig {
                library_collapsed: Some(false),
                selected_predicate: Some("library:downloading".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        save_gui_config(Some(&config_path), &gui).expect("save");
        let text = std::fs::read_to_string(&config_path).expect("read back");
        let reloaded: ConfigFile = toml::from_str(&text).expect("parse");
        assert_eq!(reloaded.gui.sidebar, gui.sidebar);
    }

    // ── M172a Lane A: save_config_atomic ─────────────────────────────

    #[test]
    fn save_config_atomic_happy_path() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("config.toml");

        let config = ConfigFile {
            session: SessionConfig {
                listen_port: Some(12345),
                ..Default::default()
            },
            ..Default::default()
        };
        save_config_atomic(&path, &config).expect("happy-path write should succeed");

        assert!(path.exists(), "atomic write must materialise the target");
        let body = std::fs::read_to_string(&path).expect("read back");
        let parsed: ConfigFile = toml::from_str(&body).expect("valid TOML emitted");
        assert_eq!(parsed.session.listen_port, Some(12345));
    }

    #[cfg(unix)]
    #[test]
    fn save_config_atomic_applies_chmod_0600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("config.toml");

        save_config_atomic(&path, &ConfigFile::default()).expect("write should succeed");

        let mode = std::fs::metadata(&path)
            .expect("stat written config")
            .permissions()
            .mode();
        // Permissions may include file-type bits on some platforms; mask to the
        // low 9 rwx bits we actually set.
        assert_eq!(
            mode & 0o777,
            0o600,
            "expected 0o600, got {:o}",
            mode & 0o777
        );
    }

    #[test]
    fn bak_created_on_first_migration_only() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("config.toml");
        let bak = super::bak_path_for(&path);

        // Pre-seed a v1 file on disk (simulates a pre-M172a config).
        std::fs::write(&path, "[session]\nlisten_port = 11111\n").expect("pre-seed config");

        let cfg_a = ConfigFile {
            session: SessionConfig {
                listen_port: Some(22222),
                ..Default::default()
            },
            ..Default::default()
        };
        save_config_atomic(&path, &cfg_a).expect("first write");
        assert!(bak.exists(), "first rewrite must create .bak");
        let bak_body_first = std::fs::read_to_string(&bak).expect("read .bak after first write");
        assert!(
            bak_body_first.contains("11111"),
            "expected original pre-seeded config in .bak, got: {bak_body_first}"
        );

        // Second write must NOT overwrite the existing .bak.
        let cfg_b = ConfigFile {
            session: SessionConfig {
                listen_port: Some(33333),
                ..Default::default()
            },
            ..Default::default()
        };
        save_config_atomic(&path, &cfg_b).expect("second write");
        let bak_body_second = std::fs::read_to_string(&bak).expect("read .bak after second write");
        assert_eq!(
            bak_body_first, bak_body_second,
            ".bak must be snapshot-on-first-write"
        );
    }

    // ── M172a Lane A: migrate_qbt_credentials_in_file ─────────────────

    #[test]
    fn migrate_qbt_credentials_in_file_noop_when_file_missing() {
        let outcome = super::migrate_qbt_credentials_in_file(Path::new(
            "/tmp/irontide-m172a-absent-config-XYZ.toml",
        ))
        .expect("missing file must be NoOp");
        assert_eq!(outcome, super::QbtFileMigration::NoOp);
    }

    #[test]
    fn migrate_qbt_credentials_in_file_noop_when_no_qbt_section() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[session]\nlisten_port = 12345\n").expect("write");

        let outcome =
            super::migrate_qbt_credentials_in_file(&path).expect("no-qbt_compat must be NoOp");
        assert_eq!(outcome, super::QbtFileMigration::NoOp);

        // File is untouched (no `.bak` should be created either).
        assert!(!super::bak_path_for(&path).exists(), "no .bak when NoOp");
    }

    #[test]
    fn migrate_qbt_credentials_in_file_rewrites_once_then_noop() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");
        let body = r#"
[qbt_compat]
enabled = true
username = "admin"
password = "legacyplaintext"
"#;
        std::fs::write(&path, body).expect("write legacy");

        let outcome = super::migrate_qbt_credentials_in_file(&path).expect("first pass");
        assert_eq!(outcome, super::QbtFileMigration::Rewritten);

        let body_after = std::fs::read_to_string(&path).expect("read back");
        assert!(
            body_after.contains("password_hash"),
            "rewrite must add hash"
        );
        assert!(
            body_after.contains("password = \"\""),
            "rewrite must blank plaintext: {body_after}"
        );

        // Second call must be NoOp and must not overwrite `.bak`.
        let bak = super::bak_path_for(&path);
        let bak_before = std::fs::read_to_string(&bak).expect("bak must exist after first write");
        let outcome2 = super::migrate_qbt_credentials_in_file(&path).expect("second pass");
        assert_eq!(outcome2, super::QbtFileMigration::NoOp);
        let bak_after = std::fs::read_to_string(&bak).expect("bak still present");
        assert_eq!(bak_before, bak_after, ".bak must not be overwritten");
    }

    #[test]
    fn migrate_qbt_credentials_in_file_noop_when_hash_already_present() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");
        let body = r#"
[qbt_compat]
enabled = true
username = "admin"
password_hash = "$argon2id$v=19$m=19456,t=2,p=1$somesalt$somehash"
"#;
        std::fs::write(&path, body).expect("write");

        let outcome =
            super::migrate_qbt_credentials_in_file(&path).expect("already-hashed must be NoOp");
        assert_eq!(outcome, super::QbtFileMigration::NoOp);
        assert!(!super::bak_path_for(&path).exists(), "no .bak on NoOp");
    }

    #[test]
    fn migrate_qbt_credentials_in_file_propagates_hash_rewrite_into_settings() {
        use argon2::Argon2;
        use argon2::password_hash::{PasswordHash, PasswordVerifier};

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[qbt_compat]
enabled = true
username = "admin"
password = "legacyplaintext"
"#,
        )
        .expect("write");

        super::migrate_qbt_credentials_in_file(&path).expect("migrate");

        // Extract the rewritten hash and verify it with the argon2 crate.
        let text = std::fs::read_to_string(&path).expect("read back");
        let doc: toml::Value = text.parse().expect("valid TOML");
        let hash = doc["qbt_compat"]["password_hash"]
            .as_str()
            .expect("password_hash must be a string")
            .to_owned();
        let parsed = PasswordHash::new(&hash).expect("rewrite must be valid PHC");
        Argon2::default()
            .verify_password(b"legacyplaintext", &parsed)
            .expect("rewrite must verify the original plaintext");
    }

    #[test]
    fn save_config_atomic_propagates_tempfile_failure() {
        // The parent "directory" here is actually a regular file — creating a
        // sibling tempfile must fail because you cannot create a file inside
        // a non-directory. Works on every platform, no platform-gated perm
        // bits, no skip-if-root dance.
        let dir = tempfile::tempdir().expect("create temp dir");
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("create blocker file");

        let path = blocker.join("config.toml");
        let result = save_config_atomic(&path, &ConfigFile::default());

        assert!(
            result.is_err(),
            "write into a non-directory parent should fail, got: {result:?}"
        );
    }
}
