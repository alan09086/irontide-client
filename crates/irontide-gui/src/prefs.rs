//! Preferences view model (M184).
//!
//! `PreferencesState` is the **committed-state** snapshot. The Slint dialog
//! holds pending edits; on Apply/OK the Rust side reads the Slint properties,
//! diffs against the committed state, applies the changes, and updates the
//! committed snapshot.

use std::str::FromStr;

use crate::app::EnginePrefs;
use crate::format::format_rate;
use crate::skin::{self, Density, RadiusPreset, Skin, Theme};
use crate::speed::{format_kib_int, parse_kib_int};
// v0.187.3 / Step 3: `parse_rate_limit` + `format_rate_limit_display` remain
// in `speed.rs` for the per-torrent detail-pane override (an advanced surface
// where human-readable input is more ergonomic). The session-level prefs
// switched to numeric KiB/s via `parse_kib_int` / `format_kib_int`.

// v0.187.3 / 8A: per-field clamp ceilings. Negative input → 0 via the
// clamp's lower bound; upper ceilings prevent unbounded values from reaching
// the engine. PEERS_MAX mirrors `HARD_PEER_CEILING` in
// `irontide-session::torrent_peers` — keep in sync; the engine-side ceiling
// is the load-bearing invariant. `RATE_MAX_KIBPS` and `PORT_MAX` are kept
// alongside as the canonical clamp ceilings for future PrefNumeric molecule
// wire-up; `#[allow(dead_code)]` suppresses the unused-const warning until
// the molecule consumes them.
pub const PEERS_MAX: i32 = 4096;
pub const ACTIVE_MAX: i32 = 1024;
#[allow(dead_code)]
pub const RATE_MAX_KIBPS: u64 = u32::MAX as u64;
#[allow(dead_code)]
pub const PORT_MAX: u16 = 65535;

pub(crate) fn parse_int_pref<T>(s: &str, fallback: T, min: T, max: T) -> T
where
    T: FromStr + Ord + Copy,
{
    s.parse::<T>().unwrap_or(fallback).clamp(min, max)
}

// v0.187.3 / Step 3: legacy human-readable display helper. Retained behind
// `#[allow(dead_code)]` as a private utility for the per-torrent detail-pane
// override path; not used in this module after the session-level prefs
// switched to numeric KiB/s.
#[allow(dead_code)]
fn format_rate_limit_display(bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        return "Unlimited".to_owned();
    }
    format_rate(bytes_per_sec)
}

/// Committed preferences state. Aggregated from `SkinSettings` + `GuiConfig`.
#[derive(Debug, Clone)]
pub struct PreferencesState {
    // Interface
    pub skin: Skin,
    pub theme: Theme,
    pub density: Density,
    pub radius: RadiusPreset,
    pub confirm_delete: bool,
    pub confirm_pause_all: bool,
    pub show_torrent_added_toast: bool,
    pub double_click_action: String,
    // Startup
    pub start_minimized: bool,
    pub minimize_to_tray: bool,
    pub resume_previous_session: bool,
    // Notifications
    pub notify_on_complete: bool,
    pub notify_on_error: bool,
    pub notify_on_rss: bool,
    pub play_sound_on_complete: bool,
    pub on_complete_program: String,
    // Downloads
    pub download_dir: String,
    pub use_incomplete_dir: bool,
    pub incomplete_dir: String,
    pub create_subfolder: bool,
    pub pre_allocate: bool,
    pub show_add_torrent_dialog: bool,
    pub skip_hash_check: bool,
    pub incomplete_extension: bool,
    pub use_auto_categories: bool,
    pub append_date_to_path: bool,
    pub watched_folder: String,
    pub copy_torrent_to: String,
    pub delete_torrent_after_add: bool,
    pub move_completed_enabled: bool,
    pub move_completed_to: String,
    pub dl_on_complete_program: String,
    // Connection
    pub listen_port: u16,
    pub randomize_port: bool,
    pub enable_upnp: bool,
    pub enable_natpmp: bool,
    pub max_connections_global: i32,
    pub max_peers_per_torrent: i32,
    pub max_upload_slots_global: i32,
    pub max_upload_slots_per_torrent: i32,
    pub active_downloads: i32,
    pub active_seeds: i32,
    pub active_limit: i32,
    pub proxy_type: String,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub proxy_peer_connections: bool,
    pub proxy_hostnames: bool,
    pub ip_filter_enabled: bool,
    pub ip_filter_path: String,
    pub ip_filter_auto_refresh: bool,
    // Speed
    pub dl_limit_enabled: bool,
    pub dl_limit_value: u64,
    pub ul_limit_enabled: bool,
    pub ul_limit_value: u64,
    pub alt_dl_limit: u64,
    pub alt_ul_limit: u64,
    pub alt_speed_enabled: bool,
    pub rate_limit_overhead: bool,
    pub rate_limit_utp: bool,
    pub rate_limit_lan: bool,
    // BitTorrent
    pub enable_dht: bool,
    pub enable_pex: bool,
    pub enable_lsd: bool,
    pub encryption_mode: String,
    pub anonymous_mode: bool,
    pub seed_ratio_enabled: bool,
    pub seed_ratio_value: f64,
    pub max_ratio_action: String,
    pub seed_time_enabled: bool,
    pub seed_time_value: u64,
    pub inactive_seed_time_enabled: bool,
    pub inactive_seed_time_value: u64,
    pub queueing_enabled: bool,
    // RSS (not-yet-active, persisted to GuiConfig)
    pub rss_enabled: bool,
    pub rss_refresh_interval: u32,
    pub rss_max_articles: u32,
    pub rss_auto_download: bool,
    pub rss_smart_filter: bool,
    pub rss_download_repacks: bool,
    // Web UI
    pub webui_enabled: bool,
    pub webui_bind: String,
    pub webui_port: u16,
    pub webui_https: bool,
    pub webui_username: String,
    pub webui_bypass_local_auth: bool,
    pub webui_session_ttl: u64,
    pub webui_max_failed_auth: u32,
    pub webui_ban_duration: u64,
    pub webui_csrf: bool,
    pub webui_host_validation: bool,
    pub webui_reverse_proxy: bool,
    pub ddns_enabled: bool,
    pub ddns_service: String,
    pub ddns_domain: String,
    // Advanced
    pub hashing_threads: usize,
    pub save_resume_interval: u64,
    pub storage_mode: String,
    pub disk_cache_size: i32,
    pub enable_utp: bool,
    pub enable_fast_extension: bool,
    pub enable_holepunch: bool,
    pub enable_bep40: bool,
    pub config_path: String,
}

impl Default for PreferencesState {
    fn default() -> Self {
        Self {
            skin: Skin::default(),
            theme: Theme::default(),
            density: Density::default(),
            radius: RadiusPreset::default(),
            confirm_delete: true,
            confirm_pause_all: false,
            show_torrent_added_toast: true,
            double_click_action: "Show Details".to_owned(),
            start_minimized: false,
            minimize_to_tray: false,
            resume_previous_session: true,
            notify_on_complete: true,
            notify_on_error: true,
            notify_on_rss: false,
            play_sound_on_complete: false,
            on_complete_program: String::new(),
            download_dir: String::new(),
            use_incomplete_dir: false,
            incomplete_dir: String::new(),
            create_subfolder: true,
            pre_allocate: false,
            show_add_torrent_dialog: true,
            skip_hash_check: false,
            incomplete_extension: false,
            use_auto_categories: false,
            append_date_to_path: false,
            watched_folder: String::new(),
            copy_torrent_to: String::new(),
            delete_torrent_after_add: false,
            move_completed_enabled: false,
            move_completed_to: String::new(),
            dl_on_complete_program: String::new(),
            // Connection
            listen_port: 42020,
            randomize_port: false,
            enable_upnp: true,
            enable_natpmp: true,
            max_connections_global: -1,
            max_peers_per_torrent: 128,
            max_upload_slots_global: -1,
            max_upload_slots_per_torrent: 4,
            active_downloads: 3,
            active_seeds: 5,
            active_limit: 500,
            proxy_type: "None".to_owned(),
            proxy_host: String::new(),
            proxy_port: 0,
            proxy_peer_connections: true,
            proxy_hostnames: true,
            ip_filter_enabled: false,
            ip_filter_path: String::new(),
            ip_filter_auto_refresh: false,
            // Speed
            dl_limit_enabled: false,
            dl_limit_value: 0,
            ul_limit_enabled: false,
            ul_limit_value: 0,
            alt_dl_limit: 0,
            alt_ul_limit: 0,
            alt_speed_enabled: false,
            rate_limit_overhead: true,
            rate_limit_utp: true,
            rate_limit_lan: false,
            // BitTorrent
            enable_dht: true,
            enable_pex: true,
            enable_lsd: true,
            encryption_mode: "Disable encryption".to_owned(),
            anonymous_mode: false,
            seed_ratio_enabled: false,
            seed_ratio_value: 2.0,
            max_ratio_action: "Pause torrent".to_owned(),
            seed_time_enabled: false,
            seed_time_value: 1440,
            inactive_seed_time_enabled: false,
            inactive_seed_time_value: 60,
            queueing_enabled: false,
            // RSS
            rss_enabled: false,
            rss_refresh_interval: 15,
            rss_max_articles: 50,
            rss_auto_download: false,
            rss_smart_filter: false,
            rss_download_repacks: false,
            // Web UI
            webui_enabled: true,
            webui_bind: "127.0.0.1".to_owned(),
            webui_port: 9080,
            webui_https: false,
            webui_username: "admin".to_owned(),
            webui_bypass_local_auth: false,
            webui_session_ttl: 86400,
            webui_max_failed_auth: 5,
            webui_ban_duration: 3600,
            webui_csrf: true,
            webui_host_validation: true,
            webui_reverse_proxy: false,
            ddns_enabled: false,
            ddns_service: String::new(),
            ddns_domain: String::new(),
            // Advanced
            hashing_threads: 4,
            save_resume_interval: 5,
            storage_mode: "Auto".to_owned(),
            disk_cache_size: -1,
            enable_utp: true,
            enable_fast_extension: true,
            enable_holepunch: true,
            enable_bep40: true,
            config_path: String::new(),
        }
    }
}

