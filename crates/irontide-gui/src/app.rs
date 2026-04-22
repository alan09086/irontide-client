use std::collections::HashSet;

/// Application lifecycle phases.
#[derive(Debug, Clone, PartialEq)]
pub enum AppPhase {
    Loading,
    Ready,
    Error(String),
}

/// Menu actions from the File menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuAction {
    AddMagnet,
    AddTorrentFile,
    Quit,
}

/// Commands sent from the Slint UI thread to the async session thread.
///
/// The GUI callbacks are synchronous (main thread), but `SessionHandle` methods
/// are async (tokio background thread). `GuiCommand` bridges that gap via an
/// unbounded mpsc channel.
#[derive(Debug)]
pub enum GuiCommand {
    /// Add a torrent from a magnet URI.
    AddMagnet {
        /// The magnet URI string.
        uri: String,
        /// Optional override for the download directory.
        download_dir: Option<String>,
    },
    /// Add a torrent from a `.torrent` file path.
    AddTorrentFile {
        /// Filesystem path to the `.torrent` file.
        path: String,
        /// Optional override for the download directory.
        download_dir: Option<String>,
    },
    /// Pause one or more torrents by info-hash hex.
    PauseTorrents {
        /// Hex-encoded info-hash strings.
        hashes: Vec<String>,
    },
    /// Resume one or more torrents by info-hash hex.
    ResumeTorrents {
        /// Hex-encoded info-hash strings.
        hashes: Vec<String>,
    },
    /// Remove one or more torrents by info-hash hex.
    RemoveTorrents {
        /// Hex-encoded info-hash strings.
        hashes: Vec<String>,
        /// Whether to also delete downloaded files.
        delete_files: bool,
    },
    /// Enable or disable seed-only mode for one or more torrents.
    SetSeedMode {
        /// Hex-encoded info-hash strings.
        hashes: Vec<String>,
        /// `true` to enter seed mode, `false` to resume downloading.
        enabled: bool,
    },
    /// Force a piece recheck for one or more torrents.
    ForceRecheck {
        /// Hex-encoded info-hash strings.
        hashes: Vec<String>,
    },
    /// Force all trackers to re-announce for one or more torrents.
    ForceReannounce {
        /// Hex-encoded info-hash strings.
        hashes: Vec<String>,
    },
    /// Update the default download directory (persists to config + session).
    SetDefaultDownloadDir {
        /// New download directory path.
        dir: String,
    },
}

/// Context-menu actions for selected torrents.
///
/// Mirrors the `MenuAction` pattern with `from_index` for Slint callback
/// integration. Indices must remain stable across releases.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContextAction {
    /// Pause the selected torrent(s).
    Pause,
    /// Resume the selected torrent(s).
    Resume,
    /// Switch to seed-only mode.
    SeedOnly,
    /// Resume downloading (exit seed-only mode).
    ResumeDownload,
    /// Remove torrent(s) but keep files.
    Remove,
    /// Remove torrent(s) and delete downloaded files.
    RemoveAndDelete,
    /// Force a full piece recheck.
    Recheck,
    /// Force all trackers to re-announce.
    ForceReannounce,
}

impl ContextAction {
    /// Parse a context-menu callback index into a `ContextAction`.
    /// Returns `None` for out-of-bounds indices.
    pub fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Pause),
            1 => Some(Self::Resume),
            2 => Some(Self::SeedOnly),
            3 => Some(Self::ResumeDownload),
            4 => Some(Self::Remove),
            5 => Some(Self::RemoveAndDelete),
            6 => Some(Self::Recheck),
            7 => Some(Self::ForceReannounce),
            _ => None,
        }
    }
}

impl MenuAction {
    /// Parse a menu callback index into a `MenuAction`.
    /// Returns `None` for out-of-bounds indices.
    pub fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::AddMagnet),
            1 => Some(Self::AddTorrentFile),
            2 => Some(Self::Quit),
            _ => None,
        }
    }
}

/// Top-level application state.
pub struct AppState {
    /// Current lifecycle phase.
    pub phase: AppPhase,
    /// One-shot channel to signal session shutdown.
    pub shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    /// Sender for dispatching async commands to the session thread.
    ///
    /// Starts as `None` and is populated by `bridge::run_session` once the
    /// session is ready.
    pub cmd_tx: Option<tokio::sync::mpsc::UnboundedSender<GuiCommand>>,
    /// Current sort column and direction.
    pub sort: crate::columns::SortState,
    /// Set of currently selected torrent info-hash hex strings.
    pub selected: HashSet<String>,
    /// The last-clicked info-hash (shift-click anchor).
    pub last_clicked: Option<String>,
    /// Current display order of info-hash hex strings.
    pub current_order: Vec<String>,
    /// Column visibility and order configuration.
    pub columns: crate::columns::ColumnConfig,
    /// Whether the column config has unsaved changes.
    pub columns_dirty: bool,
    /// Active skin/theme/density/radius settings.
    ///
    /// Stored in Lane A; Lane B wires `SkinSettings::apply` into the Slint
    /// `Tokens` global and the `skin-applied` gate, at which point this
    /// field is read on every settings change.
    #[allow(dead_code)] // Read by Lane B (skin apply + settings-tab reads).
    pub skin: crate::skin::SkinSettings,
    /// Whether the skin config has unsaved changes.
    ///
    /// Lane B sets this from the settings-tab callbacks and inspects it on
    /// shutdown to persist a `GuiConfig` update via `save_gui_config`.
    #[allow(dead_code)] // Read by Lane B (save-on-shutdown gate).
    pub skin_dirty: bool,
}

