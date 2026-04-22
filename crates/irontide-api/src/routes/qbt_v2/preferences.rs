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

    /// M172a Lane B: wired to `qbt_compat.csrf_protection_enabled`.
    pub web_ui_csrf_protection_enabled: bool,
    /// M172a Lane B: wired to `qbt_compat.host_header_validation_enabled`.
    pub web_ui_host_header_validation_enabled: bool,
    /// M172a Lane B: wired to `qbt_compat.web_ui_reverse_proxy_enabled`.
    pub web_ui_reverse_proxy_enabled: bool,
    /// M172a Lane B: semicolon-joined string form of
    /// `qbt_compat.web_ui_reverse_proxies_list` to match qBt's on-wire
    /// convention.
    pub web_ui_reverse_proxies_list: String,
}

impl From<&Settings> for QbtPreferences {
    fn from(s: &Settings) -> Self {
        use irontide::prelude::EncryptionMode;

        let encryption = match s.encryption_mode {
            EncryptionMode::Disabled => QbtEncryption::Disable,
            EncryptionMode::Enabled | EncryptionMode::PreferPlaintext => QbtEncryption::Prefer,
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

            // M172a Lane B: CSRF + reverse-proxy toggles.
            web_ui_csrf_protection_enabled: s.qbt_compat.csrf_protection_enabled,
            web_ui_host_header_validation_enabled: s.qbt_compat.host_header_validation_enabled,
            web_ui_reverse_proxy_enabled: s.qbt_compat.web_ui_reverse_proxy_enabled,
            web_ui_reverse_proxies_list: s.qbt_compat.web_ui_reverse_proxies_list.join(";"),
        }
    }
}

#[cfg(test)]
mod tests {
    //! D3.1 (M173 Lane C): unit coverage for the qBt v2 preferences DTO.
    //!
    //! Focus: encryption int mapping, wire-unit conversions (seconds vs.
    //! minutes), `Option<T>`-to-sentinel rendering (`-1` / `false`), the
    //! `max_ratio_act` enum round-trip, and the reverse-proxy list's
    //! semicolon-joined wire shape. The DTO is a pure projection of
    //! `Settings`, so these tests construct a `Settings`, run
    //! `QbtPreferences::from(&settings)`, and assert field-by-field.
    use super::*;
    use irontide::prelude::EncryptionMode;
    use irontide::session::Settings;

    fn base_settings() -> Settings {
        // Minimum-boilerplate settings — the defaults already have
        // `qbt_compat` disabled; we flip `enabled` to exercise the
        // username pass-through.
        let mut s = Settings::default();
        s.qbt_compat.enabled = true;
        s
    }

    #[test]
    fn encryption_disabled_maps_to_qbt_two() {
        let mut s = base_settings();
        s.encryption_mode = EncryptionMode::Disabled;
        let p = QbtPreferences::from(&s);
        assert_eq!(p.encryption, QbtEncryption::Disable);
        assert_eq!(u8::from(p.encryption), 2);
    }

    #[test]
    fn encryption_enabled_and_prefer_plaintext_both_map_to_qbt_zero() {
        let mut s = base_settings();
        s.encryption_mode = EncryptionMode::Enabled;
        assert_eq!(QbtPreferences::from(&s).encryption, QbtEncryption::Prefer);
        s.encryption_mode = EncryptionMode::PreferPlaintext;
        assert_eq!(QbtPreferences::from(&s).encryption, QbtEncryption::Prefer);
    }

    #[test]
    fn encryption_forced_maps_to_qbt_one() {
        let mut s = base_settings();
        s.encryption_mode = EncryptionMode::Forced;
        let p = QbtPreferences::from(&s);
        assert_eq!(p.encryption, QbtEncryption::Force);
        assert_eq!(u8::from(p.encryption), 1);
    }

    #[test]
    fn encryption_tryfrom_rejects_out_of_range_int() {
        assert!(QbtEncryption::try_from(3_u8).is_err());
        assert!(QbtEncryption::try_from(255_u8).is_err());
        // Valid values round-trip via the canonical int mapping.
        assert_eq!(QbtEncryption::try_from(0_u8).unwrap(), QbtEncryption::Prefer);
        assert_eq!(QbtEncryption::try_from(1_u8).unwrap(), QbtEncryption::Force);
        assert_eq!(
            QbtEncryption::try_from(2_u8).unwrap(),
            QbtEncryption::Disable
        );
    }