fn encryption_mode_to_label(em: irontide::wire::mse::EncryptionMode) -> String {
    match em {
        irontide::wire::mse::EncryptionMode::Disabled => "Disable encryption",
        irontide::wire::mse::EncryptionMode::Forced => "Require encryption",
        irontide::wire::mse::EncryptionMode::Enabled
        | irontide::wire::mse::EncryptionMode::PreferPlaintext => "Prefer encryption",
    }
    .to_owned()
}

#[cfg(test)]
fn label_to_encryption_mode(label: &str) -> irontide::wire::mse::EncryptionMode {
    match label {
        "Prefer encryption" => irontide::wire::mse::EncryptionMode::Enabled,
        "Require encryption" => irontide::wire::mse::EncryptionMode::Forced,
        _ => irontide::wire::mse::EncryptionMode::Disabled,
    }
}

fn max_ratio_action_to_label(a: irontide::session::MaxRatioAction) -> String {
    match a {
        irontide::session::MaxRatioAction::Pause => "Pause torrent",
        irontide::session::MaxRatioAction::Remove => "Remove torrent",
        irontide::session::MaxRatioAction::EnableSuperSeeding => "Super-seeding mode",
    }
    .to_owned()
}

#[cfg(test)]
fn label_to_max_ratio_action(label: &str) -> irontide::session::MaxRatioAction {
    match label {
        "Remove torrent" => irontide::session::MaxRatioAction::Remove,
        "Super-seeding mode" => irontide::session::MaxRatioAction::EnableSuperSeeding,
        _ => irontide::session::MaxRatioAction::Pause,
    }
}

fn storage_mode_to_label(sm: irontide::core::StorageMode) -> String {
    format!("{sm:?}")
}

fn proxy_type_to_label(pt: &irontide::session::ProxyType) -> String {
    match pt {
        irontide::session::ProxyType::None => "None",
        irontide::session::ProxyType::Socks4 => "SOCKS4",
        irontide::session::ProxyType::Socks5 => "SOCKS5",
        irontide::session::ProxyType::Socks5Password => "SOCKS5 (password)",
        irontide::session::ProxyType::Http => "HTTP",
        irontide::session::ProxyType::HttpPassword => "HTTP (password)",
    }
    .to_owned()
}

impl PreferencesState {
    /// Aggregate from the three configuration sources.
    #[must_use]
    pub fn from_app(
        skin_settings: skin::SkinSettings,
        gui: &irontide_config::GuiConfig,
        download_dir: &str,
        settings: &irontide::session::Settings,
    ) -> Self {
        Self {
            skin: skin_settings.skin,
            theme: skin_settings.theme,
            density: skin_settings.density,
            radius: skin_settings.radius,
            confirm_delete: gui.confirm_delete.unwrap_or(true),
            confirm_pause_all: gui.confirm_pause_all.unwrap_or(false),
            show_torrent_added_toast: gui.show_torrent_added_toast.unwrap_or(true),
            double_click_action: gui
                .double_click_action
                .clone()
                .unwrap_or_else(|| "Show Details".to_owned()),
            start_minimized: gui.start_minimized.unwrap_or(false),
            minimize_to_tray: gui.minimize_to_tray.unwrap_or(false),
            resume_previous_session: gui.resume_previous_session.unwrap_or(true),
            notify_on_complete: gui.notify_on_complete.unwrap_or(true),
            notify_on_error: gui.notify_on_error.unwrap_or(true),
            notify_on_rss: gui.notify_on_rss.unwrap_or(false),
            play_sound_on_complete: gui.play_sound_on_complete.unwrap_or(false),
            on_complete_program: gui.on_complete_program.clone().unwrap_or_default(),
            download_dir: download_dir.to_owned(),
            use_incomplete_dir: gui.use_incomplete_dir.unwrap_or(false),
            incomplete_dir: gui.incomplete_dir.clone().unwrap_or_default(),
            create_subfolder: gui.create_subfolder.unwrap_or(true),
            pre_allocate: gui.pre_allocate.unwrap_or(false),
            show_add_torrent_dialog: gui.show_add_torrent_dialog.unwrap_or(true),
            skip_hash_check: gui.skip_hash_check.unwrap_or(false),
            incomplete_extension: gui.incomplete_extension.unwrap_or(false),
            use_auto_categories: gui.use_auto_categories.unwrap_or(false),
            append_date_to_path: gui.append_date_to_path.unwrap_or(false),
            watched_folder: gui.watched_folder.clone().unwrap_or_default(),
            copy_torrent_to: gui.copy_torrent_to.clone().unwrap_or_default(),
            delete_torrent_after_add: gui.delete_torrent_after_add.unwrap_or(false),
            move_completed_enabled: gui.move_completed_enabled.unwrap_or(false),
            move_completed_to: gui.move_completed_to.clone().unwrap_or_default(),
            dl_on_complete_program: gui.dl_on_complete_program.clone().unwrap_or_default(),
            // Connection — from engine Settings
            listen_port: settings.listen_port,
            randomize_port: settings.randomize_port_on_startup,
            enable_upnp: settings.enable_upnp,
            enable_natpmp: settings.enable_natpmp,
            max_connections_global: settings.max_connections_global,
            max_peers_per_torrent: i32::try_from(settings.max_peers_per_torrent)
                .unwrap_or(i32::MAX),
            max_upload_slots_global: settings.max_upload_slots_global,
            max_upload_slots_per_torrent: settings.max_upload_slots_per_torrent,
            active_downloads: settings.active_downloads,
            active_seeds: settings.active_seeds,
            active_limit: settings.active_limit,
            proxy_type: proxy_type_to_label(&settings.proxy.proxy_type),
            proxy_host: settings.proxy.hostname.clone(),
            proxy_port: settings.proxy.port,
            proxy_peer_connections: settings.proxy.proxy_peer_connections,
            proxy_hostnames: settings.proxy.proxy_hostnames,
            ip_filter_enabled: gui.ip_filter_enabled.unwrap_or(settings.ip_filter_enabled),
            ip_filter_path: gui
                .ip_filter_path
                .clone()
                .unwrap_or_else(|| settings.ip_filter_path.clone()),
            ip_filter_auto_refresh: gui
                .ip_filter_auto_refresh
                .unwrap_or(settings.ip_filter_auto_refresh),
            // Speed — GUI-owned toggles + engine values
            dl_limit_enabled: gui
                .dl_limit_enabled
                .unwrap_or(settings.download_rate_limit > 0),
            dl_limit_value: gui.dl_limit_value.unwrap_or(settings.download_rate_limit),
            ul_limit_enabled: gui
                .ul_limit_enabled
                .unwrap_or(settings.upload_rate_limit > 0),
            ul_limit_value: gui.ul_limit_value.unwrap_or(settings.upload_rate_limit),
            alt_dl_limit: gui.alt_dl_limit.unwrap_or(settings.alt_download_rate_limit),
            alt_ul_limit: gui.alt_ul_limit.unwrap_or(settings.alt_upload_rate_limit),
            alt_speed_enabled: gui.alt_speed_enabled.unwrap_or(settings.alt_speed_enabled),
            rate_limit_overhead: settings.rate_limit_includes_overhead,
            rate_limit_utp: settings.rate_limit_utp,
            rate_limit_lan: settings.rate_limit_lan,
            // BitTorrent — from engine Settings
            enable_dht: settings.enable_dht,
            enable_pex: settings.enable_pex,
            enable_lsd: settings.enable_lsd,
            encryption_mode: encryption_mode_to_label(settings.encryption_mode),
            anonymous_mode: settings.anonymous_mode,
            seed_ratio_enabled: settings.seed_ratio_limit.is_some(),
            seed_ratio_value: settings.seed_ratio_limit.unwrap_or(2.0),
            max_ratio_action: max_ratio_action_to_label(settings.max_ratio_action),
            seed_time_enabled: settings.seed_time_limit_secs.is_some(),
            seed_time_value: settings.seed_time_limit_secs.unwrap_or(86400) / 60,
            inactive_seed_time_enabled: settings.inactive_seed_time_limit_secs.is_some(),
            inactive_seed_time_value: settings.inactive_seed_time_limit_secs.unwrap_or(3600) / 60,
            queueing_enabled: settings.queueing_enabled,
            // RSS — from GuiConfig (not-yet-active)
            rss_enabled: gui.rss_enabled.unwrap_or(false),
            rss_refresh_interval: gui.rss_refresh_interval_min.unwrap_or(15),
            rss_max_articles: gui.rss_max_articles.unwrap_or(50),
            rss_auto_download: gui.rss_auto_download.unwrap_or(false),
            rss_smart_filter: gui.rss_smart_filter.unwrap_or(false),
            rss_download_repacks: gui.rss_download_repacks.unwrap_or(false),
            // Web UI — from QbtCompatSettings
            webui_enabled: settings.qbt_compat.enabled,
            webui_bind: String::new(),
            webui_port: 0,
            webui_https: gui.webui_https.unwrap_or(false),
            webui_username: settings.qbt_compat.username.clone(),
            webui_bypass_local_auth: settings.qbt_compat.bypass_local_auth,
            webui_session_ttl: settings.qbt_compat.session_ttl_secs,
            webui_max_failed_auth: settings.qbt_compat.max_failed_auth_count,
            webui_ban_duration: settings.qbt_compat.ban_duration_secs,
            webui_csrf: settings.qbt_compat.csrf_protection_enabled,
            webui_host_validation: settings.qbt_compat.host_header_validation_enabled,
            webui_reverse_proxy: settings.qbt_compat.web_ui_reverse_proxy_enabled,
            ddns_enabled: gui.ddns_enabled.unwrap_or(false),
            ddns_service: gui.ddns_service.clone().unwrap_or_default(),
            ddns_domain: gui.ddns_domain.clone().unwrap_or_default(),
            // Advanced — from engine Settings
            hashing_threads: settings.hashing_threads,
            save_resume_interval: settings.save_resume_interval_secs / 60,
            storage_mode: storage_mode_to_label(settings.storage_mode),
            disk_cache_size: gui.disk_cache_size.unwrap_or(-1),
            enable_utp: settings.enable_utp,
            enable_fast_extension: settings.enable_fast_extension,
            enable_holepunch: settings.enable_holepunch,
            enable_bep40: settings.enable_bep40_eviction,
            config_path: irontide_config::resolve_config_path(None)
                .to_string_lossy()
                .into_owned(),
        }
    }