impl AppState {
    /// Create a new `AppState` in the `Loading` phase.
    pub fn new(
        shutdown_tx: tokio::sync::oneshot::Sender<()>,
        columns: crate::columns::ColumnConfig,
        skin: crate::skin::SkinSettings,
    ) -> Self {
        Self {
            phase: AppPhase::Loading,
            shutdown_tx: Some(shutdown_tx),
            cmd_tx: None,
            sort: crate::columns::SortState::default(),
            selected: HashSet::new(),
            last_clicked: None,
            current_order: Vec::new(),
            columns,
            columns_dirty: false,
            skin,
            skin_dirty: false,
        }
    }

    /// Select all torrents from the provided info-hash list.
    pub fn select_all(&mut self, all_hashes: &[String]) {
        self.selected.clear();
        for h in all_hashes {
            self.selected.insert(h.clone());
        }
    }

    /// Single-click: clear all selection, select only this hash.
    pub fn selection_click(&mut self, info_hash: &str) {
        self.selected.clear();
        self.selected.insert(info_hash.to_owned());
        self.last_clicked = Some(info_hash.to_owned());
    }

    /// Ctrl+click: toggle selection of this hash without clearing others.
    pub fn selection_ctrl_click(&mut self, info_hash: &str) {
        if self.selected.contains(info_hash) {
            self.selected.remove(info_hash);
        } else {
            self.selected.insert(info_hash.to_owned());
        }
        self.last_clicked = Some(info_hash.to_owned());
    }

    /// Shift+click: select range from last_clicked to this hash.
    /// Uses `current_order` to determine the range.
    pub fn selection_shift_click(&mut self, info_hash: &str) {
        let Some(anchor) = self.last_clicked.as_ref() else {
            // No anchor — treat as single click.
            self.selection_click(info_hash);
            return;
        };
        let anchor_pos = self.current_order.iter().position(|h| h == anchor);
        let target_pos = self.current_order.iter().position(|h| h == info_hash);
        match (anchor_pos, target_pos) {
            (Some(a), Some(t)) => {
                let (start, end) = if a <= t { (a, t) } else { (t, a) };
                self.selected.clear();
                for h in &self.current_order[start..=end] {
                    self.selected.insert(h.clone());
                }
            }
            _ => {
                // Can't find either in order — treat as single click.
                self.selection_click(info_hash);
            }
        }
        // Don't update last_clicked on shift-click (anchor stays)
    }
}

/// Smart enable/disable state for the context menu.
///
/// Computed from the display-state strings of the selected torrents.
/// Rules:
/// - Pause: enabled when at least one selected torrent is *not* paused.
/// - Resume: enabled when at least one selected torrent *is* paused.
/// - Seed Only: enabled when at least one is not in seed mode AND none are
///   fetching metadata or checking.
/// - Resume Download: enabled when at least one is in seed mode.
/// - Recheck: enabled when none are fetching metadata or checking.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenuState {
    /// Whether the "Pause" action should be enabled.
    pub can_pause: bool,
    /// Whether the "Resume" action should be enabled.
    pub can_resume: bool,
    /// Whether the "Seed Only" action should be enabled.
    pub can_seed_only: bool,
    /// Whether the "Resume Download" action should be enabled.
    pub can_resume_download: bool,
    /// Whether the "Recheck" action should be enabled.
    pub can_recheck: bool,
}

