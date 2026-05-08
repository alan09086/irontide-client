//! Preferences view model (M184).
//!
//! `PreferencesState` is the **committed-state** snapshot. The Slint dialog
//! holds pending edits; on Apply/OK the Rust side reads the Slint properties,
//! diffs against the committed state, applies the changes, and updates the
//! committed snapshot.

use std::str::FromStr;

use crate::app::EnginePrefs;
use crate::skin::{self, Density, Layout, RadiusPreset, Skin, Theme};

/// Committed preferences state. Aggregated from `SkinSettings` + `GuiConfig`.
#[derive(Debug, Clone)]
pub struct PreferencesState {
    // Interface
    pub skin: Skin,
    pub theme: Theme,
    pub density: Density,
    pub radius: RadiusPreset,
    pub layout: Layout,
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
}

impl Default for PreferencesState {
    fn default() -> Self {
        Self {
            skin: Skin::default(),
            theme: Theme::default(),
            density: Density::default(),
            radius: RadiusPreset::default(),
            layout: Layout::default(),
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
        }
    }
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
            layout: skin_settings.layout,
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
        }
    }

    /// Push committed state into Slint dialog properties.
    pub fn populate_slint(&self, win: &crate::MainWindow) {
        win.set_pref_skin(self.skin.to_string().into());
        win.set_pref_theme(self.theme.to_string().into());
        win.set_pref_density(self.density.to_string().into());
        win.set_pref_radius(self.radius.to_string().into());
        win.set_pref_layout(self.layout.label().into());
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
        // Speed
        win.set_pref_dl_limit_enabled(self.dl_limit_enabled);
        win.set_pref_dl_limit_value(self.dl_limit_value.to_string().into());
        win.set_pref_ul_limit_enabled(self.ul_limit_enabled);
        win.set_pref_ul_limit_value(self.ul_limit_value.to_string().into());
        win.set_pref_alt_dl_limit(self.alt_dl_limit.to_string().into());
        win.set_pref_alt_ul_limit(self.alt_ul_limit.to_string().into());
        win.set_pref_alt_speed_enabled(self.alt_speed_enabled);
        win.set_pref_rate_limit_overhead(self.rate_limit_overhead);
        win.set_pref_rate_limit_utp(self.rate_limit_utp);
        win.set_pref_rate_limit_lan(self.rate_limit_lan);
        win.set_pref_dirty(false);
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
        let new_layout = Layout::from_label(win.get_pref_layout().as_str()).unwrap_or(self.layout);

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
        if new_layout != self.layout {
            self.layout = new_layout;
            result.layout_changed = true;
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
        let new_max_conn_global: i32 = win
            .get_pref_max_connections_global()
            .as_str()
            .parse()
            .unwrap_or(self.max_connections_global);
        let new_max_peers: i32 = win
            .get_pref_max_peers_per_torrent()
            .as_str()
            .parse()
            .unwrap_or(self.max_peers_per_torrent);
        let new_max_ul_slots_global: i32 = win
            .get_pref_max_upload_slots_global()
            .as_str()
            .parse()
            .unwrap_or(self.max_upload_slots_global);
        let new_max_ul_slots_per: i32 = win
            .get_pref_max_upload_slots_per_torrent()
            .as_str()
            .parse()
            .unwrap_or(self.max_upload_slots_per_torrent);
        let new_active_dl: i32 = win
            .get_pref_active_downloads()
            .as_str()
            .parse()
            .unwrap_or(self.active_downloads);
        let new_active_seeds: i32 = win
            .get_pref_active_seeds()
            .as_str()
            .parse()
            .unwrap_or(self.active_seeds);
        let new_active_limit: i32 = win
            .get_pref_active_limit()
            .as_str()
            .parse()
            .unwrap_or(self.active_limit);
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

        // Speed
        let new_dl_limit_enabled = win.get_pref_dl_limit_enabled();
        let new_dl_limit_value: u64 = win
            .get_pref_dl_limit_value()
            .as_str()
            .parse()
            .unwrap_or(self.dl_limit_value);
        let new_ul_limit_enabled = win.get_pref_ul_limit_enabled();
        let new_ul_limit_value: u64 = win
            .get_pref_ul_limit_value()
            .as_str()
            .parse()
            .unwrap_or(self.ul_limit_value);
        self.alt_dl_limit = win
            .get_pref_alt_dl_limit()
            .as_str()
            .parse()
            .unwrap_or(self.alt_dl_limit);
        self.alt_ul_limit = win
            .get_pref_alt_ul_limit()
            .as_str()
            .parse()
            .unwrap_or(self.alt_ul_limit);
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
    }
}

/// Result of a preferences Apply operation.
#[derive(Debug, Default)]
pub struct ApplyResult {
    pub skin_changed: bool,
    pub layout_changed: bool,
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
        assert_eq!(state.layout, Layout::L1);
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
            layout: Layout::L2,
            ..skin::SkinSettings::default()
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
        assert_eq!(state.layout, Layout::L2);
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
}