    /// Push committed state into Slint dialog properties.
    pub fn populate_slint(&self, win: &crate::MainWindow) {
        win.set_pref_skin(self.skin.to_string().into());
        win.set_pref_theme(self.theme.to_string().into());
        win.set_pref_density(self.density.to_string().into());
        win.set_pref_radius(self.radius.to_string().into());
        win.set_pref_confirm_delete(self.confirm_delete);
        win.set_pref_confirm_pause_all(self.confirm_pause_all);
        win.set_pref_show_torrent_added_toast(self.show_torrent_added_toast);
        win.set_pref_double_click_action(self.double_click_action.as_str().into());
        win.set_pref_start_minimized(self.start_minimized);
        win.set_pref_minimize_to_tray(self.minimize_to_tray);
        win.set_pref_resume_previous_session(self.resume_previous_session);
        win.set_pref_notify_on_complete(self.notify_on_complete);
        win.set_pref_notify_on_error(self.notify_on_error);
        win.set_pref_notify_on_rss(self.notify_on_rss);
        win.set_pref_play_sound_on_complete(self.play_sound_on_complete);
        win.set_pref_on_complete_program(self.on_complete_program.as_str().into());
        win.set_pref_download_dir(self.download_dir.as_str().into());
        win.set_pref_use_incomplete_dir(self.use_incomplete_dir);
        win.set_pref_incomplete_dir(self.incomplete_dir.as_str().into());
        win.set_pref_create_subfolder(self.create_subfolder);
        win.set_pref_pre_allocate(self.pre_allocate);
        win.set_pref_show_add_torrent_dialog(self.show_add_torrent_dialog);
        win.set_pref_skip_hash_check(self.skip_hash_check);
        win.set_pref_incomplete_extension(self.incomplete_extension);
        win.set_pref_use_auto_categories(self.use_auto_categories);
        win.set_pref_append_date_to_path(self.append_date_to_path);
        win.set_pref_watched_folder(self.watched_folder.as_str().into());
        win.set_pref_copy_torrent_to(self.copy_torrent_to.as_str().into());
        win.set_pref_delete_torrent_after_add(self.delete_torrent_after_add);
        win.set_pref_move_completed_enabled(self.move_completed_enabled);
        win.set_pref_move_completed_to(self.move_completed_to.as_str().into());
        win.set_pref_dl_on_complete_program(self.dl_on_complete_program.as_str().into());
        // Connection
        win.set_pref_listen_port(self.listen_port.to_string().into());
        win.set_pref_randomize_port(self.randomize_port);
        win.set_pref_enable_upnp(self.enable_upnp);
        win.set_pref_enable_natpmp(self.enable_natpmp);
        win.set_pref_max_connections_global(self.max_connections_global.to_string().into());
        win.set_pref_max_peers_per_torrent(self.max_peers_per_torrent.to_string().into());
        win.set_pref_max_upload_slots_global(self.max_upload_slots_global.to_string().into());
        win.set_pref_max_upload_slots_per_torrent(
            self.max_upload_slots_per_torrent.to_string().into(),
        );
        win.set_pref_active_downloads(self.active_downloads.to_string().into());
        win.set_pref_active_seeds(self.active_seeds.to_string().into());
        win.set_pref_active_limit(self.active_limit.to_string().into());
        win.set_pref_proxy_type(self.proxy_type.as_str().into());
        win.set_pref_proxy_host(self.proxy_host.as_str().into());
        win.set_pref_proxy_port(self.proxy_port.to_string().into());
        win.set_pref_proxy_peer_connections(self.proxy_peer_connections);
        win.set_pref_proxy_hostnames(self.proxy_hostnames);
        win.set_pref_ip_filter_enabled(self.ip_filter_enabled);
        win.set_pref_ip_filter_path(self.ip_filter_path.as_str().into());
        win.set_pref_ip_filter_auto_refresh(self.ip_filter_auto_refresh);
        // Speed — v0.187.3 / Step 3: switch the session-level rate limits to
        // the numeric KiB/s display convention (0 = Unlimited). Per-torrent
        // overrides in the detail pane keep the legacy human-readable form
        // (`parse_rate_limit` / `format_rate_limit_display`) since they're
        // an advanced surface where "1.5 MiB/s" is more ergonomic.
        win.set_pref_dl_limit_enabled(self.dl_limit_enabled);
        win.set_pref_dl_limit_value(format_kib_int(self.dl_limit_value).into());
        win.set_pref_ul_limit_enabled(self.ul_limit_enabled);
        win.set_pref_ul_limit_value(format_kib_int(self.ul_limit_value).into());
        win.set_pref_alt_dl_limit(format_kib_int(self.alt_dl_limit).into());
        win.set_pref_alt_ul_limit(format_kib_int(self.alt_ul_limit).into());
        win.set_pref_alt_speed_enabled(self.alt_speed_enabled);
        win.set_pref_rate_limit_overhead(self.rate_limit_overhead);
        win.set_pref_rate_limit_utp(self.rate_limit_utp);
        win.set_pref_rate_limit_lan(self.rate_limit_lan);
        // BitTorrent
        win.set_pref_enable_dht(self.enable_dht);
        win.set_pref_enable_pex(self.enable_pex);
        win.set_pref_enable_lsd(self.enable_lsd);
        win.set_pref_encryption_mode(self.encryption_mode.as_str().into());
        win.set_pref_anonymous_mode(self.anonymous_mode);
        win.set_pref_seed_ratio_enabled(self.seed_ratio_enabled);
        win.set_pref_seed_ratio_value(format!("{:.2}", self.seed_ratio_value).into());
        win.set_pref_max_ratio_action(self.max_ratio_action.as_str().into());
        win.set_pref_seed_time_enabled(self.seed_time_enabled);
        win.set_pref_seed_time_value(self.seed_time_value.to_string().into());
        win.set_pref_inactive_seed_time_enabled(self.inactive_seed_time_enabled);
        win.set_pref_inactive_seed_time_value(self.inactive_seed_time_value.to_string().into());
        win.set_pref_queueing_enabled(self.queueing_enabled);
        // RSS
        win.set_pref_rss_enabled(self.rss_enabled);
        win.set_pref_rss_refresh_interval(self.rss_refresh_interval.to_string().into());
        win.set_pref_rss_max_articles(self.rss_max_articles.to_string().into());
        win.set_pref_rss_auto_download(self.rss_auto_download);
        win.set_pref_rss_smart_filter(self.rss_smart_filter);
        win.set_pref_rss_download_repacks(self.rss_download_repacks);
        // Web UI
        win.set_pref_webui_enabled(self.webui_enabled);
        win.set_pref_webui_bind(self.webui_bind.as_str().into());
        win.set_pref_webui_port(self.webui_port.to_string().into());
        win.set_pref_webui_https(self.webui_https);
        win.set_pref_webui_username(self.webui_username.as_str().into());
        win.set_pref_webui_bypass_local_auth(self.webui_bypass_local_auth);
        win.set_pref_webui_session_ttl(self.webui_session_ttl.to_string().into());
        win.set_pref_webui_max_failed_auth(self.webui_max_failed_auth.to_string().into());
        win.set_pref_webui_ban_duration(self.webui_ban_duration.to_string().into());
        win.set_pref_webui_csrf(self.webui_csrf);
        win.set_pref_webui_host_validation(self.webui_host_validation);
        win.set_pref_webui_reverse_proxy(self.webui_reverse_proxy);
        win.set_pref_ddns_enabled(self.ddns_enabled);
        win.set_pref_ddns_service(self.ddns_service.as_str().into());
        win.set_pref_ddns_domain(self.ddns_domain.as_str().into());
        // Advanced
        win.set_pref_hashing_threads(self.hashing_threads.to_string().into());
        win.set_pref_save_resume_interval(self.save_resume_interval.to_string().into());
        win.set_pref_storage_mode(self.storage_mode.as_str().into());
        win.set_pref_disk_cache_size(self.disk_cache_size.to_string().into());
        win.set_pref_enable_utp(self.enable_utp);
        win.set_pref_enable_fast_ext(self.enable_fast_extension);
        win.set_pref_enable_holepunch(self.enable_holepunch);
        win.set_pref_enable_bep40(self.enable_bep40);
        win.set_pref_config_path(self.config_path.as_str().into());
        win.set_pref_dirty(false);
    }

