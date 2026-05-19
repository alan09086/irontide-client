use std::collections::{HashMap, HashSet};

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
    CreateTorrent,
    Preferences,
    Quit,
}

/// M185: Engine-backed settings from the Preferences dialog.
///
/// Boxed through the `GuiCommand` channel to keep the enum size small.
/// Each `Option` field is `Some` only when the user changed it.
#[derive(Debug, Default)]
pub struct EnginePrefs {
    pub download_dir: Option<String>,
    pub create_subfolder: Option<bool>,
    pub listen_port: Option<u16>,
    pub randomize_port_on_startup: Option<bool>,
    pub enable_upnp: Option<bool>,
    pub enable_natpmp: Option<bool>,
    pub max_connections_global: Option<i32>,
    pub max_peers_per_torrent: Option<usize>,
    pub max_upload_slots_global: Option<i32>,
    pub max_upload_slots_per_torrent: Option<i32>,
    pub active_downloads: Option<i32>,
    pub active_seeds: Option<i32>,
    pub active_limit: Option<i32>,
    pub proxy_type: Option<String>,
    pub proxy_host: Option<String>,
    pub proxy_port: Option<u16>,
    pub proxy_peer_connections: Option<bool>,
    pub proxy_hostnames: Option<bool>,
    pub ip_filter_enabled: Option<bool>,
    pub ip_filter_path: Option<String>,
    pub ip_filter_auto_refresh: Option<bool>,
    pub download_rate_limit: Option<u64>,
    pub upload_rate_limit: Option<u64>,
    pub dl_limit_enabled: Option<bool>,
    pub ul_limit_enabled: Option<bool>,
    pub alt_download_rate_limit: Option<u64>,
    pub alt_upload_rate_limit: Option<u64>,
    pub alt_speed_enabled: Option<bool>,
    pub rate_limit_includes_overhead: Option<bool>,
    pub rate_limit_utp: Option<bool>,
    pub rate_limit_lan: Option<bool>,
    pub encryption_mode: Option<String>,
    pub anonymous_mode: Option<bool>,
    pub queueing_enabled: Option<bool>,
    // M187: BitTorrent tab — new fields
    pub enable_dht: Option<bool>,
    pub enable_pex: Option<bool>,
    pub enable_lsd: Option<bool>,
    #[allow(clippy::option_option)]
    pub seed_ratio_limit: Option<Option<f64>>,
    pub max_ratio_action: Option<String>,
    #[allow(clippy::option_option)]
    pub seed_time_limit_secs: Option<Option<u64>>,
    #[allow(clippy::option_option)]
    pub inactive_seed_time_limit_secs: Option<Option<u64>>,
    // M187: Web UI tab
    pub qbt_compat_enabled: Option<bool>,
    pub qbt_compat_username: Option<String>,
    pub qbt_compat_bypass_local_auth: Option<bool>,
    pub qbt_compat_session_ttl: Option<u64>,
    pub qbt_compat_max_failed_auth: Option<u32>,
    pub qbt_compat_ban_duration: Option<u64>,
    pub qbt_compat_csrf: Option<bool>,
    pub qbt_compat_host_validation: Option<bool>,
    pub qbt_compat_reverse_proxy: Option<bool>,
    // v0.187.3 / 2A: Web UI port + bind under [qbt_compat]. Wired through
    // apply_engine_prefs_to_settings in bridge.rs; the runtime
    // apply_settings_classified call marks both as restart-required so the
    // bridge can post a toast.
    pub qbt_compat_port: Option<u16>,
    pub qbt_compat_bind_address: Option<String>,
    // M187: Advanced tab
    pub hashing_threads: Option<usize>,
    pub save_resume_interval_secs: Option<u64>,
    pub enable_utp: Option<bool>,
    pub enable_fast_extension: Option<bool>,
    pub enable_holepunch: Option<bool>,
    pub enable_bep40_eviction: Option<bool>,
}

/// Source of a torrent being added via the unified dialog.
#[derive(Debug, Clone)]
pub enum AddTorrentSource {
    File(String),
    Magnet(String),
}

/// M192: torrent output format for the Create Torrent dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CreateTorrentFormat {
    V1,
    #[default]
    Hybrid,
    V2,
}

impl CreateTorrentFormat {
    #[must_use]
    pub fn from_label(s: &str) -> Self {
        match s {
            "v1" => Self::V1,
            "v2" => Self::V2,
            _ => Self::Hybrid,
        }
    }

    #[allow(dead_code, reason = "M192: used in palette test assertions")]
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::Hybrid => "hybrid",
            Self::V2 => "v2",
        }
    }
}

/// M192: state for the Create Torrent dialog.
#[derive(Debug, Clone, Default)]
pub struct CreateTorrentState {
    pub source_path: String,
    pub source_name: String,
    pub source_size_bytes: u64,
    pub tracker_text: String,
    pub web_seed_text: String,
    pub comment: String,
    pub piece_size_label: String,
    pub format: CreateTorrentFormat,
    pub is_private: bool,
    pub source_tag: String,
    pub output_path: String,
    #[allow(dead_code, reason = "M192: read by handle_create_torrent for progress signalling")]
    pub is_creating: bool,
    #[allow(dead_code, reason = "M192: read by handle_create_torrent for progress signalling")]
    pub create_progress: f32,
    pub create_error: String,
}


