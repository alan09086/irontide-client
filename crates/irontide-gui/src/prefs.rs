//! Preferences view model (M184).
//!
//! `PreferencesState` is the **committed-state** snapshot. The Slint dialog
//! holds pending edits; on Apply/OK the Rust side reads the Slint properties,
//! diffs against the committed state, applies the changes, and updates the
//! committed snapshot.

use std::str::FromStr;

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
        }
    }
}

impl PreferencesState {
    /// Aggregate from the three configuration sources.
    #[must_use]
    pub fn from_app(
        skin_settings: skin::SkinSettings,
        gui: &irontide_config::GuiConfig,
        download_dir: &str,
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
        let new_layout =
            Layout::from_label(win.get_pref_layout().as_str()).unwrap_or(self.layout);

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
}

impl ApplyResult {
    #[must_use]
    pub fn has_engine_changes(&self) -> bool {
        self.download_dir.is_some() || self.create_subfolder.is_some()
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
        let state = PreferencesState::from_app(skin, &gui, "/tmp/dl");
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
        let state = PreferencesState::from_app(skin, &gui, "");
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
        let recovered = PreferencesState::from_app(skin::SkinSettings::default(), &gui, "");
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