    #[must_use]
    #[cfg(test)]
    pub fn reset_tab(&mut self, tab: &str) -> bool {
        let d = Self::default();
        match tab {
            "behavior" => {
                self.skin = d.skin;
                self.theme = d.theme;
                self.density = d.density;
                self.radius = d.radius;
                self.confirm_delete = d.confirm_delete;
                self.confirm_pause_all = d.confirm_pause_all;
                self.show_torrent_added_toast = d.show_torrent_added_toast;
                self.double_click_action = d.double_click_action;
                self.start_minimized = d.start_minimized;
                self.minimize_to_tray = d.minimize_to_tray;
                self.resume_previous_session = d.resume_previous_session;
                self.notify_on_complete = d.notify_on_complete;
                self.notify_on_error = d.notify_on_error;
                self.notify_on_rss = d.notify_on_rss;
                self.play_sound_on_complete = d.play_sound_on_complete;
                self.on_complete_program = d.on_complete_program;
                true
            }
            "downloads" => {
                self.download_dir = d.download_dir;
                self.use_incomplete_dir = d.use_incomplete_dir;
                self.incomplete_dir = d.incomplete_dir;
                self.create_subfolder = d.create_subfolder;
                self.pre_allocate = d.pre_allocate;
                self.show_add_torrent_dialog = d.show_add_torrent_dialog;
                self.skip_hash_check = d.skip_hash_check;
                self.incomplete_extension = d.incomplete_extension;
                self.use_auto_categories = d.use_auto_categories;
                self.append_date_to_path = d.append_date_to_path;
                self.watched_folder = d.watched_folder;
                self.copy_torrent_to = d.copy_torrent_to;
                self.delete_torrent_after_add = d.delete_torrent_after_add;
                self.move_completed_enabled = d.move_completed_enabled;
                self.move_completed_to = d.move_completed_to;
                self.dl_on_complete_program = d.dl_on_complete_program;
                true
            }
            "connection" => {
                self.listen_port = d.listen_port;
                self.randomize_port = d.randomize_port;
                self.enable_upnp = d.enable_upnp;
                self.enable_natpmp = d.enable_natpmp;
                self.max_connections_global = d.max_connections_global;
                self.max_peers_per_torrent = d.max_peers_per_torrent;
                self.max_upload_slots_global = d.max_upload_slots_global;
                self.max_upload_slots_per_torrent = d.max_upload_slots_per_torrent;
                self.active_downloads = d.active_downloads;
                self.active_seeds = d.active_seeds;
                self.active_limit = d.active_limit;
                self.proxy_type = d.proxy_type;
                self.proxy_host = d.proxy_host;
                self.proxy_port = d.proxy_port;
                self.proxy_peer_connections = d.proxy_peer_connections;
                self.proxy_hostnames = d.proxy_hostnames;
                self.ip_filter_enabled = d.ip_filter_enabled;
                self.ip_filter_path = d.ip_filter_path;
                self.ip_filter_auto_refresh = d.ip_filter_auto_refresh;
                true
            }
            "speed" => {
                self.dl_limit_enabled = d.dl_limit_enabled;
                self.dl_limit_value = d.dl_limit_value;
                self.ul_limit_enabled = d.ul_limit_enabled;
                self.ul_limit_value = d.ul_limit_value;
                self.alt_dl_limit = d.alt_dl_limit;
                self.alt_ul_limit = d.alt_ul_limit;
                self.alt_speed_enabled = d.alt_speed_enabled;
                self.rate_limit_overhead = d.rate_limit_overhead;
                self.rate_limit_utp = d.rate_limit_utp;
                self.rate_limit_lan = d.rate_limit_lan;
                true
            }
            "bittorrent" => {
                self.enable_dht = d.enable_dht;
                self.enable_pex = d.enable_pex;
                self.enable_lsd = d.enable_lsd;
                self.encryption_mode = d.encryption_mode;
                self.anonymous_mode = d.anonymous_mode;
                self.seed_ratio_enabled = d.seed_ratio_enabled;
                self.seed_ratio_value = d.seed_ratio_value;
                self.max_ratio_action = d.max_ratio_action;
                self.seed_time_enabled = d.seed_time_enabled;
                self.seed_time_value = d.seed_time_value;
                self.inactive_seed_time_enabled = d.inactive_seed_time_enabled;
                self.inactive_seed_time_value = d.inactive_seed_time_value;
                self.queueing_enabled = d.queueing_enabled;
                true
            }
            "rss" => {
                self.rss_enabled = d.rss_enabled;
                self.rss_refresh_interval = d.rss_refresh_interval;
                self.rss_max_articles = d.rss_max_articles;
                self.rss_auto_download = d.rss_auto_download;
                self.rss_smart_filter = d.rss_smart_filter;
                self.rss_download_repacks = d.rss_download_repacks;
                true
            }
            "webui" => {
                self.webui_enabled = d.webui_enabled;
                self.webui_bind = d.webui_bind;
                self.webui_port = d.webui_port;
                self.webui_https = d.webui_https;
                self.webui_username = d.webui_username;
                self.webui_bypass_local_auth = d.webui_bypass_local_auth;
                self.webui_session_ttl = d.webui_session_ttl;
                self.webui_max_failed_auth = d.webui_max_failed_auth;
                self.webui_ban_duration = d.webui_ban_duration;
                self.webui_csrf = d.webui_csrf;
                self.webui_host_validation = d.webui_host_validation;
                self.webui_reverse_proxy = d.webui_reverse_proxy;
                self.ddns_enabled = d.ddns_enabled;
                self.ddns_service = d.ddns_service;
                self.ddns_domain = d.ddns_domain;
                true
            }
            "advanced" => {
                self.hashing_threads = d.hashing_threads;
                self.save_resume_interval = d.save_resume_interval;
                self.storage_mode = d.storage_mode;
                self.disk_cache_size = d.disk_cache_size;
                self.enable_utp = d.enable_utp;
                self.enable_fast_extension = d.enable_fast_extension;
                self.enable_holepunch = d.enable_holepunch;
                self.enable_bep40 = d.enable_bep40;
                true
            }
            _ => false,
        }
    }

    pub fn populate_slint_tab(&self, tab: &str, win: &crate::MainWindow) {
        match tab {
            "behavior" => {
                win.set_pref_skin(self.skin.to_string().into());
                win.set_pref_theme(self.theme.to_string().into());
                win.set_pref_density(self.density.to_string().into());
                win.set_pref_radius(self.radius.to_string().into());
                win.set_pref_confirm_delete(self.confirm_delete);
                win.set_pref_confirm_pause_all(self.confirm_pause_all);
                win.set_pref_show_torrent_added_toast(self.show_torrent_added_toast);
                win.set_pref_double_click_action(self.double_click_action.as_str().into());
                win.set_pref_start_minimized(self.start_minimized);
                win.set_pref_minimize_to_tray(self.minimize_to_tray);
                win.set_pref_resume_previous_session(self.resume_previous_session);
                win.set_pref_notify_on_complete(self.notify_on_complete);
                win.set_pref_notify_on_error(self.notify_on_error);
                win.set_pref_notify_on_rss(self.notify_on_rss);
                win.set_pref_play_sound_on_complete(self.play_sound_on_complete);
                win.set_pref_on_complete_program(self.on_complete_program.as_str().into());
            }
            "downloads" => {
                win.set_pref_download_dir(self.download_dir.as_str().into());
                win.set_pref_use_incomplete_dir(self.use_incomplete_dir);
                win.set_pref_incomplete_dir(self.incomplete_dir.as_str().into());
                win.set_pref_create_subfolder(self.create_subfolder);
                win.set_pref_pre_allocate(self.pre_allocate);
                win.set_pref_show_add_torrent_dialog(self.show_add_torrent_dialog);
                win.set_pref_skip_hash_check(self.skip_hash_check);
                win.set_pref_incomplete_extension(self.incomplete_extension);
                win.set_pref_use_auto_categories(self.use_auto_categories);
                win.set_pref_append_date_to_path(self.append_date_to_path);
                win.set_pref_watched_folder(self.watched_folder.as_str().into());
                win.set_pref_copy_torrent_to(self.copy_torrent_to.as_str().into());
                win.set_pref_delete_torrent_after_add(self.delete_torrent_after_add);
                win.set_pref_move_completed_enabled(self.move_completed_enabled);
                win.set_pref_move_completed_to(self.move_completed_to.as_str().into());
                win.set_pref_dl_on_complete_program(self.dl_on_complete_program.as_str().into());
            }
            "connection" => {
                win.set_pref_listen_port(self.listen_port.to_string().into());
                win.set_pref_randomize_port(self.randomize_port);
                win.set_pref_enable_upnp(self.enable_upnp);
                win.set_pref_enable_natpmp(self.enable_natpmp);
                win.set_pref_max_connections_global(self.max_connections_global.to_string().into());
                win.set_pref_max_peers_per_torrent(self.max_peers_per_torrent.to_string().into());
                win.set_pref_max_upload_slots_global(
                    self.max_upload_slots_global.to_string().into(),
                );
                win.set_pref_max_upload_slots_per_torrent(
                    self.max_upload_slots_per_torrent.to_string().into(),
                );
                win.set_pref_active_downloads(self.active_downloads.to_string().into());
                win.set_pref_active_seeds(self.active_seeds.to_string().into());
                win.set_pref_active_limit(self.active_limit.to_string().into());
                win.set_pref_proxy_type(self.proxy_type.as_str().into());
                win.set_pref_proxy_host(self.proxy_host.as_str().into());
                win.set_pref_proxy_port(self.proxy_port.to_string().into());
                win.set_pref_proxy_peer_connections(self.proxy_peer_connections);
                win.set_pref_proxy_hostnames(self.proxy_hostnames);
                win.set_pref_ip_filter_enabled(self.ip_filter_enabled);
                win.set_pref_ip_filter_path(self.ip_filter_path.as_str().into());
                win.set_pref_ip_filter_auto_refresh(self.ip_filter_auto_refresh);
            }
            "speed" => {
                win.set_pref_dl_limit_enabled(self.dl_limit_enabled);
                win.set_pref_dl_limit_value(format_kib_int(self.dl_limit_value).into());
                win.set_pref_ul_limit_enabled(self.ul_limit_enabled);
                win.set_pref_ul_limit_value(format_kib_int(self.ul_limit_value).into());
                win.set_pref_alt_dl_limit(format_kib_int(self.alt_dl_limit).into());
                win.set_pref_alt_ul_limit(format_kib_int(self.alt_ul_limit).into());
                win.set_pref_alt_speed_enabled(self.alt_speed_enabled);
                win.set_pref_rate_limit_overhead(self.rate_limit_overhead);
                win.set_pref_rate_limit_utp(self.rate_limit_utp);
                win.set_pref_rate_limit_lan(self.rate_limit_lan);
            }
            "bittorrent" => {
                win.set_pref_enable_dht(self.enable_dht);
                win.set_pref_enable_pex(self.enable_pex);
                win.set_pref_enable_lsd(self.enable_lsd);
                win.set_pref_encryption_mode(self.encryption_mode.as_str().into());
                win.set_pref_anonymous_mode(self.anonymous_mode);
                win.set_pref_seed_ratio_enabled(self.seed_ratio_enabled);
                win.set_pref_seed_ratio_value(format!("{:.2}", self.seed_ratio_value).into());
                win.set_pref_max_ratio_action(self.max_ratio_action.as_str().into());
                win.set_pref_seed_time_enabled(self.seed_time_enabled);
                win.set_pref_seed_time_value(self.seed_time_value.to_string().into());
                win.set_pref_inactive_seed_time_enabled(self.inactive_seed_time_enabled);
                win.set_pref_inactive_seed_time_value(
                    self.inactive_seed_time_value.to_string().into(),
                );
                win.set_pref_queueing_enabled(self.queueing_enabled);
            }
            "rss" => {
                win.set_pref_rss_enabled(self.rss_enabled);
                win.set_pref_rss_refresh_interval(self.rss_refresh_interval.to_string().into());
                win.set_pref_rss_max_articles(self.rss_max_articles.to_string().into());
                win.set_pref_rss_auto_download(self.rss_auto_download);
                win.set_pref_rss_smart_filter(self.rss_smart_filter);
                win.set_pref_rss_download_repacks(self.rss_download_repacks);
            }
            "webui" => {
                win.set_pref_webui_enabled(self.webui_enabled);
                win.set_pref_webui_bind(self.webui_bind.as_str().into());
                win.set_pref_webui_port(self.webui_port.to_string().into());
                win.set_pref_webui_https(self.webui_https);
                win.set_pref_webui_username(self.webui_username.as_str().into());
                win.set_pref_webui_bypass_local_auth(self.webui_bypass_local_auth);
                win.set_pref_webui_session_ttl(self.webui_session_ttl.to_string().into());
                win.set_pref_webui_max_failed_auth(self.webui_max_failed_auth.to_string().into());
                win.set_pref_webui_ban_duration(self.webui_ban_duration.to_string().into());
                win.set_pref_webui_csrf(self.webui_csrf);
                win.set_pref_webui_host_validation(self.webui_host_validation);
                win.set_pref_webui_reverse_proxy(self.webui_reverse_proxy);
                win.set_pref_ddns_enabled(self.ddns_enabled);
                win.set_pref_ddns_service(self.ddns_service.as_str().into());
                win.set_pref_ddns_domain(self.ddns_domain.as_str().into());
            }
            "advanced" => {
                win.set_pref_hashing_threads(self.hashing_threads.to_string().into());
                win.set_pref_save_resume_interval(self.save_resume_interval.to_string().into());
                win.set_pref_storage_mode(self.storage_mode.as_str().into());
                win.set_pref_disk_cache_size(self.disk_cache_size.to_string().into());
                win.set_pref_enable_utp(self.enable_utp);
                win.set_pref_enable_fast_ext(self.enable_fast_extension);
                win.set_pref_enable_holepunch(self.enable_holepunch);
                win.set_pref_enable_bep40(self.enable_bep40);
                win.set_pref_config_path(self.config_path.as_str().into());
            }
            _ => {}
        }
    }