impl ContextMenuState {
    /// Compute the smart enable/disable state from a slice of display-state
    /// strings (as produced by `format::format_state`).
    ///
    /// An empty slice disables all actions.
    pub fn compute(states: &[&str]) -> Self {
        if states.is_empty() {
            return Self {
                can_pause: false,
                can_resume: false,
                can_seed_only: false,
                can_resume_download: false,
                can_recheck: false,
            };
        }

        let mut any_paused = false;
        let mut any_not_paused = false;
        let mut any_seed_mode = false;
        let mut any_not_seed_mode = false;
        let mut any_fetching_or_checking = false;

        for &state_str in states {
            if state_str == "paused" {
                any_paused = true;
            } else {
                any_not_paused = true;
            }
            if state_str == "seed only" {
                any_seed_mode = true;
            } else {
                any_not_seed_mode = true;
            }
            if state_str == "fetching metadata" || state_str == "checking" {
                any_fetching_or_checking = true;
            }
        }

        Self {
            can_pause: any_not_paused,
            can_resume: any_paused,
            can_seed_only: any_not_seed_mode && !any_fetching_or_checking,
            can_resume_download: any_seed_mode,
            can_recheck: !any_fetching_or_checking,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_phase_default_is_loading() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        assert_eq!(state.phase, AppPhase::Loading);
    }

    #[test]
    fn app_phase_transitions() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        assert_eq!(state.phase, AppPhase::Loading);

        state.phase = AppPhase::Ready;
        assert_eq!(state.phase, AppPhase::Ready);

        state.phase = AppPhase::Error("test error".to_string());
        assert_eq!(state.phase, AppPhase::Error("test error".to_string()));
    }

    #[test]
    fn menu_action_from_index() {
        assert_eq!(MenuAction::from_index(0), Some(MenuAction::AddMagnet));
        assert_eq!(MenuAction::from_index(1), Some(MenuAction::AddTorrentFile));
        assert_eq!(MenuAction::from_index(2), Some(MenuAction::Quit));
    }

    #[test]
    fn menu_action_out_of_bounds() {
        assert_eq!(MenuAction::from_index(-1), None);
        assert_eq!(MenuAction::from_index(3), None);
        assert_eq!(MenuAction::from_index(100), None);
    }