/// File entry for the add-torrent preview.
#[derive(Debug, Clone)]
pub struct PreviewFileEntry {
    pub name: String,
    pub size: u64,
    pub is_folder: bool,
    pub depth: usize,
}

/// Preview of torrent metadata shown in the unified add-torrent dialog.
#[derive(Debug, Clone)]
pub struct AddTorrentPreview {
    pub name: String,
    pub total_size: u64,
    pub file_count: usize,
    pub created_by: Option<String>,
    pub trackers: String,
    pub files: Vec<PreviewFileEntry>,
    pub file_selected: Vec<bool>,
    pub source: AddTorrentSource,
}

/// Commands sent from the Slint UI thread to the async session thread.
///
/// The GUI callbacks are synchronous (main thread), but `SessionHandle` methods
/// are async (tokio background thread). `GuiCommand` bridges that gap via an
/// unbounded mpsc channel.
#[derive(Debug)]
pub enum GuiCommand {
    /// Add a torrent from a magnet URI (legacy path, kept for backward compat).
    #[allow(dead_code)]
    AddMagnet {
        /// The magnet URI string.
        uri: String,
        /// Optional override for the download directory.
        download_dir: Option<String>,
    },
    /// Add a torrent from a `.torrent` file path (legacy path, kept for backward compat).
    #[allow(dead_code)]
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
    /// Toggle sequential-download mode for a torrent (M177 Content tab).
    SetSequentialDownload {
        /// Hex-encoded info-hash string.
        info_hash: String,
        /// `true` to enable sequential mode, `false` to disable.
        enabled: bool,
    },
    /// M178 (TODO-1 / D-user-3): set the download priority on one or more
    /// files of a torrent. Batch form so right-clicking with several files
    /// selected applies in one dispatch.
    SetFilePriority {
        /// Hex-encoded info-hash string.
        info_hash: String,
        /// File indices to update.
        file_indices: Vec<usize>,
        /// New priority for all listed files.
        priority: irontide::core::FilePriority,
    },
    /// M178: force-reannounce a single tracker URL (Trackers tab action).
    ReannounceTracker {
        /// Hex-encoded info-hash string.
        info_hash: String,
        /// Tracker URL to reannounce. M178 ships torrent-wide reannounce
        /// only; the URL is recorded here for M180 polish (per-tracker
        /// reannounce when the engine API lands).
        #[allow(dead_code, reason = "M178: per-URL dispatch deferred to M180 polish")]
        url: String,
    },
    /// M185: apply changed session-level settings from the Preferences dialog.
    ApplySettings { engine_prefs: Box<EnginePrefs> },
    /// M191: add a torrent from the unified dialog's parsed preview.
    AddTorrentFromPreview {
        preview: AddTorrentPreview,
        download_dir: Option<String>,
        start_paused: bool,
        skip_checking: bool,
    },
    /// M192: create a .torrent file from the dialog state.
    CreateTorrent {
        state: CreateTorrentState,
    },
    /// M193: pause every torrent in the session (tray menu action).
    PauseAll,
    /// M193: resume every torrent in the session (tray menu action).
    ResumeAll,
    /// M180: set per-torrent DL/UL rate limits.
    SetTorrentRateLimit {
        /// Hex-encoded info-hash string.
        info_hash: String,
        /// Bytes/sec download limit (`Some(0)` = unlimited, `None` = don't change).
        download_limit: Option<u64>,
        /// Bytes/sec upload limit (`Some(0)` = unlimited, `None` = don't change).
        upload_limit: Option<u64>,
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
            2 => Some(Self::CreateTorrent),
            3 => Some(Self::Preferences),
            4 => Some(Self::Quit),
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
    /// Loaded from `GuiConfig` on startup, applied to the Slint `Tokens`
    /// global via [`crate::skin::SkinSettings::apply`]. Mutated by the
    /// Tweaks overlay callbacks (see `main.rs`).
    pub skin: crate::skin::SkinSettings,
    /// Whether the skin config has unsaved changes.
    ///
    /// Set by the Tweaks overlay callbacks; inspected at shutdown to
    /// persist a `GuiConfig` update via `save_gui_config`.
    pub skin_dirty: bool,
    /// Active sidebar predicate (M173 Lane A).
    ///
    /// Read by the poll loop on every tick to filter the torrent list
    /// before sorting + diffing. Mutated by sidebar row-click callbacks
    /// (task A8). The default is `SidebarPredicate::All` so the list
    /// matches the M163 behaviour until the user clicks a sidebar row.
    pub predicate: crate::sidebar::SidebarPredicate,
    /// Whether the sidebar selection has unsaved changes.
    ///
    /// Set by the sidebar row-click callback when [`Self::predicate`] or
    /// the persisted selected section moves; inspected at shutdown to
    /// persist a `GuiConfig` update via `save_gui_config`.
    ///
    /// Currently set by `set_predicate` and read by `populate_sidebar_config`
    /// at shutdown to gate the `[gui.sidebar]` save.
    pub sidebar_dirty: bool,
    /// Per-section sidebar collapsed flags (M173 Lane A task A9).
    /// Mirrors the four `<section>-collapsed` Slint properties on
    /// `MainWindow`. Initialised from `[gui.sidebar]` at startup,
    /// re-read at shutdown to compose the saved config.
    pub sidebar_library_collapsed: bool,
    /// Categories collapsed flag — see [`Self::sidebar_library_collapsed`].
    pub sidebar_category_collapsed: bool,
    /// Tags collapsed flag — see [`Self::sidebar_library_collapsed`].
    pub sidebar_tag_collapsed: bool,
    /// Trackers collapsed flag — see [`Self::sidebar_library_collapsed`].
    pub sidebar_tracker_collapsed: bool,
    /// Per-folder expansion keys for the M177 detail-pane Content tab
    /// file tree. Keys are `"{info_hash}/{folder_path}"` strings.
    /// Semantically the set holds folders the user has *explicitly
    /// collapsed* — default behaviour with the set empty is
    /// **expanded** (D-user-1). The field name mirrors the locked plan
    /// even though the plumbing tracks the inverse state. Pruned on
    /// torrent removal (D-eng-4 Iron Rule, see `main.rs` `RemoveTorrent`
    /// / `RemoveAndDelete` dispatch) to avoid a long-running session
    /// memory leak.
    pub detail_expanded: HashSet<String>,
    /// Active detail-pane tab label as set by the Slint pill row.
    /// `"General"`, `"Content"`, `"Peers"`, `"Trackers"`, or `"HTTP Sources"`.
    /// The poll loop gates the per-tick fetches on the active tab to
    /// avoid wasted work on huge torrents (D-eng-2 / D-eng-3).
    pub detail_active_tab: String,
    /// M178 (TODO-1): Selected file indices for the Content-tab multi-select
    /// + per-file priority popup.
    ///
    /// Cleared on torrent change because indices are only meaningful for
    /// the current torrent (D-eng-7).
    pub detail_files_selected: HashSet<usize>,
    /// M178 (TODO-1): Anchor index for Shift+Click range selection. Set
    /// to the most recently single-clicked file index. Cleared alongside
    /// `detail_files_selected` on torrent change.
    pub last_clicked_file_index: Option<usize>,
    /// M178 (D-eng-5 / D-eng-7): Pending per-file priority popup target —
    /// `(info_hash, file_indices_snapshot)` captured at right-click time.
    /// Cleared on popup-option-click, popup-dismiss, tab-change, and
    /// torrent-change. Last-write-wins (Issue 2.1) — a second right-click
    /// overwrites the previous pending target.
    pub pending_file_priority_target: Option<(String, Vec<usize>)>,
    /// Cached flat file list for the currently selected torrent. Used by
    /// `collect_folder_file_indices` to resolve folder paths to contained
    /// file indices for folder-level priority targeting (F9).
    pub detail_flat_files: Vec<irontide_format::FlatFileEntry>,
    /// M180: per-torrent speed histories for the Speed tab graph.
    pub speed_histories: HashMap<String, crate::speed::SpeedHistory>,
    /// M183: recently dispatched palette commands (most-recent-first, cap 5).
    pub palette_recent: Vec<crate::palette::PaletteCommandId>,
    /// M184: committed preferences state (view model).
    pub prefs: crate::prefs::PreferencesState,
    /// M184: whether preferences have been applied this session (gates persist).
    pub prefs_dirty: bool,
    /// M191: parsed torrent preview for the unified add-torrent dialog.
    pub add_torrent_preview: Option<AddTorrentPreview>,
    /// M191: active tab in the add-torrent dialog ("file"/"magnet"/"url").
    pub add_torrent_tab: String,
    /// M191: "start paused" toggle state in the add-torrent dialog.
    pub add_torrent_start_paused: bool,
    /// M191: "skip hash check" toggle state in the add-torrent dialog.
    pub add_torrent_skip_checking: bool,
    /// M192: state for the Create Torrent dialog.
    pub create_torrent: CreateTorrentState,
    /// M192: whether the Create Torrent dialog is visible.
    #[allow(dead_code, reason = "M192: read by Slint property bindings at runtime")]
    pub show_create_torrent_dialog: bool,
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
            predicate: crate::sidebar::SidebarPredicate::default(),
            sidebar_dirty: false,
            sidebar_library_collapsed: false,
            sidebar_category_collapsed: false,
            sidebar_tag_collapsed: false,
            sidebar_tracker_collapsed: false,
            detail_expanded: HashSet::new(),
            detail_active_tab: String::from("General"),
            detail_files_selected: HashSet::new(),
            last_clicked_file_index: None,
            pending_file_priority_target: None,
            detail_flat_files: Vec::new(),
            speed_histories: HashMap::new(),
            palette_recent: Vec::new(),
            prefs: crate::prefs::PreferencesState::default(),
            prefs_dirty: false,
            add_torrent_preview: None,
            add_torrent_tab: String::from("file"),
            add_torrent_start_paused: false,
            add_torrent_skip_checking: false,
            create_torrent: CreateTorrentState::default(),
            show_create_torrent_dialog: false,
        }
    }

    /// Load persisted sidebar state from a [`irontide_config::SidebarConfig`].
    ///
    /// Each absent field stays at the constructor default. Predicate
    /// tokens that fail to parse (legacy / unknown / corrupt) silently
    /// fall back to the `Library::All` default — invalid tokens never
    /// panic the GUI.
    pub fn load_sidebar_config(&mut self, cfg: &irontide_config::SidebarConfig) {
        if let Some(v) = cfg.library_collapsed {
            self.sidebar_library_collapsed = v;
        }
        if let Some(v) = cfg.category_collapsed {
            self.sidebar_category_collapsed = v;
        }
        if let Some(v) = cfg.tag_collapsed {
            self.sidebar_tag_collapsed = v;
        }
        if let Some(v) = cfg.tracker_collapsed {
            self.sidebar_tracker_collapsed = v;
        }
        if let Some(token) = &cfg.selected_predicate
            && let Some(section) = crate::sidebar::SidebarSection::from_token(token)
        {
            self.predicate = crate::sidebar::SidebarPredicate::from_section(&section);
        }
    }

    /// Compose a [`irontide_config::SidebarConfig`] snapshot of the
    /// current sidebar state, suitable for persistence at shutdown.
    ///
    /// The selected predicate is serialised as the matching
    /// `SidebarSection::to_token()` slug. Predicates that have no
    /// canonical section form (currently the bare `All` and the
    /// recursive `And`) are not persisted — the GUI falls back to the
    /// default on next launch.
    #[must_use]
    pub fn to_sidebar_config(&self) -> irontide_config::SidebarConfig {
        let selected_predicate = match &self.predicate {
            crate::sidebar::SidebarPredicate::Library(f) => {
                Some(crate::sidebar::SidebarSection::Library(*f).to_token())
            }
            crate::sidebar::SidebarPredicate::Category(name) => {
                Some(crate::sidebar::SidebarSection::Category(name.clone()).to_token())
            }
            crate::sidebar::SidebarPredicate::Tag(name) => {
                Some(crate::sidebar::SidebarSection::Tag(name.clone()).to_token())
            }
            crate::sidebar::SidebarPredicate::Tracker(b) => {
                Some(crate::sidebar::SidebarSection::Tracker(*b).to_token())
            }
            // `All` is the cold-start default — no need to persist.
            // `And` is composed at runtime and has no canonical token.
            crate::sidebar::SidebarPredicate::All | crate::sidebar::SidebarPredicate::And(_, _) => {
                None
            }
        };
        irontide_config::SidebarConfig {
            library_collapsed: Some(self.sidebar_library_collapsed),
            category_collapsed: Some(self.sidebar_category_collapsed),
            tag_collapsed: Some(self.sidebar_tag_collapsed),
            tracker_collapsed: Some(self.sidebar_tracker_collapsed),
            selected_predicate,
            // Scroll offset is reserved for future plumbing — Slint
            // does not currently surface the Flickable's vertical
            // offset through the sidebar organism. Keep the field in
            // the schema so adding it later does not bump the version.
            scroll_offset_px: None,
        }
    }

    /// Select all torrents from the provided info-hash list.
    pub fn select_all(&mut self, all_hashes: &[String]) {
        self.selected.clear();
        for h in all_hashes {
            self.selected.insert(h.clone());
        }
    }

    /// Toggle a folder key in [`Self::detail_expanded`] (M177 Step 6).
    ///
    /// Returns `true` if the key is now in the set (i.e. the folder is
    /// now collapsed); `false` if the key was just removed (i.e. now
    /// expanded). The semantic inverse of the field name is documented
    /// on the field itself.
    pub fn toggle_detail_folder(&mut self, key: &str) -> bool {
        if self.detail_expanded.remove(key) {
            false
        } else {
            self.detail_expanded.insert(key.to_owned());
            true
        }
    }

    /// Prune all [`Self::detail_expanded`] entries that belong to a
    /// removed torrent (D-eng-4 Iron Rule). The folder key namespace
    /// is `"{info_hash}/{folder_path}"`; we drop every entry whose key
    /// starts with `"{info_hash}/"`. Called from the
    /// `on_delete_confirmed` callback after the user confirms removal.
    pub fn prune_detail_expanded_for(&mut self, info_hash: &str) {
        let prefix = format!("{info_hash}/");
        self.detail_expanded.retain(|k| !k.starts_with(&prefix));
    }

    /// M180: remove speed history for a removed torrent.
    pub fn prune_speed_history_for(&mut self, info_hash: &str) {
        self.speed_histories.remove(info_hash);
    }

    /// M178 (D-eng-7 Iron Rule): clear file selection + popup state
    /// when the detail pane's torrent changes. File indices are only
    /// meaningful for the current torrent — leaving them populated would
    /// either select wrong rows or leak across torrent boundaries.
    pub fn clear_file_selection_for_torrent_change(&mut self) {
        self.detail_files_selected.clear();
        self.last_clicked_file_index = None;
        self.pending_file_priority_target = None;
        self.detail_flat_files.clear();
    }

    /// M178 (TODO-1): apply a multi-select click to the file selection.
    ///
    ///   * `ctrl=false, shift=false` → single-select (replaces the set).
    ///   * `ctrl=true` → toggle that index in/out of the set.
    ///   * `shift=true` → range-select from `last_clicked_file_index` to
    ///     the new index (inclusive). Falls back to single-select if the
    ///     anchor is `None`.
    pub fn apply_file_click(&mut self, index: usize, ctrl: bool, shift: bool) {
        if shift && let Some(anchor) = self.last_clicked_file_index {
            let (lo, hi) = if anchor <= index {
                (anchor, index)
            } else {
                (index, anchor)
            };
            self.detail_files_selected.clear();
            for i in lo..=hi {
                self.detail_files_selected.insert(i);
            }
            return;
        }
        // shift=true with no anchor falls through to single-select.
        if ctrl {
            if !self.detail_files_selected.remove(&index) {
                self.detail_files_selected.insert(index);
            }
            self.last_clicked_file_index = Some(index);
            return;
        }
        // Plain click: replace selection.
        self.detail_files_selected.clear();
        self.detail_files_selected.insert(index);
        self.last_clicked_file_index = Some(index);
    }

    /// Resolve the info-hash that should drive the detail pane (M177).
    ///
    /// Priority:
    /// 1. `last_clicked` if it is still in the [`Self::selected`] set
    ///    (handles the common case: user clicks a row, detail pane
    ///    follows that row).
    /// 2. Otherwise the first hash in [`Self::current_order`] that is
    ///    in the selection set (used after a Ctrl+A select-all, where
    ///    `last_clicked` may not even exist on screen).
    /// 3. Otherwise `None` (deselected — detail pane shows the
    ///    "Select a torrent to see details" empty state).
    ///
    /// Per D-eng-1: this stays separate from `selected: HashSet<String>`
    /// so future callers can rebuild the model snapshot without re-pushing
    /// the primary selection. Returns a `&str` borrow keyed off `self`
    /// so callers can `.map(str::to_owned)` with the lock still held.
    #[must_use]
    pub fn primary_selected(&self) -> Option<&str> {
        if let Some(h) = self.last_clicked.as_deref()
            && self.selected.contains(h)
        {
            return Some(h);
        }
        self.current_order
            .iter()
            .find(|h| self.selected.contains(h.as_str()))
            .map(String::as_str)
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

    /// Replace the active sidebar predicate.
    ///
    /// Returns `true` when the predicate actually changed (the next poll
    /// tick will rebuild the visible model). Idempotent updates do nothing
    /// and return `false` so callers can suppress UI churn.
    ///
    /// The selection set is left intact; the rebuild will hide rows that
    /// no longer match but a subsequent navigation back to the previous
    /// predicate restores the visible-row selection in place.
    ///
    /// Production callers land in task A8; tests cover the helper here.
    #[allow(dead_code)]
    pub fn set_predicate(&mut self, predicate: crate::sidebar::SidebarPredicate) -> bool {
        if self.predicate == predicate {
            return false;
        }
        self.predicate = predicate;
        self.sidebar_dirty = true;
        true
    }

    /// Shift+click: select range from `last_clicked` to this hash.
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
            if state_str == "paused" || state_str == "queued" {
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
        assert_eq!(MenuAction::from_index(2), Some(MenuAction::CreateTorrent));
        assert_eq!(MenuAction::from_index(3), Some(MenuAction::Preferences));
        assert_eq!(MenuAction::from_index(4), Some(MenuAction::Quit));
    }

    #[test]
    fn menu_action_out_of_bounds() {
        assert_eq!(MenuAction::from_index(-1), None);
        assert_eq!(MenuAction::from_index(5), None);
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
        let _rate_limit = GuiCommand::SetTorrentRateLimit {
            info_hash: "7788".into(),
            download_limit: Some(1_048_576),
            upload_limit: None,
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
        assert_eq!(MenuAction::from_index(0), Some(MenuAction::AddMagnet));
        assert_eq!(MenuAction::from_index(1), Some(MenuAction::AddTorrentFile));
        assert_eq!(MenuAction::from_index(2), Some(MenuAction::CreateTorrent));
        assert_eq!(MenuAction::from_index(3), Some(MenuAction::Preferences));
        assert_eq!(MenuAction::from_index(4), Some(MenuAction::Quit));
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
    fn ctx_menu_all_queued() {
        let state = ContextMenuState::compute(&["queued", "queued"]);
        assert!(!state.can_pause);
        assert!(state.can_resume);
        assert!(state.can_seed_only);
        assert!(!state.can_resume_download);
        assert!(state.can_recheck);
    }

    #[test]
    fn ctx_menu_mixed_queued_downloading() {
        let state = ContextMenuState::compute(&["queued", "downloading"]);
        assert!(state.can_pause);
        assert!(state.can_resume);
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

    // ── M173 Lane A task A9: sidebar config load/save ────────────────

    #[test]
    fn load_sidebar_config_applies_all_fields() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        let cfg = irontide_config::SidebarConfig {
            library_collapsed: Some(true),
            category_collapsed: Some(false),
            tag_collapsed: Some(true),
            tracker_collapsed: Some(false),
            selected_predicate: Some("category:Linux".into()),
            scroll_offset_px: Some(40.0),
        };
        state.load_sidebar_config(&cfg);
        assert!(state.sidebar_library_collapsed);
        assert!(!state.sidebar_category_collapsed);
        assert!(state.sidebar_tag_collapsed);
        assert!(!state.sidebar_tracker_collapsed);
        assert_eq!(
            state.predicate,
            crate::sidebar::SidebarPredicate::Category("Linux".into())
        );
    }

    #[test]
    fn load_sidebar_config_invalid_token_falls_back_to_default() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        let cfg = irontide_config::SidebarConfig {
            selected_predicate: Some("garbage:nonsense".into()),
            ..Default::default()
        };
        state.load_sidebar_config(&cfg);
        // Predicate stays at the constructor default.
        assert_eq!(state.predicate, crate::sidebar::SidebarPredicate::All);
    }

    #[test]
    fn to_sidebar_config_round_trips_named_predicates() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        state.predicate = crate::sidebar::SidebarPredicate::Tag("hd".into());
        state.sidebar_library_collapsed = true;
        let cfg = state.to_sidebar_config();
        assert_eq!(cfg.selected_predicate.as_deref(), Some("tag:hd"));
        assert_eq!(cfg.library_collapsed, Some(true));
    }

    #[test]
    fn to_sidebar_config_omits_default_all_predicate() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        // Default predicate is `All` — must NOT serialise (next launch
        // gets the same default, no need to persist).
        let cfg = state.to_sidebar_config();
        assert_eq!(cfg.selected_predicate, None);
    }

    // ── M173 Lane A: set_predicate ────────────────────────────────────

    #[test]
    fn set_predicate_changes_state_and_marks_dirty() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        // Default predicate is `All`, dirty flag starts clean.
        assert_eq!(state.predicate, crate::sidebar::SidebarPredicate::All);
        assert!(!state.sidebar_dirty);

        let new_pred =
            crate::sidebar::SidebarPredicate::Library(crate::sidebar::LibraryFilter::Paused);
        let changed = state.set_predicate(new_pred.clone());
        assert!(changed);
        assert_eq!(state.predicate, new_pred);
        assert!(state.sidebar_dirty);
    }

    #[test]
    fn set_predicate_idempotent_change_returns_false() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        );
        let pred = crate::sidebar::SidebarPredicate::Category("Linux".into());
        assert!(state.set_predicate(pred.clone()));
        assert!(state.sidebar_dirty);
        // Reset dirty flag — the next call should not flip it back on
        // because the predicate did not change.
        state.sidebar_dirty = false;
        let changed = state.set_predicate(pred);
        assert!(!changed);
        assert!(!state.sidebar_dirty);
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

    // ── M177 Step 1: primary_selected (detail-pane selection) ─────────

    fn fresh_state() -> AppState {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        AppState::new(
            tx,
            crate::columns::ColumnConfig::default(),
            crate::skin::SkinSettings::default(),
        )
    }

    #[test]
    fn primary_selected_none_on_empty_selection() {
        let state = fresh_state();
        assert_eq!(state.primary_selected(), None);
    }

    #[test]
    fn primary_selected_uses_last_clicked_when_selected() {
        let mut state = fresh_state();
        state.current_order = vec!["aaa".into(), "bbb".into(), "ccc".into()];
        state.selection_click("bbb");
        // last_clicked == bbb, selected == {bbb} — primary should be bbb.
        assert_eq!(state.primary_selected(), Some("bbb"));
    }

    #[test]
    fn primary_selected_falls_back_to_first_in_current_order() {
        // Ctrl+A: last_clicked may not be set, but the order-keyed
        // fallback should still find the first selected hash.
        let mut state = fresh_state();
        state.current_order = vec!["aaa".into(), "bbb".into(), "ccc".into()];
        state.last_clicked = None;
        state.select_all(&state.current_order.clone());
        assert_eq!(state.primary_selected(), Some("aaa"));
    }

    #[test]
    fn primary_selected_empty_current_order_returns_none() {
        // D-eng-5 defensive: selection set non-empty but current_order
        // empty (race during model rebuild). Must not panic and must
        // return None — the detail pane simply renders the empty state.
        let mut state = fresh_state();
        state.selected.insert("ghost".into());
        state.last_clicked = None;
        // current_order intentionally left empty.
        assert_eq!(state.primary_selected(), None);
    }

    // ── M177 Step 6: detail_expanded toggle + cleanup ─────────────────

    #[test]
    fn toggle_detail_folder_round_trip_collapses_then_expands() {
        let mut state = fresh_state();
        let key = "abcd/video";
        // First click: collapse — key now in the set.
        let collapsed = state.toggle_detail_folder(key);
        assert!(collapsed);
        assert!(state.detail_expanded.contains(key));
        // Second click: re-expand — key removed.
        let collapsed = state.toggle_detail_folder(key);
        assert!(!collapsed);
        assert!(!state.detail_expanded.contains(key));
    }

    #[test]
    fn prune_detail_expanded_for_drops_only_target_torrent_keys() {
        // D-eng-4 Iron Rule regression: removing torrent A must not
        // touch torrent B's collapsed-folder state.
        let mut state = fresh_state();
        state.detail_expanded.insert("aaaa/video".into());
        state.detail_expanded.insert("aaaa/video/extras".into());
        state.detail_expanded.insert("bbbb/movies".into());
        state.prune_detail_expanded_for("aaaa");
        assert!(!state.detail_expanded.contains("aaaa/video"));
        assert!(!state.detail_expanded.contains("aaaa/video/extras"));
        assert!(state.detail_expanded.contains("bbbb/movies"));
        assert_eq!(state.detail_expanded.len(), 1);
    }

    #[test]
    fn prune_detail_expanded_for_unrelated_hash_is_a_noop() {
        // Removing a torrent that has no folder keys mustn't blow away
        // the set or panic.
        let mut state = fresh_state();
        state.detail_expanded.insert("aaaa/video".into());
        state.prune_detail_expanded_for("cccc");
        assert!(state.detail_expanded.contains("aaaa/video"));
        assert_eq!(state.detail_expanded.len(), 1);
    }

    // ── M178 Lane D: multi-select + popup state ──────────────────────

    #[test]
    fn apply_file_click_single_select_replaces_set() {
        let mut state = fresh_state();
        state.detail_files_selected.insert(0);
        state.detail_files_selected.insert(1);
        state.apply_file_click(5, false, false);
        assert_eq!(state.detail_files_selected.len(), 1);
        assert!(state.detail_files_selected.contains(&5));
        assert_eq!(state.last_clicked_file_index, Some(5));
    }

    #[test]
    fn apply_file_click_ctrl_toggles() {
        let mut state = fresh_state();
        state.apply_file_click(2, false, false);
        state.apply_file_click(4, true, false);
        assert!(state.detail_files_selected.contains(&2));
        assert!(state.detail_files_selected.contains(&4));
        // Ctrl+Click 2 again removes it
        state.apply_file_click(2, true, false);
        assert!(!state.detail_files_selected.contains(&2));
        assert!(state.detail_files_selected.contains(&4));
    }

    #[test]
    fn apply_file_click_shift_range_selects() {
        let mut state = fresh_state();
        state.apply_file_click(2, false, false); // anchor at 2
        state.apply_file_click(5, false, true); // shift to 5
        assert!(state.detail_files_selected.contains(&2));
        assert!(state.detail_files_selected.contains(&3));
        assert!(state.detail_files_selected.contains(&4));
        assert!(state.detail_files_selected.contains(&5));
    }

    #[test]
    fn apply_file_click_shift_range_works_in_reverse() {
        let mut state = fresh_state();
        state.apply_file_click(8, false, false); // anchor at 8
        state.apply_file_click(3, false, true); // shift to 3 (reverse)
        for i in 3..=8 {
            assert!(state.detail_files_selected.contains(&i), "missing {i}");
        }
    }

    #[test]
    fn apply_file_click_shift_without_anchor_falls_through_to_single() {
        let mut state = fresh_state();
        state.apply_file_click(5, false, true);
        assert_eq!(state.detail_files_selected.len(), 1);
        assert!(state.detail_files_selected.contains(&5));
    }

    #[test]
    fn clear_file_selection_for_torrent_change_resets_all() {
        let mut state = fresh_state();
        state.detail_files_selected.insert(1);
        state.detail_files_selected.insert(2);
        state.last_clicked_file_index = Some(2);
        state.pending_file_priority_target = Some(("hash".to_owned(), vec![1, 2]));
        state.clear_file_selection_for_torrent_change();
        assert!(state.detail_files_selected.is_empty());
        assert!(state.last_clicked_file_index.is_none());
        assert!(state.pending_file_priority_target.is_none());
    }

    #[test]
    fn detail_active_tab_default_and_persistence() {
        // The active tab survives across selection changes (the
        // primary-selection plumbing never touches it). Slint's
        // `if`-branch re-mount on a layout swap reads from the same
        // AppState field via the `detail-active-tab` property binding,
        // so persistence is unit-testable here.
        let mut state = fresh_state();
        assert_eq!(state.detail_active_tab, "General");
        state.detail_active_tab = "Content".to_owned();
        // Simulate a selection change — primary handlers should not
        // touch the tab field.
        state.selection_click("aaa");
        assert_eq!(state.detail_active_tab, "Content");
    }

    // ── M191: AddTorrentPreview + dialog state ─────────────────────────

    #[test]
    fn add_torrent_preview_file_selected_toggle() {
        let mut preview = AddTorrentPreview {
            name: "test".into(),
            total_size: 100,
            file_count: 3,
            created_by: None,
            trackers: String::new(),
            files: vec![
                PreviewFileEntry {
                    name: "a.txt".into(),
                    size: 30,
                    is_folder: false,
                    depth: 0,
                },
                PreviewFileEntry {
                    name: "b.txt".into(),
                    size: 40,
                    is_folder: false,
                    depth: 0,
                },
                PreviewFileEntry {
                    name: "c.txt".into(),
                    size: 30,
                    is_folder: false,
                    depth: 0,
                },
            ],
            file_selected: vec![true, true, true],
            source: AddTorrentSource::File("/tmp/test.torrent".into()),
        };
        assert!(preview.file_selected.iter().all(|&s| s));
        preview.file_selected[1] = false;
        assert!(!preview.file_selected[1]);
        assert!(preview.file_selected[0]);
        assert!(preview.file_selected[2]);
    }

    #[test]
    fn add_torrent_preview_select_all_deselect_all() {
        let mut preview = AddTorrentPreview {
            name: "test".into(),
            total_size: 100,
            file_count: 2,
            created_by: None,
            trackers: String::new(),
            files: vec![
                PreviewFileEntry {
                    name: "a.txt".into(),
                    size: 50,
                    is_folder: false,
                    depth: 0,
                },
                PreviewFileEntry {
                    name: "b.txt".into(),
                    size: 50,
                    is_folder: false,
                    depth: 0,
                },
            ],
            file_selected: vec![true, true],
            source: AddTorrentSource::File("/tmp/test.torrent".into()),
        };
        preview.file_selected.fill(false);
        assert!(preview.file_selected.iter().all(|&s| !s));
        preview.file_selected.fill(true);
        assert!(preview.file_selected.iter().all(|&s| s));
    }

    #[test]
    fn add_torrent_dialog_state_defaults() {
        let state = fresh_state();
        assert!(state.add_torrent_preview.is_none());
        assert_eq!(state.add_torrent_tab, "file");
        assert!(!state.add_torrent_start_paused);
        assert!(!state.add_torrent_skip_checking);
    }

    #[test]
    fn add_torrent_dialog_toggle_paused() {
        let mut state = fresh_state();
        state.add_torrent_start_paused = !state.add_torrent_start_paused;
        assert!(state.add_torrent_start_paused);
        state.add_torrent_start_paused = !state.add_torrent_start_paused;
        assert!(!state.add_torrent_start_paused);
    }

    #[test]
    fn add_torrent_preview_magnet_source_has_no_files() {
        let preview = AddTorrentPreview {
            name: "Ubuntu ISO".into(),
            total_size: 0,
            file_count: 0,
            created_by: None,
            trackers: String::new(),
            files: Vec::new(),
            file_selected: Vec::new(),
            source: AddTorrentSource::Magnet("magnet:?xt=urn:btih:abc".into()),
        };
        assert!(preview.files.is_empty());
        assert!(preview.file_selected.is_empty());
    }

    // ── M192: CreateTorrentState + dialog state ──────────────────────────

    #[test]
    fn create_torrent_state_defaults() {
        let ct = CreateTorrentState::default();
        assert!(ct.source_path.is_empty());
        assert!(ct.source_name.is_empty());
        assert_eq!(ct.source_size_bytes, 0);
        assert!(ct.tracker_text.is_empty());
        assert!(ct.comment.is_empty());
        assert_eq!(ct.piece_size_label, "");
        assert_eq!(ct.format, CreateTorrentFormat::Hybrid);
        assert!(!ct.is_private);
        assert!(ct.source_tag.is_empty());
        assert!(ct.output_path.is_empty());
        assert!(!ct.is_creating);
        assert!((ct.create_progress - 0.0).abs() < f32::EPSILON);
        assert!(ct.create_error.is_empty());
    }

    #[test]
    fn create_torrent_format_label_round_trip() {
        for fmt in [
            CreateTorrentFormat::V1,
            CreateTorrentFormat::Hybrid,
            CreateTorrentFormat::V2,
        ] {
            assert_eq!(CreateTorrentFormat::from_label(fmt.label()), fmt);
        }
    }

    #[test]
    fn create_torrent_format_unknown_defaults_to_hybrid() {
        assert_eq!(
            CreateTorrentFormat::from_label("unknown"),
            CreateTorrentFormat::Hybrid
        );
    }

    #[test]
    fn create_torrent_dialog_state_defaults() {
        let state = fresh_state();
        assert!(!state.show_create_torrent_dialog);
        assert!(state.create_torrent.source_path.is_empty());
    }

    #[test]
    fn gui_command_create_torrent_constructs() {
        let _cmd = GuiCommand::CreateTorrent {
            state: CreateTorrentState {
                source_path: "/home/user/files".into(),
                source_name: "files".into(),
                source_size_bytes: 1_048_576,
                tracker_text: "http://tracker.example.com/announce".into(),
                piece_size_label: "Auto".into(),
                format: CreateTorrentFormat::V2,
                is_private: true,
                ..Default::default()
            },
        };
    }

    #[test]
    fn gui_command_add_torrent_from_preview_constructs() {
        let preview = AddTorrentPreview {
            name: "test".into(),
            total_size: 1024,
            file_count: 1,
            created_by: Some("IronTide".into()),
            trackers: "http://tracker.example.com/announce".into(),
            files: vec![PreviewFileEntry {
                name: "test.bin".into(),
                size: 1024,
                is_folder: false,
                depth: 0,
            }],
            file_selected: vec![true],
            source: AddTorrentSource::File("/tmp/test.torrent".into()),
        };
        let _cmd = GuiCommand::AddTorrentFromPreview {
            preview,
            download_dir: Some("/tmp/dl".into()),
            start_paused: true,
            skip_checking: false,
        };
    }
}