    /// Read Slint dialog properties, diff against committed state, apply
    /// changes, and return what changed.
    pub fn diff_and_apply(&mut self, win: &crate::MainWindow) -> ApplyResult {
        let mut result = ApplyResult::default();

        // Skin axis changes
        let new_skin = Skin::from_str(win.get_pref_skin().as_str()).unwrap_or(self.skin);
        let new_theme = Theme::from_str(win.get_pref_theme().as_str()).unwrap_or(self.theme);
        let new_density =
            Density::from_str(win.get_pref_density().as_str()).unwrap_or(self.density);
        let new_radius =
            RadiusPreset::from_str(win.get_pref_radius().as_str()).unwrap_or(self.radius);

        if new_skin != self.skin
            || new_theme != self.theme
            || new_density != self.density
            || new_radius != self.radius
        {
            self.skin = new_skin;
            self.theme = new_theme;
            self.density = new_density;
            self.radius = new_radius;
            result.skin_changed = true;
        }

        // Behaviour bools
        self.confirm_delete = win.get_pref_confirm_delete();
        self.confirm_pause_all = win.get_pref_confirm_pause_all();
        self.show_torrent_added_toast = win.get_pref_show_torrent_added_toast();
        self.double_click_action = win.get_pref_double_click_action().to_string();
        self.start_minimized = win.get_pref_start_minimized();
        self.minimize_to_tray = win.get_pref_minimize_to_tray();
        self.resume_previous_session = win.get_pref_resume_previous_session();
        self.notify_on_complete = win.get_pref_notify_on_complete();
        self.notify_on_error = win.get_pref_notify_on_error();
        self.notify_on_rss = win.get_pref_notify_on_rss();
        self.play_sound_on_complete = win.get_pref_play_sound_on_complete();
        self.on_complete_program = win.get_pref_on_complete_program().to_string();

        // Downloads
        let new_download_dir = win.get_pref_download_dir().to_string();
        if new_download_dir != self.download_dir {
            result.download_dir = Some(new_download_dir.clone());
            self.download_dir = new_download_dir;
        }
        let new_create_subfolder = win.get_pref_create_subfolder();
        if new_create_subfolder != self.create_subfolder {
            result.create_subfolder = Some(new_create_subfolder);
            self.create_subfolder = new_create_subfolder;
        }

        self.use_incomplete_dir = win.get_pref_use_incomplete_dir();
        self.incomplete_dir = win.get_pref_incomplete_dir().to_string();
        self.pre_allocate = win.get_pref_pre_allocate();
        self.show_add_torrent_dialog = win.get_pref_show_add_torrent_dialog();
        self.skip_hash_check = win.get_pref_skip_hash_check();
        self.incomplete_extension = win.get_pref_incomplete_extension();
        self.use_auto_categories = win.get_pref_use_auto_categories();
        self.append_date_to_path = win.get_pref_append_date_to_path();
        self.watched_folder = win.get_pref_watched_folder().to_string();
        self.copy_torrent_to = win.get_pref_copy_torrent_to().to_string();
        self.delete_torrent_after_add = win.get_pref_delete_torrent_after_add();
        self.move_completed_enabled = win.get_pref_move_completed_enabled();
        self.move_completed_to = win.get_pref_move_completed_to().to_string();
        self.dl_on_complete_program = win.get_pref_dl_on_complete_program().to_string();

        // Connection
        let new_listen_port: u16 = win
            .get_pref_listen_port()
            .as_str()
            .parse()
            .unwrap_or(self.listen_port);
        let new_randomize_port = win.get_pref_randomize_port();
        let new_enable_upnp = win.get_pref_enable_upnp();
        let new_enable_natpmp = win.get_pref_enable_natpmp();
        let new_max_conn_global: i32 = parse_int_pref(
            win.get_pref_max_connections_global().as_str(),
            self.max_connections_global,
            0,
            PEERS_MAX,
        );
        // Bug 7: clamp at input — negative values silently became "unlimited"
        // pre-v0.187.3, which exhausted FDs under load.
        let new_max_peers: i32 = parse_int_pref(
            win.get_pref_max_peers_per_torrent().as_str(),
            self.max_peers_per_torrent,
            0,
            PEERS_MAX,
        );
        let new_max_ul_slots_global: i32 = parse_int_pref(
            win.get_pref_max_upload_slots_global().as_str(),
            self.max_upload_slots_global,
            0,
            PEERS_MAX,
        );
        let new_max_ul_slots_per: i32 = parse_int_pref(
            win.get_pref_max_upload_slots_per_torrent().as_str(),
            self.max_upload_slots_per_torrent,
            0,
            PEERS_MAX,
        );
        let new_active_dl: i32 = parse_int_pref(
            win.get_pref_active_downloads().as_str(),
            self.active_downloads,
            0,
            ACTIVE_MAX,
        );
        let new_active_seeds: i32 = parse_int_pref(
            win.get_pref_active_seeds().as_str(),
            self.active_seeds,
            0,
            ACTIVE_MAX,
        );
        let new_active_limit: i32 = parse_int_pref(
            win.get_pref_active_limit().as_str(),
            self.active_limit,
            0,
            ACTIVE_MAX,
        );
        let new_proxy_type = win.get_pref_proxy_type().to_string();
        let new_proxy_host = win.get_pref_proxy_host().to_string();
        let new_proxy_port: u16 = win
            .get_pref_proxy_port()
            .as_str()
            .parse()
            .unwrap_or(self.proxy_port);
        let new_proxy_peer = win.get_pref_proxy_peer_connections();
        let new_proxy_hostnames = win.get_pref_proxy_hostnames();
        self.ip_filter_enabled = win.get_pref_ip_filter_enabled();
        self.ip_filter_path = win.get_pref_ip_filter_path().to_string();
        self.ip_filter_auto_refresh = win.get_pref_ip_filter_auto_refresh();

        // Speed — v0.187.3 / Step 3: numeric KiB/s parser. Negative and
        // non-numeric strings become 0 ("Unlimited"); positive numbers are
        // treated as KiB/s and multiplied to bytes/sec for the engine.
        let new_dl_limit_enabled = win.get_pref_dl_limit_enabled();
        let new_dl_limit_value: u64 = parse_kib_int(win.get_pref_dl_limit_value().as_str());
        let new_ul_limit_enabled = win.get_pref_ul_limit_enabled();
        let new_ul_limit_value: u64 = parse_kib_int(win.get_pref_ul_limit_value().as_str());
        self.alt_dl_limit = parse_kib_int(win.get_pref_alt_dl_limit().as_str());
        self.alt_ul_limit = parse_kib_int(win.get_pref_alt_ul_limit().as_str());
        self.alt_speed_enabled = win.get_pref_alt_speed_enabled();
        self.rate_limit_overhead = win.get_pref_rate_limit_overhead();
        self.rate_limit_utp = win.get_pref_rate_limit_utp();
        self.rate_limit_lan = win.get_pref_rate_limit_lan();

        // Build engine prefs
        let mut ep = EnginePrefs::default();
        if new_listen_port != self.listen_port {
            ep.listen_port = Some(new_listen_port);
            self.listen_port = new_listen_port;
        }
        if new_randomize_port != self.randomize_port {
            ep.randomize_port_on_startup = Some(new_randomize_port);
            self.randomize_port = new_randomize_port;
        }
        if new_enable_upnp != self.enable_upnp {
            ep.enable_upnp = Some(new_enable_upnp);
            self.enable_upnp = new_enable_upnp;
        }
        if new_enable_natpmp != self.enable_natpmp {
            ep.enable_natpmp = Some(new_enable_natpmp);
            self.enable_natpmp = new_enable_natpmp;
        }
        if new_max_conn_global != self.max_connections_global {
            ep.max_connections_global = Some(new_max_conn_global);
            self.max_connections_global = new_max_conn_global;
        }
        if new_max_peers != self.max_peers_per_torrent {
            ep.max_peers_per_torrent = Some(usize::try_from(new_max_peers.max(0)).unwrap_or(0));
            self.max_peers_per_torrent = new_max_peers;
        }
        if new_max_ul_slots_global != self.max_upload_slots_global {
            ep.max_upload_slots_global = Some(new_max_ul_slots_global);
            self.max_upload_slots_global = new_max_ul_slots_global;
        }
        if new_max_ul_slots_per != self.max_upload_slots_per_torrent {
            ep.max_upload_slots_per_torrent = Some(new_max_ul_slots_per);
            self.max_upload_slots_per_torrent = new_max_ul_slots_per;
        }
        if new_active_dl != self.active_downloads {
            ep.active_downloads = Some(new_active_dl);
            self.active_downloads = new_active_dl;
        }
        if new_active_seeds != self.active_seeds {
            ep.active_seeds = Some(new_active_seeds);
            self.active_seeds = new_active_seeds;
        }
        if new_active_limit != self.active_limit {
            ep.active_limit = Some(new_active_limit);
            self.active_limit = new_active_limit;
        }
        if new_proxy_type != self.proxy_type
            || new_proxy_host != self.proxy_host
            || new_proxy_port != self.proxy_port
        {
            ep.proxy_type = Some(new_proxy_type.clone());
            ep.proxy_host = Some(new_proxy_host.clone());
            ep.proxy_port = Some(new_proxy_port);
            self.proxy_type = new_proxy_type;
            self.proxy_host = new_proxy_host;
            self.proxy_port = new_proxy_port;
        }
        if new_proxy_peer != self.proxy_peer_connections {
            ep.proxy_peer_connections = Some(new_proxy_peer);
            self.proxy_peer_connections = new_proxy_peer;
        }
        if new_proxy_hostnames != self.proxy_hostnames {
            ep.proxy_hostnames = Some(new_proxy_hostnames);
            self.proxy_hostnames = new_proxy_hostnames;
        }
        ep.ip_filter_enabled = Some(self.ip_filter_enabled);
        ep.ip_filter_path = Some(self.ip_filter_path.clone());
        ep.ip_filter_auto_refresh = Some(self.ip_filter_auto_refresh);

        // Speed — GUI-owned toggle logic: toggle-off sends 0 to engine
        let effective_dl = if new_dl_limit_enabled {
            new_dl_limit_value
        } else {
            0
        };
        let effective_ul = if new_ul_limit_enabled {
            new_ul_limit_value
        } else {
            0
        };
        self.dl_limit_enabled = new_dl_limit_enabled;
        self.dl_limit_value = new_dl_limit_value;
        self.ul_limit_enabled = new_ul_limit_enabled;
        self.ul_limit_value = new_ul_limit_value;
        ep.download_rate_limit = Some(effective_dl);
        ep.upload_rate_limit = Some(effective_ul);
        ep.dl_limit_enabled = Some(new_dl_limit_enabled);
        ep.ul_limit_enabled = Some(new_ul_limit_enabled);
        ep.alt_download_rate_limit = Some(self.alt_dl_limit);
        ep.alt_upload_rate_limit = Some(self.alt_ul_limit);
        ep.alt_speed_enabled = Some(self.alt_speed_enabled);
        ep.rate_limit_includes_overhead = Some(self.rate_limit_overhead);
        ep.rate_limit_utp = Some(self.rate_limit_utp);
        ep.rate_limit_lan = Some(self.rate_limit_lan);

        // BitTorrent
        let new_enable_dht = win.get_pref_enable_dht();
        let new_enable_pex = win.get_pref_enable_pex();
        let new_enable_lsd = win.get_pref_enable_lsd();
        let new_encryption = win.get_pref_encryption_mode().to_string();
        let new_anonymous = win.get_pref_anonymous_mode();
        let new_seed_ratio_enabled = win.get_pref_seed_ratio_enabled();
        let new_seed_ratio_value: f64 = win
            .get_pref_seed_ratio_value()
            .as_str()
            .parse()
            .unwrap_or(self.seed_ratio_value);
        let new_max_ratio_action = win.get_pref_max_ratio_action().to_string();
        let new_seed_time_enabled = win.get_pref_seed_time_enabled();
        let new_seed_time_value: u64 = win
            .get_pref_seed_time_value()
            .as_str()
            .parse()
            .unwrap_or(self.seed_time_value);
        let new_inactive_seed_time_enabled = win.get_pref_inactive_seed_time_enabled();
        let new_inactive_seed_time_value: u64 = win
            .get_pref_inactive_seed_time_value()
            .as_str()
            .parse()
            .unwrap_or(self.inactive_seed_time_value);
        let new_queueing = win.get_pref_queueing_enabled();

        if new_enable_dht != self.enable_dht {
            ep.enable_dht = Some(new_enable_dht);
            self.enable_dht = new_enable_dht;
        }
        if new_enable_pex != self.enable_pex {
            ep.enable_pex = Some(new_enable_pex);
            self.enable_pex = new_enable_pex;
        }
        if new_enable_lsd != self.enable_lsd {
            ep.enable_lsd = Some(new_enable_lsd);
            self.enable_lsd = new_enable_lsd;
        }
        if new_encryption != self.encryption_mode {
            ep.encryption_mode = Some(new_encryption.clone());
            self.encryption_mode = new_encryption;
        }
        if new_anonymous != self.anonymous_mode {
            ep.anonymous_mode = Some(new_anonymous);
            self.anonymous_mode = new_anonymous;
        }
        if new_queueing != self.queueing_enabled {
            ep.queueing_enabled = Some(new_queueing);
            self.queueing_enabled = new_queueing;
        }
        self.seed_ratio_enabled = new_seed_ratio_enabled;
        self.seed_ratio_value = new_seed_ratio_value;
        self.max_ratio_action.clone_from(&new_max_ratio_action);
        self.seed_time_enabled = new_seed_time_enabled;
        self.seed_time_value = new_seed_time_value;
        self.inactive_seed_time_enabled = new_inactive_seed_time_enabled;
        self.inactive_seed_time_value = new_inactive_seed_time_value;
        ep.seed_ratio_limit = Some(if new_seed_ratio_enabled {
            Some(new_seed_ratio_value)
        } else {
            None
        });
        ep.max_ratio_action = Some(new_max_ratio_action);
        ep.seed_time_limit_secs = Some(if new_seed_time_enabled {
            Some(new_seed_time_value * 60)
        } else {
            None
        });
        ep.inactive_seed_time_limit_secs = Some(if new_inactive_seed_time_enabled {
            Some(new_inactive_seed_time_value * 60)
        } else {
            None
        });

        // RSS (not-yet-active — just persist to state)
        self.rss_enabled = win.get_pref_rss_enabled();
        self.rss_refresh_interval = win
            .get_pref_rss_refresh_interval()
            .as_str()
            .parse()
            .unwrap_or(self.rss_refresh_interval);
        self.rss_max_articles = win
            .get_pref_rss_max_articles()
            .as_str()
            .parse()
            .unwrap_or(self.rss_max_articles);
        self.rss_auto_download = win.get_pref_rss_auto_download();
        self.rss_smart_filter = win.get_pref_rss_smart_filter();
        self.rss_download_repacks = win.get_pref_rss_download_repacks();

        // Web UI
        let new_webui_enabled = win.get_pref_webui_enabled();
        let new_webui_username = win.get_pref_webui_username().to_string();
        let new_webui_bypass = win.get_pref_webui_bypass_local_auth();
        let new_webui_ttl: u64 = win
            .get_pref_webui_session_ttl()
            .as_str()
            .parse()
            .unwrap_or(self.webui_session_ttl);
        let new_webui_max_auth: u32 = win
            .get_pref_webui_max_failed_auth()
            .as_str()
            .parse()
            .unwrap_or(self.webui_max_failed_auth);
        let new_webui_ban: u64 = win
            .get_pref_webui_ban_duration()
            .as_str()
            .parse()
            .unwrap_or(self.webui_ban_duration);
        let new_webui_csrf = win.get_pref_webui_csrf();
        let new_webui_host_val = win.get_pref_webui_host_validation();
        let new_webui_rproxy = win.get_pref_webui_reverse_proxy();

        if new_webui_enabled != self.webui_enabled {
            ep.qbt_compat_enabled = Some(new_webui_enabled);
            self.webui_enabled = new_webui_enabled;
        }
        if new_webui_username != self.webui_username {
            ep.qbt_compat_username = Some(new_webui_username.clone());
            self.webui_username = new_webui_username;
        }
        if new_webui_bypass != self.webui_bypass_local_auth {
            ep.qbt_compat_bypass_local_auth = Some(new_webui_bypass);
            self.webui_bypass_local_auth = new_webui_bypass;
        }
        if new_webui_ttl != self.webui_session_ttl {
            ep.qbt_compat_session_ttl = Some(new_webui_ttl);
            self.webui_session_ttl = new_webui_ttl;
        }
        if new_webui_max_auth != self.webui_max_failed_auth {
            ep.qbt_compat_max_failed_auth = Some(new_webui_max_auth);
            self.webui_max_failed_auth = new_webui_max_auth;
        }
        if new_webui_ban != self.webui_ban_duration {
            ep.qbt_compat_ban_duration = Some(new_webui_ban);
            self.webui_ban_duration = new_webui_ban;
        }
        if new_webui_csrf != self.webui_csrf {
            ep.qbt_compat_csrf = Some(new_webui_csrf);
            self.webui_csrf = new_webui_csrf;
        }
        if new_webui_host_val != self.webui_host_validation {
            ep.qbt_compat_host_validation = Some(new_webui_host_val);
            self.webui_host_validation = new_webui_host_val;
        }
        if new_webui_rproxy != self.webui_reverse_proxy {
            ep.qbt_compat_reverse_proxy = Some(new_webui_rproxy);
            self.webui_reverse_proxy = new_webui_rproxy;
        }
        self.webui_https = win.get_pref_webui_https();
        // v0.187.3 / 2A: diff webui_bind/port against the committed snapshot
        // and emit EnginePrefs entries so bridge.rs can both (a) write the
        // values back to settings.qbt_compat (the single source of truth)
        // and (b) drive the restart-required toast.
        let new_webui_bind = win.get_pref_webui_bind().to_string();
        let new_webui_port: u16 = win
            .get_pref_webui_port()
            .as_str()
            .parse()
            .unwrap_or(self.webui_port);
        if new_webui_bind != self.webui_bind {
            ep.qbt_compat_bind_address = Some(new_webui_bind.clone());
            self.webui_bind = new_webui_bind;
        }
        if new_webui_port != self.webui_port {
            ep.qbt_compat_port = Some(new_webui_port);
            self.webui_port = new_webui_port;
        }
        self.ddns_enabled = win.get_pref_ddns_enabled();
        self.ddns_service = win.get_pref_ddns_service().to_string();
        self.ddns_domain = win.get_pref_ddns_domain().to_string();

        // Advanced
        let new_hashing: usize = win
            .get_pref_hashing_threads()
            .as_str()
            .parse()
            .unwrap_or(self.hashing_threads);
        let new_resume_interval: u64 = win
            .get_pref_save_resume_interval()
            .as_str()
            .parse()
            .unwrap_or(self.save_resume_interval);
        let new_enable_utp = win.get_pref_enable_utp();
        let new_enable_fast = win.get_pref_enable_fast_ext();
        let new_enable_hp = win.get_pref_enable_holepunch();
        let new_enable_bep40 = win.get_pref_enable_bep40();

        if new_hashing != self.hashing_threads {
            ep.hashing_threads = Some(new_hashing);
            self.hashing_threads = new_hashing;
        }
        if new_resume_interval != self.save_resume_interval {
            ep.save_resume_interval_secs = Some(new_resume_interval * 60);
            self.save_resume_interval = new_resume_interval;
        }
        if new_enable_utp != self.enable_utp {
            ep.enable_utp = Some(new_enable_utp);
            self.enable_utp = new_enable_utp;
        }
        if new_enable_fast != self.enable_fast_extension {
            ep.enable_fast_extension = Some(new_enable_fast);
            self.enable_fast_extension = new_enable_fast;
        }
        if new_enable_hp != self.enable_holepunch {
            ep.enable_holepunch = Some(new_enable_hp);
            self.enable_holepunch = new_enable_hp;
        }
        if new_enable_bep40 != self.enable_bep40 {
            ep.enable_bep40_eviction = Some(new_enable_bep40);
            self.enable_bep40 = new_enable_bep40;
        }
        self.disk_cache_size = win
            .get_pref_disk_cache_size()
            .as_str()
            .parse()
            .unwrap_or(self.disk_cache_size);

        result.engine_prefs = Some(Box::new(ep));

        result.gui_config_dirty = true;
        win.set_pref_dirty(false);
        result
    }