    #[test]
    fn max_ratio_none_renders_sentinel_and_flag_false() {
        let mut s = base_settings();
        s.seed_ratio_limit = None;
        let p = QbtPreferences::from(&s);
        assert!(
            (p.max_ratio - -1.0).abs() < f64::EPSILON,
            "unset seed ratio must surface as -1.0 sentinel"
        );
        assert!(!p.max_ratio_enabled);
    }

    #[test]
    fn max_ratio_some_renders_value_and_flag_true() {
        let mut s = base_settings();
        s.seed_ratio_limit = Some(1.75);
        let p = QbtPreferences::from(&s);
        assert!((p.max_ratio - 1.75).abs() < f64::EPSILON);
        assert!(p.max_ratio_enabled);
    }

    #[test]
    fn seed_time_seconds_convert_to_wire_minutes() {
        let mut s = base_settings();
        // Storage unit is seconds; wire unit is minutes. Integer division
        // is the documented truncation for fractional minutes.
        s.seed_time_limit_secs = Some(3660); // 61 minutes (with 60 s truncated).
        s.inactive_seed_time_limit_secs = Some(120); // 2 minutes exactly.
        let p = QbtPreferences::from(&s);
        assert_eq!(p.max_seeding_time, 61);
        assert!(p.max_seeding_time_enabled);
        assert_eq!(p.max_inactive_seeding_time, 2);
        assert!(p.max_inactive_seeding_time_enabled);
    }

    #[test]
    fn seed_time_none_renders_minus_one_and_flag_false() {
        let mut s = base_settings();
        s.seed_time_limit_secs = None;
        s.inactive_seed_time_limit_secs = None;
        let p = QbtPreferences::from(&s);
        assert_eq!(p.max_seeding_time, -1);
        assert!(!p.max_seeding_time_enabled);
        assert_eq!(p.max_inactive_seeding_time, -1);
        assert!(!p.max_inactive_seeding_time_enabled);
    }

    #[test]
    fn max_ratio_action_enum_variants_all_serialise() {
        // All three variants must render with the exact wire slugs that
        // qBt expects on `max_ratio_act`.
        let mut s = base_settings();
        s.max_ratio_action = MaxRatioAction::Pause;
        assert_eq!(QbtPreferences::from(&s).max_ratio_act, "pause");
        s.max_ratio_action = MaxRatioAction::Remove;
        assert_eq!(QbtPreferences::from(&s).max_ratio_act, "remove");
        s.max_ratio_action = MaxRatioAction::EnableSuperSeeding;
        assert_eq!(
            QbtPreferences::from(&s).max_ratio_act,
            "enable_super_seeding"
        );
    }

    #[test]
    fn reverse_proxies_list_joins_with_semicolons() {
        let mut s = base_settings();
        s.qbt_compat.web_ui_reverse_proxies_list = vec![
            "10.0.0.0/8".into(),
            "192.168.0.0/16".into(),
            "::1/128".into(),
        ];
        let p = QbtPreferences::from(&s);
        assert_eq!(
            p.web_ui_reverse_proxies_list, "10.0.0.0/8;192.168.0.0/16;::1/128",
            "qBt wire convention is `;`-joined CIDRs"
        );
    }

    #[test]
    fn dto_roundtrips_through_json_losslessly() {
        // The DTO is Serialize + Deserialize, so *arr clients that echo
        // a prefs payload back to us must produce the same struct. This
        // also covers the bespoke `QbtEncryption` (u8 <-> enum) impl.
        let mut s = base_settings();
        s.seed_ratio_limit = Some(2.5);
        s.seed_time_limit_secs = Some(1800); // 30 minutes
        s.encryption_mode = EncryptionMode::Forced;
        s.max_ratio_action = MaxRatioAction::EnableSuperSeeding;
        s.enable_dht = true;
        s.enable_pex = true;
        s.enable_lsd = false;
        s.enable_upnp = true;
        s.listen_port = 42000;
        let p = QbtPreferences::from(&s);
        let bytes = serde_json::to_vec(&p).expect("serialise");
        let p2: QbtPreferences = serde_json::from_slice(&bytes).expect("deserialise");
        assert_eq!(p.encryption, p2.encryption);
        assert_eq!(p.dht, p2.dht);
        assert_eq!(p.pex, p2.pex);
        assert_eq!(p.lsd, p2.lsd);
        assert_eq!(p.upnp, p2.upnp);
        assert_eq!(p.listen_port, p2.listen_port);
        assert_eq!(p.max_seeding_time, p2.max_seeding_time);
        assert!((p.max_ratio - p2.max_ratio).abs() < f64::EPSILON);
        assert_eq!(p.max_ratio_act, p2.max_ratio_act);
    }
}
