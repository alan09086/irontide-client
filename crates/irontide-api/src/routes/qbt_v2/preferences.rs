#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: qBt preferences DTO — settings projection follows qBt wire-format integer widths"
)]

//! qBt v2 preferences DTO (M168, full impl in Task 7).
//!
//! Projects `IronTide` `Settings` onto the qBt `WebUI` v2 preferences JSON shape
//! that `*arr` clients expect. See M170 for the reverse direction
//! (`setPreferences`).

use irontide::session::{MaxRatioAction, Settings};
use serde::{Deserialize, Serialize};

/// qBt encryption mode enum — canonical mapping.
///
/// qBt `WebUI` v2 (since 4.x) uses:
/// - `0` = Prefer encryption
/// - `1` = Force encryption (require)
/// - `2` = Disable encryption
///
/// Mapped from `IronTide`'s `irontide_wire::mse::EncryptionMode`.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(into = "u8", try_from = "u8")]
pub enum QbtEncryption {
    Prefer = 0,
    Force = 1,
    Disable = 2,
}

impl From<QbtEncryption> for u8 {
    fn from(e: QbtEncryption) -> Self {
        e as Self
    }
}

impl TryFrom<u8> for QbtEncryption {
    type Error = &'static str;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Prefer),
            1 => Ok(Self::Force),
            2 => Ok(Self::Disable),
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
    /// M226: wired to `settings.default_add_paused`. qBt's wire name uses
    /// the "start paused" phrasing; the engine field uses the "default
    /// add paused" phrasing — they describe the same boolean. Previously
    /// hardcoded `false` (pre-M226), now reads through to the engine value.
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

    /// M214: NAT-PMP toggle (`IronTide` extension — qBt only exposes `UPnP`).
    pub natpmp: bool,
    /// M214: global connection cap (`-1` = unlimited). Distinct from qBt's
    /// legacy `max_connec` which projects to per-torrent.
    pub max_connec_global: i32,
    /// M224: per-torrent unchoke-slot cap (`-1` = unlimited). Mirrors qBt's
    /// `max_uploads_per_torrent` wire field.
    pub max_uploads_per_torrent: i32,
    /// M214: proxy type as qBt's wire signed integer enum. Values:
    /// 0=None, 1=Http, 2=Socks4, 3=Socks5, 4=HttpPassword, 5=Socks5Password.
    pub proxy_type: i32,
    /// M214: proxy server hostname or IP.
    pub proxy_ip: String,
    /// M214: proxy server port.
    pub proxy_port: u16,
    /// M214: proxy auth username (empty string = no username configured).
    pub proxy_username: String,
    /// M214: route peer connections through the configured proxy.
    pub proxy_peer_connections: bool,
    /// M214: resolve hostnames through the proxy (SOCKS5/HTTP only).
    pub proxy_hostnames: bool,
    /// M214: drop traffic entirely when proxy fails.
    pub force_proxy: bool,
    // NOTE: `proxy_password` is intentionally NOT exposed on the GET side —
    // same input-only convention as `web_ui_password`. The qBt v2 docs treat
    // this field as write-only too. Tests assert its absence (M214 step 7).
    /// M215: anonymous-mode peer ID. Set via setPreferences, classified as
    /// `restart_required` (handshake/peer-id rotation at session boot).
    pub anonymous_mode: bool,
    /// M215 + M225: piece-hashing worker count. M225 reclassifies as
    /// `classify_immediate` — the live `SettingsDelta::hashing_threads` path
    /// propagates the new value through `TorrentCommand::UpdateSettings` to
    /// every active `TorrentActor::handle_update_settings`. Per-torrent fan-
    /// out replaces the M215 boot-time snapshot.
    pub hashing_threads: u32,
    /// M215 + M225: periodic resume-save interval (seconds). M225 wires a
    /// `tokio::sync::Notify` so the `SessionActor` select! arm rebuilds the
    /// timer on the next loop tick — classified as `classify_immediate`.
    pub save_resume_interval: u64,
    /// M225: master enable switch for the IP filter / live ban list.
    /// Classified as `classify_immediate` — flipping the bit takes the
    /// outer `Arc<RwLock<IpFilter>>` write-lock and `is_blocked` short-
    /// circuits to `false` on the next admit gate without a session restart.
    pub ip_filter_enabled: bool,