    /// Write current state into a `GuiConfig` for persistence.
    pub fn populate_gui_config(&self, gui: &mut irontide_config::GuiConfig) {
        gui.confirm_delete = Some(self.confirm_delete);
        gui.confirm_pause_all = Some(self.confirm_pause_all);
        gui.show_torrent_added_toast = Some(self.show_torrent_added_toast);
        gui.double_click_action = Some(self.double_click_action.clone());
        gui.start_minimized = Some(self.start_minimized);
        gui.minimize_to_tray = Some(self.minimize_to_tray);
        gui.resume_previous_session = Some(self.resume_previous_session);
        gui.notify_on_complete = Some(self.notify_on_complete);
        gui.notify_on_error = Some(self.notify_on_error);
        gui.notify_on_rss = Some(self.notify_on_rss);
        gui.play_sound_on_complete = Some(self.play_sound_on_complete);
        gui.on_complete_program = Some(self.on_complete_program.clone());
        gui.create_subfolder = Some(self.create_subfolder);
        gui.pre_allocate = Some(self.pre_allocate);
        gui.show_add_torrent_dialog = Some(self.show_add_torrent_dialog);
        gui.skip_hash_check = Some(self.skip_hash_check);
        gui.use_incomplete_dir = Some(self.use_incomplete_dir);
        gui.incomplete_dir = Some(self.incomplete_dir.clone());
        gui.incomplete_extension = Some(self.incomplete_extension);
        gui.use_auto_categories = Some(self.use_auto_categories);
        gui.append_date_to_path = Some(self.append_date_to_path);
        gui.watched_folder = Some(self.watched_folder.clone());
        gui.copy_torrent_to = Some(self.copy_torrent_to.clone());
        gui.delete_torrent_after_add = Some(self.delete_torrent_after_add);
        gui.move_completed_enabled = Some(self.move_completed_enabled);
        gui.move_completed_to = Some(self.move_completed_to.clone());
        gui.dl_on_complete_program = Some(self.dl_on_complete_program.clone());
        // Connection + Speed (GUI-owned persistence)
        gui.dl_limit_enabled = Some(self.dl_limit_enabled);
        gui.dl_limit_value = Some(self.dl_limit_value);
        gui.ul_limit_enabled = Some(self.ul_limit_enabled);
        gui.ul_limit_value = Some(self.ul_limit_value);
        gui.alt_dl_limit = Some(self.alt_dl_limit);
        gui.alt_ul_limit = Some(self.alt_ul_limit);
        gui.alt_speed_enabled = Some(self.alt_speed_enabled);
        gui.ip_filter_enabled = Some(self.ip_filter_enabled);
        gui.ip_filter_path = Some(self.ip_filter_path.clone());
        gui.ip_filter_auto_refresh = Some(self.ip_filter_auto_refresh);
        // RSS (not-yet-active, persisted to GuiConfig)
        gui.rss_enabled = Some(self.rss_enabled);
        gui.rss_refresh_interval_min = Some(self.rss_refresh_interval);
        gui.rss_max_articles = Some(self.rss_max_articles);
        gui.rss_auto_download = Some(self.rss_auto_download);
        gui.rss_smart_filter = Some(self.rss_smart_filter);
        gui.rss_download_repacks = Some(self.rss_download_repacks);
        // Web UI not-yet-active
        gui.webui_https = Some(self.webui_https);
        gui.ddns_enabled = Some(self.ddns_enabled);
        gui.ddns_service = Some(self.ddns_service.clone());
        gui.ddns_domain = Some(self.ddns_domain.clone());
        // Advanced not-yet-active
        gui.disk_cache_size = Some(self.disk_cache_size);
    }
}

