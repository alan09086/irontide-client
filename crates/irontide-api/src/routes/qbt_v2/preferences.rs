//! qBt v2 preferences DTO (M168, full impl in Task 7).
//!
//! Projects IronTide `Settings` onto the qBt WebUI v2 preferences JSON shape
//! that `*arr` clients expect. See M170 for the reverse direction
//! (`setPreferences`).

use irontide::session::{MaxRatioAction, Settings};
use serde::{Deserialize, Serialize};

/// qBt encryption mode enum — canonical mapping.
///
/// qBt WebUI v2 (since 4.x) uses:
/// - `0` = Prefer encryption
/// - `1` = Force encryption (require)
/// - `2` = Disable encryption
///
/// Mapped from IronTide's `irontide_wire::mse::EncryptionMode`.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(into = "u8", try_from = "u8")]
pub enum QbtEncryption {
    Prefer = 0,
    Force = 1,
    Disable = 2,
}

impl From<QbtEncryption> for u8 {
    fn from(e: QbtEncryption) -> Self {
        e as u8
    }
}

impl TryFrom<u8> for QbtEncryption {
    type Error = &'static str;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(QbtEncryption::Prefer),
            1 => Ok(QbtEncryption::Force),
            2 => Ok(QbtEncryption::Disable),
            _ => Err("invalid qBt encryption mode"),
        }
    }
}

/// Subset of qBt `app/preferences` JSON that `*arr` actually reads.
///
/// Deliberately flat — we don't model every field qBt exposes. Unknown fields
/// `*arr` requests default to hardcoded safe values so deserialisation never
/// panics with a missing key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QbtPreferences {
    pub save_path: String,
    pub dht: bool,
    pub pex: bool,
    pub lsd: bool,
    pub upnp: bool,
    pub listen_port: u16,
    pub max_ratio: f64,
    pub max_ratio_enabled: bool,
    pub encryption: QbtEncryption,
    pub web_ui_username: String,

    /// M171: Action taken when `seed_ratio_limit` is reached.
    /// Wire format: `"pause"` | `"remove"` | `"enable_super_seeding"`.
    pub max_ratio_act: String,
    /// M171: Wired to `seed_time_limit_secs` (D1a).
    pub max_seeding_time_enabled: bool,
    /// M171: Wired to `seed_time_limit_secs` (D1a), minutes on the wire.
    pub max_seeding_time: i64,
    /// M171: Wired to `inactive_seed_time_limit_secs` (D1a).
    pub max_inactive_seeding_time_enabled: bool,
    /// M171: Wired to `inactive_seed_time_limit_secs` (D1a), minutes on the wire.
    pub max_inactive_seeding_time: i64,
    /// M171: Wired to `queueing_enabled` (D1+D2).
    pub queueing_enabled: bool,
    /// M171: Wired to `create_subfolder` (D1+D2).
    pub create_subfolder_enabled: bool,
    /// Hardcoded safe default — IronTide adds torrents running by default.
    /// TODO(M174): wire once we have an "add paused" toggle in Settings.
    pub start_paused_enabled: bool,
    /// M171: Wired to `auto_manage_torrents` (D1+D2).
    pub auto_tmm_enabled: bool,
}

impl From<&Settings> for QbtPreferences {
    fn from(s: &Settings) -> Self {
        use irontide::prelude::EncryptionMode;

        let encryption = match s.encryption_mode {
            EncryptionMode::Disabled => QbtEncryption::Disable,
            EncryptionMode::Enabled | EncryptionMode::PreferPlaintext => {
                QbtEncryption::Prefer
            }
            EncryptionMode::Forced => QbtEncryption::Force,
        };

        let (max_ratio, max_ratio_enabled) = match s.seed_ratio_limit {
            Some(r) => (r, true),
            None => (-1.0, false),
        };

        // M171: qBt stores seed-time preferences in MINUTES on the wire;
        // our canonical field is seconds. Use integer division — fractional
        // minutes are not expressible in qBt's model. The paired `*_enabled`
        // boolean mirrors qBt exactly: `Some` => true, `None` => false.
        let (max_seeding_time, max_seeding_time_enabled) = match s.seed_time_limit_secs {
            Some(secs) => ((secs / 60) as i64, true),
            None => (-1, false),
        };
        let (max_inactive_seeding_time, max_inactive_seeding_time_enabled) =
            match s.inactive_seed_time_limit_secs {
                Some(secs) => ((secs / 60) as i64, true),
                None => (-1, false),
            };

        Self {
            save_path: s.download_dir.to_string_lossy().into_owned(),
            dht: s.enable_dht,
            pex: s.enable_pex,
            lsd: s.enable_lsd,
            upnp: s.enable_upnp,
            listen_port: s.listen_port,
            max_ratio,
            max_ratio_enabled,
            encryption,
            web_ui_username: s.qbt_compat.username.clone(),

            // M171: seed-time preferences wired to real Settings (D1a).
            max_seeding_time,
            max_seeding_time_enabled,
            max_inactive_seeding_time,
            max_inactive_seeding_time_enabled,

            // M171 D2: four fields that were hardcoded in M168/M170 are
            // now wired to real Settings — see commit for the canonical
            // mapping.
            max_ratio_act: match s.max_ratio_action {
                MaxRatioAction::Pause => "pause",
                MaxRatioAction::Remove => "remove",
                MaxRatioAction::EnableSuperSeeding => "enable_super_seeding",
            }
            .into(),
            queueing_enabled: s.queueing_enabled,
            create_subfolder_enabled: s.create_subfolder,
            auto_tmm_enabled: s.auto_manage_torrents,

            // Hardcoded safe default until M174.
            start_paused_enabled: false,
        }
    }
}
