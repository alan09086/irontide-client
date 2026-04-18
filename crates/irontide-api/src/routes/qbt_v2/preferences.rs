//! qBt v2 preferences DTO (M168, full impl in Task 7).
//!
//! Projects IronTide `Settings` onto the qBt WebUI v2 preferences JSON shape
//! that `*arr` clients expect. See M170 for the reverse direction
//! (`setPreferences`).

use irontide::session::Settings;
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

    // Hardcoded safe defaults below. FIXME(M170): wire to real state.
    pub max_ratio_act: String,
    pub max_seeding_time_enabled: bool,
    pub max_seeding_time: i64,
    pub max_inactive_seeding_time_enabled: bool,
    pub max_inactive_seeding_time: i64,
    pub queueing_enabled: bool,
    pub create_subfolder_enabled: bool,
    pub start_paused_enabled: bool,
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

            // FIXME(M170): wire to real state.
            max_ratio_act: "pause".into(),
            max_seeding_time_enabled: false,
            max_seeding_time: -1,
            max_inactive_seeding_time_enabled: false,
            max_inactive_seeding_time: -1,
            queueing_enabled: false,
            create_subfolder_enabled: true,
            start_paused_enabled: false,
            auto_tmm_enabled: false,
        }
    }
}