/// Result of a preferences Apply operation.
#[derive(Debug, Default)]
pub struct ApplyResult {
    pub skin_changed: bool,
    pub download_dir: Option<String>,
    pub create_subfolder: Option<bool>,
    pub gui_config_dirty: bool,
    pub engine_prefs: Option<Box<crate::app::EnginePrefs>>,
}

impl ApplyResult {
    #[allow(
        dead_code,
        reason = "M185: used in tests, future milestones may re-use in main.rs"
    )]
    #[must_use]
    pub fn has_engine_changes(&self) -> bool {
        self.download_dir.is_some()
            || self.create_subfolder.is_some()
            || self.engine_prefs.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_sensible_values() {
        let state = PreferencesState::default();
        assert_eq!(state.skin, Skin::Tide);
        assert_eq!(state.theme, Theme::Dark);
        assert_eq!(state.density, Density::Balanced);
        assert_eq!(state.radius, RadiusPreset::Balanced);
        assert!(state.confirm_delete);
        assert!(state.create_subfolder);
        assert!(state.resume_previous_session);
        assert!(state.notify_on_complete);
        assert_eq!(state.download_dir, "");
    }

    #[test]
    fn from_app_uses_skin_settings() {
        let skin = skin::SkinSettings {
            skin: Skin::Forge,
            theme: Theme::Light,
            density: Density::Compact,
            radius: RadiusPreset::Sharp,
        };
        let gui = irontide_config::GuiConfig::default();
        let state = PreferencesState::from_app(
            skin,
            &gui,
            "/tmp/dl",
            &irontide::session::Settings::default(),
        );
        assert_eq!(state.skin, Skin::Forge);
        assert_eq!(state.theme, Theme::Light);
        assert_eq!(state.density, Density::Compact);
        assert_eq!(state.radius, RadiusPreset::Sharp);
        assert_eq!(state.download_dir, "/tmp/dl");
    }

    #[test]
    fn from_app_reads_gui_config_fields() {
        let skin = skin::SkinSettings::default();
        let gui = irontide_config::GuiConfig {
            confirm_delete: Some(false),
            create_subfolder: Some(false),
            notify_on_complete: Some(false),
            ..Default::default()
        };
        let state =
            PreferencesState::from_app(skin, &gui, "", &irontide::session::Settings::default());
        assert!(!state.confirm_delete);
        assert!(!state.create_subfolder);
        assert!(!state.notify_on_complete);
    }

    #[test]
    fn populate_gui_config_round_trips() {
        let state = PreferencesState {
            confirm_delete: false,
            create_subfolder: false,
            on_complete_program: "/usr/bin/notify".to_owned(),
            ..Default::default()
        };

        let mut gui = irontide_config::GuiConfig::default();
        state.populate_gui_config(&mut gui);

        assert_eq!(gui.confirm_delete, Some(false));
        assert_eq!(gui.create_subfolder, Some(false));
        assert_eq!(gui.on_complete_program.as_deref(), Some("/usr/bin/notify"));

        // Round-trip: from_app should recover the same values.
        let recovered = PreferencesState::from_app(
            skin::SkinSettings::default(),
            &gui,
            "",
            &irontide::session::Settings::default(),
        );
        assert!(!recovered.confirm_delete);
        assert!(!recovered.create_subfolder);
        assert_eq!(recovered.on_complete_program, "/usr/bin/notify");
    }

    #[test]
    fn apply_result_has_engine_changes_when_download_dir_set() {
        let result = ApplyResult {
            download_dir: Some("/new/path".to_owned()),
            ..Default::default()
        };
        assert!(result.has_engine_changes());
    }

    #[test]
    fn apply_result_no_engine_changes_when_empty() {
        let result = ApplyResult::default();
        assert!(!result.has_engine_changes());
    }

    // ── M187: BitTorrent / RSS / Web UI / Advanced ──────────────────

    #[test]
    fn default_has_sensible_bittorrent_values() {
        let state = PreferencesState::default();
        assert!(state.enable_dht);
        assert!(state.enable_pex);
        assert!(state.enable_lsd);
        assert_eq!(state.encryption_mode, "Disable encryption");
        assert!(!state.anonymous_mode);
        assert!(!state.seed_ratio_enabled);
        assert!((state.seed_ratio_value - 2.0).abs() < f64::EPSILON);
        assert_eq!(state.max_ratio_action, "Pause torrent");
        assert!(!state.queueing_enabled);
        assert!(!state.rss_enabled);
        assert!(state.enable_utp);
        assert!(state.enable_fast_extension);
        assert!(state.enable_holepunch);
        assert!(state.enable_bep40);
    }

    #[test]
    fn from_app_reads_bittorrent_fields() {
        let skin = skin::SkinSettings::default();
        let gui = irontide_config::GuiConfig::default();
        let settings = irontide::session::Settings {
            enable_dht: false,
            enable_pex: false,
            encryption_mode: irontide::wire::mse::EncryptionMode::Forced,
            anonymous_mode: true,
            seed_ratio_limit: Some(1.5),
            max_ratio_action: irontide::session::MaxRatioAction::Remove,
            seed_time_limit_secs: Some(7200),
            ..Default::default()
        };
        let state = PreferencesState::from_app(skin, &gui, "", &settings);
        assert!(!state.enable_dht);
        assert!(!state.enable_pex);
        assert_eq!(state.encryption_mode, "Require encryption");
        assert!(state.anonymous_mode);
        assert!(state.seed_ratio_enabled);
        assert!((state.seed_ratio_value - 1.5).abs() < f64::EPSILON);
        assert_eq!(state.max_ratio_action, "Remove torrent");
        assert!(state.seed_time_enabled);
        assert_eq!(state.seed_time_value, 120);
    }

    #[test]
    fn from_app_reads_qbt_compat_fields() {
        let skin = skin::SkinSettings::default();
        let gui = irontide_config::GuiConfig::default();
        let settings = irontide::session::Settings {
            qbt_compat: irontide::session::QbtCompatSettings {
                enabled: false,
                username: "testuser".to_owned(),
                bypass_local_auth: true,
                session_ttl_secs: 3600,
                csrf_protection_enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let state = PreferencesState::from_app(skin, &gui, "", &settings);
        assert!(!state.webui_enabled);
        assert_eq!(state.webui_username, "testuser");
        assert!(state.webui_bypass_local_auth);
        assert_eq!(state.webui_session_ttl, 3600);
        assert!(!state.webui_csrf);
    }

    #[test]
    fn populate_gui_config_round_trips_rss() {
        let state = PreferencesState {
            rss_enabled: true,
            rss_refresh_interval: 30,
            rss_max_articles: 100,
            rss_auto_download: true,
            rss_smart_filter: true,
            rss_download_repacks: true,
            ..Default::default()
        };
        let mut gui = irontide_config::GuiConfig::default();
        state.populate_gui_config(&mut gui);
        assert_eq!(gui.rss_enabled, Some(true));
        assert_eq!(gui.rss_refresh_interval_min, Some(30));
        assert_eq!(gui.rss_max_articles, Some(100));
        assert_eq!(gui.rss_auto_download, Some(true));
        assert_eq!(gui.rss_smart_filter, Some(true));
        assert_eq!(gui.rss_download_repacks, Some(true));
        let recovered = PreferencesState::from_app(
            skin::SkinSettings::default(),
            &gui,
            "",
            &irontide::session::Settings::default(),
        );
        assert!(recovered.rss_enabled);
        assert_eq!(recovered.rss_refresh_interval, 30);
        assert_eq!(recovered.rss_max_articles, 100);
    }

    #[test]
    fn populate_gui_config_round_trips_webui() {
        let state = PreferencesState {
            webui_https: true,
            ddns_enabled: true,
            ddns_service: "duckdns.org".to_owned(),
            ddns_domain: "test.duckdns.org".to_owned(),
            ..Default::default()
        };
        let mut gui = irontide_config::GuiConfig::default();
        state.populate_gui_config(&mut gui);
        assert_eq!(gui.webui_https, Some(true));
        assert_eq!(gui.ddns_enabled, Some(true));
        assert_eq!(gui.ddns_service.as_deref(), Some("duckdns.org"));
        assert_eq!(gui.ddns_domain.as_deref(), Some("test.duckdns.org"));
    }

    #[test]
    fn populate_gui_config_round_trips_advanced() {
        let state = PreferencesState {
            disk_cache_size: 512,
            ..Default::default()
        };
        let mut gui = irontide_config::GuiConfig::default();
        state.populate_gui_config(&mut gui);
        assert_eq!(gui.disk_cache_size, Some(512));
    }

    #[test]
    fn encryption_mode_label_round_trip() {
        use irontide::wire::mse::EncryptionMode;
        let cases = [
            (EncryptionMode::Disabled, "Disable encryption"),
            (EncryptionMode::Enabled, "Prefer encryption"),
            (EncryptionMode::Forced, "Require encryption"),
        ];
        for (mode, expected_label) in &cases {
            let label = encryption_mode_to_label(*mode);
            assert_eq!(&label, expected_label);
            let recovered = label_to_encryption_mode(&label);
            assert_eq!(
                std::mem::discriminant(&recovered),
                std::mem::discriminant(mode)
            );
        }
    }

    // ── v0.187.4: reset_tab per-tab tests ─────────────────────────

    fn mutated_state() -> PreferencesState {
        PreferencesState {
            skin: Skin::Forge,
            theme: Theme::Light,
            density: Density::Compact,
            radius: RadiusPreset::Sharp,
            confirm_delete: false,
            resume_previous_session: false,
            download_dir: "/custom".to_owned(),
            create_subfolder: false,
            pre_allocate: true,
            listen_port: 9999,
            enable_upnp: false,
            max_connections_global: 1,
            dl_limit_enabled: true,
            dl_limit_value: 500,
            rate_limit_utp: false,
            enable_dht: false,
            enable_pex: false,
            encryption_mode: "Require encryption".to_owned(),
            seed_ratio_enabled: true,
            rss_enabled: true,
            rss_refresh_interval: 5,
            rss_max_articles: 999,
            webui_enabled: true,
            webui_port: 1234,
            webui_csrf: false,
            ddns_enabled: true,
            hashing_threads: 16,
            disk_cache_size: 2048,
            enable_utp: false,
            enable_bep40: false,
            ..Default::default()
        }
    }

    #[test]
    fn reset_tab_behavior() {
        let d = PreferencesState::default();
        let mut s = mutated_state();
        assert!(s.reset_tab("behavior"));
        assert_eq!(s.skin, d.skin);
        assert_eq!(s.theme, d.theme);
        assert_eq!(s.density, d.density);
        assert_eq!(s.radius, d.radius);
        assert_eq!(s.confirm_delete, d.confirm_delete);
        assert_eq!(s.resume_previous_session, d.resume_previous_session);
        // Non-behavior fields untouched
        assert_eq!(s.download_dir, "/custom");
        assert!(!s.enable_dht);
    }

    #[test]
    fn reset_tab_downloads() {
        let d = PreferencesState::default();
        let mut s = mutated_state();
        assert!(s.reset_tab("downloads"));
        assert_eq!(s.download_dir, d.download_dir);
        assert_eq!(s.create_subfolder, d.create_subfolder);
        assert_eq!(s.pre_allocate, d.pre_allocate);
        // Non-downloads fields untouched
        assert_eq!(s.skin, Skin::Forge);
    }

    #[test]
    fn reset_tab_connection() {
        let d = PreferencesState::default();
        let mut s = mutated_state();
        assert!(s.reset_tab("connection"));
        assert_eq!(s.listen_port, d.listen_port);
        assert_eq!(s.enable_upnp, d.enable_upnp);
        assert_eq!(s.max_connections_global, d.max_connections_global);
        // Non-connection fields untouched
        assert!(s.dl_limit_enabled);
    }

    #[test]
    fn reset_tab_speed() {
        let d = PreferencesState::default();
        let mut s = mutated_state();
        assert!(s.reset_tab("speed"));
        assert_eq!(s.dl_limit_enabled, d.dl_limit_enabled);
        assert_eq!(s.dl_limit_value, d.dl_limit_value);
        assert_eq!(s.rate_limit_utp, d.rate_limit_utp);
        // Non-speed fields untouched
        assert_eq!(s.listen_port, 9999);
    }

    #[test]
    fn reset_tab_bittorrent() {
        let d = PreferencesState::default();
        let mut s = mutated_state();
        assert!(s.reset_tab("bittorrent"));
        assert_eq!(s.enable_dht, d.enable_dht);
        assert_eq!(s.enable_pex, d.enable_pex);
        assert_eq!(s.encryption_mode, d.encryption_mode);
        assert_eq!(s.seed_ratio_enabled, d.seed_ratio_enabled);
        // Non-bittorrent fields untouched
        assert!(s.rss_enabled);
    }

    #[test]
    fn reset_tab_rss() {
        let d = PreferencesState::default();
        let mut s = mutated_state();
        assert!(s.reset_tab("rss"));
        assert_eq!(s.rss_enabled, d.rss_enabled);
        assert_eq!(s.rss_refresh_interval, d.rss_refresh_interval);
        assert_eq!(s.rss_max_articles, d.rss_max_articles);
        // Non-rss fields untouched
        assert!(s.webui_enabled);
    }

    #[test]
    fn reset_tab_webui() {
        let d = PreferencesState::default();
        let mut s = mutated_state();
        assert!(s.reset_tab("webui"));
        assert_eq!(s.webui_enabled, d.webui_enabled);
        assert_eq!(s.webui_port, d.webui_port);
        assert_eq!(s.webui_csrf, d.webui_csrf);
        assert_eq!(s.ddns_enabled, d.ddns_enabled);
        // Non-webui fields untouched
        assert_eq!(s.hashing_threads, 16);
    }

    #[test]
    fn reset_tab_advanced() {
        let d = PreferencesState::default();
        let mut s = mutated_state();
        assert!(s.reset_tab("advanced"));
        assert_eq!(s.hashing_threads, d.hashing_threads);
        assert_eq!(s.disk_cache_size, d.disk_cache_size);
        assert_eq!(s.enable_utp, d.enable_utp);
        assert_eq!(s.enable_bep40, d.enable_bep40);
        // Non-advanced fields untouched
        assert!(s.webui_enabled);
    }

    #[test]
    fn reset_tab_unknown_returns_false() {
        let mut s = mutated_state();
        let before_skin = s.skin;
        assert!(!s.reset_tab("nonexistent"));
        assert_eq!(s.skin, before_skin);
    }

    #[test]
    fn max_ratio_action_label_round_trip() {
        use irontide::session::MaxRatioAction;
        let cases = [
            (MaxRatioAction::Pause, "Pause torrent"),
            (MaxRatioAction::Remove, "Remove torrent"),
            (MaxRatioAction::EnableSuperSeeding, "Super-seeding mode"),
        ];
        for (action, expected_label) in &cases {
            let label = max_ratio_action_to_label(*action);
            assert_eq!(&label, expected_label);
            let recovered = label_to_max_ratio_action(&label);
            assert_eq!(
                std::mem::discriminant(&recovered),
                std::mem::discriminant(action)
            );
        }
    }
}