    #[test]
    fn test_selection_single_click() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        state.selection_click("abc123");
        assert!(state.selected.contains("abc123"));
        assert_eq!(state.selected.len(), 1);
        // Second click clears first
        state.selection_click("def456");
        assert!(!state.selected.contains("abc123"));
        assert!(state.selected.contains("def456"));
        assert_eq!(state.selected.len(), 1);
    }

    #[test]
    fn test_selection_ctrl_toggle() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        state.selection_ctrl_click("abc123");
        assert!(state.selected.contains("abc123"));
        state.selection_ctrl_click("def456");
        assert_eq!(state.selected.len(), 2);
        // Toggle off
        state.selection_ctrl_click("abc123");
        assert!(!state.selected.contains("abc123"));
        assert_eq!(state.selected.len(), 1);
    }

    #[test]
    fn test_selection_shift_range() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        state.current_order = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
        // Click "b" first (sets anchor)
        state.selection_click("b");
        // Shift-click "d" — should select b, c, d
        state.selection_shift_click("d");
        assert_eq!(state.selected.len(), 3);
        assert!(state.selected.contains("b"));
        assert!(state.selected.contains("c"));
        assert!(state.selected.contains("d"));
    }

    #[test]
    fn gui_command_variants_construct() {
        // Verify each GuiCommand variant can be constructed without panic.
        let _add_magnet = GuiCommand::AddMagnet {
            uri: "magnet:?xt=urn:btih:abc".into(),
            download_dir: None,
        };
        let _add_torrent = GuiCommand::AddTorrentFile {
            path: "/tmp/test.torrent".into(),
            download_dir: Some("/tmp/dl".into()),
        };
        let _pause = GuiCommand::PauseTorrents {
            hashes: vec!["aabb".into()],
        };
        let _resume = GuiCommand::ResumeTorrents {
            hashes: vec!["ccdd".into()],
        };
        let _remove = GuiCommand::RemoveTorrents {
            hashes: vec!["eeff".into()],
            delete_files: true,
        };
        let _seed = GuiCommand::SetSeedMode {
            hashes: vec!["1122".into()],
            enabled: true,
        };
        let _recheck = GuiCommand::ForceRecheck {
            hashes: vec!["3344".into()],
        };
        let _reannounce = GuiCommand::ForceReannounce {
            hashes: vec!["5566".into()],
        };
    }

    #[test]
    fn context_action_from_index_valid() {
        assert_eq!(ContextAction::from_index(0), Some(ContextAction::Pause));
        assert_eq!(ContextAction::from_index(1), Some(ContextAction::Resume));
        assert_eq!(ContextAction::from_index(2), Some(ContextAction::SeedOnly));
        assert_eq!(
            ContextAction::from_index(3),
            Some(ContextAction::ResumeDownload)
        );
        assert_eq!(ContextAction::from_index(4), Some(ContextAction::Remove));
        assert_eq!(
            ContextAction::from_index(5),
            Some(ContextAction::RemoveAndDelete)
        );
        assert_eq!(ContextAction::from_index(6), Some(ContextAction::Recheck));
        assert_eq!(
            ContextAction::from_index(7),
            Some(ContextAction::ForceReannounce)
        );
    }

    #[test]
    fn context_action_from_index_invalid() {
        assert_eq!(ContextAction::from_index(-1), None);
        assert_eq!(ContextAction::from_index(8), None);
        assert_eq!(ContextAction::from_index(100), None);
    }

    #[test]
    fn select_all_populates() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        let hashes: Vec<String> = vec!["aaa".into(), "bbb".into(), "ccc".into()];
        state.select_all(&hashes);
        assert_eq!(state.selected.len(), 3);
        assert!(state.selected.contains("aaa"));
        assert!(state.selected.contains("bbb"));
        assert!(state.selected.contains("ccc"));
    }

    #[test]
    fn select_all_empty() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        // Pre-populate to verify it clears.
        state.selected.insert("existing".into());
        state.select_all(&[]);
        assert!(state.selected.is_empty());
    }

    #[test]
    fn menu_action_index_stability() {
        // Regression: indices 0/1/2 must map to the same actions across releases.
        assert_eq!(MenuAction::from_index(0), Some(MenuAction::AddMagnet));
        assert_eq!(MenuAction::from_index(1), Some(MenuAction::AddTorrentFile));
        assert_eq!(MenuAction::from_index(2), Some(MenuAction::Quit));
    }

    // ── ContextMenuState tests ────────────────────────────────────────────

    #[test]
    fn ctx_menu_empty_selection_disables_all() {
        let state = ContextMenuState::compute(&[]);
        assert!(!state.can_pause);
        assert!(!state.can_resume);
        assert!(!state.can_seed_only);
        assert!(!state.can_resume_download);
        assert!(!state.can_recheck);
    }

    #[test]
    fn ctx_menu_all_paused() {
        let state = ContextMenuState::compute(&["paused", "paused"]);
        // Can't pause something already paused.
        assert!(!state.can_pause);
        // Can resume paused torrents.
        assert!(state.can_resume);
        // Paused is not seed mode.
        assert!(state.can_seed_only);
        assert!(!state.can_resume_download);
        assert!(state.can_recheck);
    }

    #[test]
    fn ctx_menu_all_downloading() {
        let state = ContextMenuState::compute(&["downloading", "downloading"]);
        assert!(state.can_pause);
        assert!(!state.can_resume);
        assert!(state.can_seed_only);
        assert!(!state.can_resume_download);
        assert!(state.can_recheck);
    }

    #[test]
    fn ctx_menu_mixed_paused_and_downloading() {
        let state = ContextMenuState::compute(&["paused", "downloading"]);
        // At least one not paused → can pause.
        assert!(state.can_pause);
        // At least one paused → can resume.
        assert!(state.can_resume);
        assert!(state.can_seed_only);
        assert!(!state.can_resume_download);
        assert!(state.can_recheck);
    }

    #[test]
    fn ctx_menu_includes_seed_only() {
        let state = ContextMenuState::compute(&["seed only", "downloading"]);
        assert!(state.can_pause);
        assert!(!state.can_resume);
        assert!(state.can_seed_only);
        // At least one in seed mode → can resume download.
        assert!(state.can_resume_download);
        assert!(state.can_recheck);
    }

    #[test]
    fn ctx_menu_all_seed_only() {
        let state = ContextMenuState::compute(&["seed only", "seed only"]);
        assert!(state.can_pause);
        assert!(!state.can_resume);
        // All in seed mode → can't set seed only.
        assert!(!state.can_seed_only);
        assert!(state.can_resume_download);
        assert!(state.can_recheck);
    }

    #[test]
    fn ctx_menu_fetching_metadata_disables_seed_and_recheck() {
        let state = ContextMenuState::compute(&["fetching metadata", "downloading"]);
        assert!(state.can_pause);
        assert!(!state.can_resume);
        // Fetching metadata → can't seed only, can't recheck.
        assert!(!state.can_seed_only);
        assert!(!state.can_resume_download);
        assert!(!state.can_recheck);
    }

    #[test]
    fn ctx_menu_checking_disables_seed_and_recheck() {
        let state = ContextMenuState::compute(&["checking"]);
        assert!(state.can_pause);
        assert!(!state.can_resume);
        assert!(!state.can_seed_only);
        assert!(!state.can_resume_download);
        assert!(!state.can_recheck);
    }

    #[test]
    fn ctx_menu_seeding_state() {
        let state = ContextMenuState::compute(&["seeding"]);
        assert!(state.can_pause);
        assert!(!state.can_resume);
        assert!(state.can_seed_only);
        assert!(!state.can_resume_download);
        assert!(state.can_recheck);
    }

    #[test]
    fn ctx_menu_single_paused() {
        let state = ContextMenuState::compute(&["paused"]);
        assert!(!state.can_pause);
        assert!(state.can_resume);
        assert!(state.can_seed_only);
        assert!(!state.can_resume_download);
        assert!(state.can_recheck);
    }
}