    // ── M228: M226 engine fields GET projection ─────────────────────
    /// M228: Fire an OS notification when a torrent finishes. Wired from
    /// `settings.notify_on_complete`. `IronTide` extension (no qBt analogue).
    pub notify_on_complete: bool,
    /// M228: Fire an OS notification on torrent error. Wired from
    /// `settings.notify_on_error`. `IronTide` extension.
    pub notify_on_error: bool,
    /// M228: Program path run on torrent completion (qBt parity:
    /// `autorun_program`). Wired from `settings.on_complete_program`; empty
    /// string when None.
    pub autorun_program: String,
    /// M228: Whether incomplete downloads use a separate directory (qBt
    /// parity: `temp_path_enabled`). Wired from `settings.use_incomplete_dir`.
    pub temp_path_enabled: bool,
    /// M228: Incomplete-downloads directory (qBt parity: `temp_path`).
    /// Wired from `settings.incomplete_dir`; empty when None.
    pub temp_path: String,
    /// M228: Default for `AddTorrentParams.skip_checking`. `IronTide`
    /// extension. Wired from `settings.default_skip_hash_check`.
    pub add_skip_check: bool,
    /// M228: Append `.!it`/`.!qB` to in-flight downloads (qBt parity:
    /// `incomplete_files_ext`). Wired from `settings.incomplete_extension_enabled`.
    pub incomplete_files_ext: bool,
    /// M228: Single-folder watched-directory path. `IronTide` simplified
    /// projection of qBt's `scan_dirs` object map. Wired from
    /// `settings.watched_folder`; empty when None.
    pub scan_dirs_v2: String,
    /// M228: qBt `auto_delete_mode` (`0=manual, 2=always`). Wired from
    /// `settings.delete_torrent_after_add`: `false → 0`, `true → 2`. Round-
    /// trip of wire value `1` is lossy (becomes `2`).
    pub auto_delete_mode: i32,
    /// M228: Move completed torrents to a destination directory. `IronTide`
    /// extension wire name. Wired from `settings.move_completed_enabled`.
    pub move_completed_enabled: bool,
    /// M228: Destination for completed-torrent moves. `IronTide` extension.
    /// Wired from `settings.move_completed_to`; empty when None.
    pub save_path_completed: String,
    /// M228: Enable HTTPS for the qBt v2 `WebUI` (qBt parity: `use_https`).
    /// Wired from `settings.web_ui_https_enabled`. STORED ONLY engine-side.
    pub use_https: bool,
    /// M228: Bind peer listeners to a specific network interface (qBt
    /// parity: `current_network_interface`). Wired from
    /// `settings.network_interface`; empty when None.
    pub current_network_interface: String,
    /// M228: Preallocate full file extents (qBt parity: `preallocate_all`).
    /// Wired: `Some(Full) → true`, anything else → `false`.
    pub preallocate_all: bool,
    /// M228: Auto-refresh the IP-filter file. `IronTide` extension wire name.
    /// Wired from `settings.ip_filter_auto_refresh`.
    pub ip_filter_auto_refresh: bool,
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

            // M226: live read from engine `default_add_paused`.
            start_paused_enabled: s.default_add_paused,

            // M172a Lane B: CSRF + reverse-proxy toggles.
            web_ui_csrf_protection_enabled: s.qbt_compat.csrf_protection_enabled,
            web_ui_host_header_validation_enabled: s.qbt_compat.host_header_validation_enabled,
            web_ui_reverse_proxy_enabled: s.qbt_compat.web_ui_reverse_proxy_enabled,
            web_ui_reverse_proxies_list: s.qbt_compat.web_ui_reverse_proxies_list.join(";"),

            // M214: Connection + Speed round-trip.
            natpmp: s.enable_natpmp,
            max_connec_global: s.max_connections_global,
            // M224: per-torrent unchoke-slot cap.
            max_uploads_per_torrent: s.max_uploads_per_torrent,
            proxy_type: match s.proxy.proxy_type {
                irontide::session::ProxyType::None => 0,
                irontide::session::ProxyType::Http => 1,
                irontide::session::ProxyType::Socks4 => 2,
                irontide::session::ProxyType::Socks5 => 3,
                irontide::session::ProxyType::HttpPassword => 4,
                irontide::session::ProxyType::Socks5Password => 5,
            },
            proxy_ip: s.proxy.hostname.clone(),
            proxy_port: s.proxy.port,
            proxy_username: s.proxy.username.clone().unwrap_or_default(),
            proxy_peer_connections: s.proxy.proxy_peer_connections,
            proxy_hostnames: s.proxy.proxy_hostnames,
            force_proxy: s.force_proxy,

            // M215: BitTorrent + Advanced round-trip.
            anonymous_mode: s.anonymous_mode,
            hashing_threads: s.hashing_threads as u32,
            save_resume_interval: s.save_resume_interval_secs,

            // M225: live IP filter / ban-list enable switch.
            ip_filter_enabled: s.ip_filter_enabled,

            // ── M228: M226 engine fields GET projection ─────────────
            notify_on_complete: s.notify_on_complete,
            notify_on_error: s.notify_on_error,
            autorun_program: s
                .on_complete_program
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            temp_path_enabled: s.use_incomplete_dir,
            temp_path: s
                .incomplete_dir
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            add_skip_check: s.default_skip_hash_check,
            incomplete_files_ext: s.incomplete_extension_enabled,
            scan_dirs_v2: s
                .watched_folder
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            auto_delete_mode: if s.delete_torrent_after_add { 2 } else { 0 },
            move_completed_enabled: s.move_completed_enabled,
            save_path_completed: s
                .move_completed_to
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            use_https: s.web_ui_https_enabled,
            current_network_interface: s.network_interface.clone().unwrap_or_default(),
            preallocate_all: matches!(
                s.preallocate_mode,
                Some(irontide::storage::PreallocateMode::Full)
            ),
            ip_filter_auto_refresh: s.ip_filter_auto_refresh,
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
        assert_eq!(
            QbtEncryption::try_from(0_u8).unwrap(),
            QbtEncryption::Prefer
        );
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
    fn max_uploads_per_torrent_projects_default_minus_one_sentinel() {
        // M224: default is `-1` ("unlimited"); the GET-side wire field must
        // emit it verbatim so qBt-compatible clients see the canonical
        // sentinel rather than `0` (which means "choke everyone" on POST).
        let s = base_settings();
        let p = QbtPreferences::from(&s);
        assert_eq!(p.max_uploads_per_torrent, -1);
    }

    #[test]
    fn max_uploads_per_torrent_projects_positive_cap_verbatim() {
        let mut s = base_settings();
        s.max_uploads_per_torrent = 6;
        let p = QbtPreferences::from(&s);
        assert_eq!(p.max_uploads_per_torrent, 6);
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
        assert_eq!(p.max_uploads_per_torrent, p2.max_uploads_per_torrent);
    }
}
