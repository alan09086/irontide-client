use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;
use slint::ComponentHandle as _;

/// Monotonic counter so only the latest toast's timer dismisses.
static TOAST_GENERATION: AtomicU64 = AtomicU64::new(0);

use crate::app::{AppPhase, AppState, GuiCommand};

/// Spawn the session lifecycle on a background thread.
///
/// The thread builds a tokio runtime, starts the session, loads resume
/// state, signals the UI, then waits for the shutdown oneshot before
/// saving state and shutting down.
pub fn spawn_session_thread(
    settings: irontide::session::Settings,
    api_config: irontide_config::ApiConfig,
    watched_folders: Vec<irontide_config::WatchedFolder>,
    weak: slint::Weak<crate::MainWindow>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    state: Arc<Mutex<AppState>>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("irontide-session".into())
        .spawn(move || {
            let rt = irontide_config::build_runtime(&settings);
            rt.block_on(async {
                run_session(
                    settings,
                    api_config,
                    watched_folders,
                    weak,
                    shutdown_rx,
                    state,
                )
                .await;
            });
            rt.shutdown_timeout(std::time::Duration::from_secs(1));
        })
        .expect("failed to spawn session thread")
}

async fn run_session(
    settings: irontide::session::Settings,
    api_config: irontide_config::ApiConfig,
    watched_folders: Vec<irontide_config::WatchedFolder>,
    weak: slint::Weak<crate::MainWindow>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    state: Arc<Mutex<AppState>>,
) {
    // Start session.
    let session = match irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
    {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("Session start failed: {e}");
            tracing::error!("{msg}");
            state.lock().phase = AppPhase::Error(msg.clone());
            let _ = weak.upgrade_in_event_loop(move |win| {
                win.set_error_text(msg.into());
            });
            return;
        }
    };

    // Start embedded API server (default port 9080, disable with port = 0).
    let api_port = api_config.port.unwrap_or(9080);
    let _api_task = if api_port > 0 {
        let bind = api_config.bind.as_deref().unwrap_or("127.0.0.1");
        let addr: std::net::SocketAddr = match format!("{bind}:{api_port}").parse() {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("invalid API bind address: {e}");
                return;
            }
        };
        match irontide_api::ApiServer::bind(addr, session.clone()).await {
            Ok(server) => {
                tracing::info!("API server listening on {}", server.local_addr());
                Some(tokio::spawn(async move {
                    let _ = server.run().await;
                }))
            }
            Err(e) => {
                tracing::warn!("API server failed to bind on {addr}: {e}");
                None
            }
        }
    } else {
        None
    };

    // Load resume state.
    match session.load_resume_state().await {
        Ok(result) => {
            if result.restored > 0 {
                tracing::info!(
                    restored = result.restored,
                    skipped = result.skipped,
                    failed = result.failed,
                    "resume state loaded"
                );
            }
        }
        Err(e) => {
            tracing::warn!("failed to load resume state: {e}");
        }
    }

    // Create command channel and install the sender in shared state.
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    state.lock().cmd_tx = Some(cmd_tx);

    // Push default download directory to UI for dialog pre-fill.
    let default_download_dir = session
        .settings()
        .await
        .map(|s| s.download_dir.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Signal UI ready + initialise the torrent model.
    state.lock().phase = AppPhase::Ready;
    let _ = weak.upgrade_in_event_loop(move |win| {
        crate::poll::init_window(&win);
        win.set_session_ready(true);
        win.set_status_text("Ready".into());
        win.set_default_download_dir(default_download_dir.into());
    });

    // M194: start filesystem watcher for auto-add folders.
    let (watch_tx, watch_rx) = tokio::sync::mpsc::unbounded_channel();
    let _watcher = crate::watcher::FolderWatcher::start(&watched_folders, watch_tx);
    let _watch_task = tokio::spawn(crate::watcher::process_watch_events(
        watch_rx,
        session.clone(),
        weak.clone(),
    ));

    // Start poll loop and wait for shutdown or commands.
    let poll_handle = tokio::spawn(crate::poll::poll_loop(
        session.clone(),
        weak.clone(),
        state.clone(),
    ));

    // `oneshot::Receiver` is `Unpin` in tokio, so `&mut` works directly.
    // `JoinHandle` is also `Unpin`.
    let mut shutdown_rx = shutdown_rx;
    let mut poll_handle = poll_handle;
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => break,
            _ = &mut poll_handle => break,
            Some(cmd) = cmd_rx.recv() => {
                handle_gui_command(cmd, &session, &weak).await;
            }
        }
    }

    // Save resume state before shutdown.
    match session.save_resume_state().await {
        Ok(count) => {
            if count > 0 {
                tracing::info!(count, "saved resume state");
            }
        }
        Err(e) => {
            tracing::warn!("failed to save resume state: {e}");
        }
    }

    // Shutdown session.
    if let Err(e) = session.shutdown().await {
        tracing::warn!("session shutdown error: {e}");
    }
}

/// Handle a menu action from the File menu.
pub fn handle_menu_action(
    action: crate::app::MenuAction,
    weak: &slint::Weak<crate::MainWindow>,
    _state: &Arc<Mutex<AppState>>,
) {
    match action {
        crate::app::MenuAction::Quit => {
            let _ = weak.upgrade_in_event_loop(|win| {
                win.hide().ok();
            });
        }
        crate::app::MenuAction::AddMagnet => {
            let _ = weak.upgrade_in_event_loop(|win| {
                win.set_add_torrent_tab("magnet".into());
                win.set_show_add_torrent_dialog(true);
            });
        }
        crate::app::MenuAction::AddTorrentFile => {
            let _ = weak.upgrade_in_event_loop(|win| {
                win.set_add_torrent_tab("file".into());
                win.set_show_add_torrent_dialog(true);
            });
        }
        crate::app::MenuAction::CreateTorrent => {
            let _ = weak.upgrade_in_event_loop(|win| {
                win.set_show_create_torrent_dialog(true);
            });
        }
        crate::app::MenuAction::Preferences => {
            let _ = weak.upgrade_in_event_loop(|win| {
                win.set_show_preferences_dialog(true);
            });
        }
    }
}

/// Display a toast notification in the UI.
///
/// The toast auto-dismisses after 3 seconds. Each new toast increments a
/// generation counter so that only the *latest* toast's timer actually
/// hides the overlay (older timers become no-ops).
///
/// When `is_error` is `true` the toast uses `Palette.danger` as its
/// background and border colour.
pub fn show_toast(weak: &slint::Weak<crate::MainWindow>, msg: &str, is_error: bool) {
    let generation = TOAST_GENERATION
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    let text = msg.to_owned();
    let weak_for_timer = weak.clone();
    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_toast_text(text.into());
        win.set_toast_visible(true);
        win.set_toast_is_error(is_error);
        // Auto-dismiss after 3 s. The generation guard ensures that if a
        // newer toast appeared in the meantime, this callback is a no-op.
        slint::Timer::single_shot(std::time::Duration::from_secs(3), move || {
            if TOAST_GENERATION.load(Ordering::Relaxed) == generation {
                let _ = weak_for_timer.upgrade_in_event_loop(|win| {
                    win.set_toast_visible(false);
                });
            }
        });
    });
}

/// Handle a "Browse..." button click for selecting a download directory.
///
/// Spawns `rfd::FileDialog` on a separate thread because the native GTK
/// dialog blocks the calling thread. On selection, updates
/// `default-download-dir` on the main window and sends a
/// `SetDefaultDownloadDir` command to persist the change to the session
/// settings and config file.
pub fn handle_browse_download_dir(
    weak: &slint::Weak<crate::MainWindow>,
    state: &Arc<Mutex<AppState>>,
) {
    let weak = weak.clone();
    let cmd_tx = state.lock().cmd_tx.clone();
    std::thread::spawn(move || {
        let folder = rfd::FileDialog::new().pick_folder();
        if let Some(path) = folder {
            let path_str = path.to_string_lossy().into_owned();
            let dir = path_str.clone();
            let _ = weak.upgrade_in_event_loop(move |win| {
                win.set_default_download_dir(path_str.into());
            });
            // Persist the new download dir to session + config file.
            if let Some(tx) = cmd_tx {
                let _ = tx.send(GuiCommand::SetDefaultDownloadDir { dir });
            }
        }
    });
}

/// Handle a "Browse..." button for a Preferences dialog path field.
///
/// Spawns `rfd::FileDialog` on a separate thread (GTK blocks). On
/// selection, writes the result back to the `pref-download-dir` property
/// and marks the dialog dirty.
pub fn handle_browse_pref_folder(
    weak: &slint::Weak<crate::MainWindow>,
    _state: &Arc<Mutex<AppState>>,
    _field: &str,
) {
    let weak = weak.clone();
    std::thread::spawn(move || {
        let folder = rfd::FileDialog::new().pick_folder();
        if let Some(path) = folder {
            let path_str = path.to_string_lossy().into_owned();
            let _ = weak.upgrade_in_event_loop(move |win| {
                win.set_pref_download_dir(path_str.into());
                win.set_pref_dirty(true);
            });
        }
    });
}

/// Handle a "Browse..." button click for selecting a `.torrent` file.
///
/// Spawns `rfd::FileDialog` on a separate thread (GTK blocks). On
/// selection, reads and parses the torrent file, builds a full preview
/// (name, size, files, trackers, `created_by`), stores it on `AppState`,
/// and pushes the results to the main window.
pub fn handle_browse_torrent_file(
    weak: &slint::Weak<crate::MainWindow>,
    state: &std::sync::Arc<parking_lot::Mutex<crate::app::AppState>>,
) {
    let weak = weak.clone();
    let state = state.clone();
    std::thread::spawn(move || {
        let file = rfd::FileDialog::new()
            .add_filter("Torrent", &["torrent"])
            .pick_file();

        if let Some(path) = file {
            let path_str = path.to_string_lossy().into_owned();
            match std::fs::read(&path) {
                Ok(data) => match irontide::core::torrent_from_bytes_any(&data) {
                    Ok(meta) => {
                        let preview = build_preview_from_meta(
                            &meta,
                            crate::app::AddTorrentSource::File(path_str.clone()),
                        );
                        let name: String = preview.name.clone();
                        let size_str = crate::format::format_size(preview.total_size);
                        let count = i32::try_from(preview.file_count).unwrap_or(i32::MAX);
                        let trackers = preview.trackers.clone();
                        let created_by = preview.created_by.clone().unwrap_or_default();
                        let file_rows = build_sendable_file_rows(&preview);

                        let file_exts = extract_file_extensions(&preview);
                        let tracker_list = extract_tracker_urls(&preview);
                        let suggested =
                            suggest_category(&name, &file_exts, &tracker_list).unwrap_or_default();

                        state.lock().add_torrent_preview = Some(preview);

                        let _ = weak.upgrade_in_event_loop(move |win| {
                            win.set_add_torrent_file_path(path_str.into());
                            win.set_add_torrent_preview_name(name.into());
                            win.set_add_torrent_preview_size(size_str.into());
                            win.set_add_torrent_preview_file_count(count);
                            win.set_add_torrent_preview_trackers(trackers.into());
                            win.set_add_torrent_preview_created_by(created_by.into());
                            win.set_add_torrent_suggested_category(suggested.into());
                            let model = slint::ModelRc::new(slint::VecModel::from(file_rows));
                            win.set_add_torrent_preview_files(model);
                        });
                    }
                    Err(e) => {
                        show_toast(&weak, &format!("Failed to parse torrent: {e}"), true);
                    }
                },
                Err(e) => {
                    show_toast(&weak, &format!("Failed to read file: {e}"), true);
                }
            }
        }
    });
}

/// Build an `AddTorrentPreview` from parsed torrent metadata.
///
/// `source` is consumed and stored verbatim on the returned preview — caller
/// chooses whether the bytes came from a local file, a magnet (not currently
/// used here — magnet uses a separate skeleton path), or a URL fetch (M218).
pub(crate) fn build_preview_from_meta(
    meta: &irontide::core::TorrentMeta,
    source: crate::app::AddTorrentSource,
) -> crate::app::AddTorrentPreview {
    let (name, total_size, file_count) = extract_torrent_info(meta);

    let (created_by, trackers, files) = match meta {
        irontide::core::TorrentMeta::V1(v1) => {
            let cb = v1.created_by.clone();
            let tr = extract_trackers_v1(v1);
            let fs = v1
                .info
                .files()
                .iter()
                .map(|f| crate::app::PreviewFileEntry {
                    name: f.path.join("/"),
                    size: f.length,
                    is_folder: false,
                    depth: 0,
                })
                .collect::<Vec<_>>();
            (cb, tr, fs)
        }
        irontide::core::TorrentMeta::V2(v2) => {
            let cb = v2.created_by.clone();
            let tr = extract_trackers_v2(v2);
            let fs = v2
                .info
                .files()
                .iter()
                .map(|f| crate::app::PreviewFileEntry {
                    name: f.path.join("/"),
                    size: f.attr.length,
                    is_folder: false,
                    depth: 0,
                })
                .collect::<Vec<_>>();
            (cb, tr, fs)
        }
        irontide::core::TorrentMeta::Hybrid(v1, _v2) => {
            let cb = v1.created_by.clone();
            let tr = extract_trackers_v1(v1);
            let fs = v1
                .info
                .files()
                .iter()
                .map(|f| crate::app::PreviewFileEntry {
                    name: f.path.join("/"),
                    size: f.length,
                    is_folder: false,
                    depth: 0,
                })
                .collect::<Vec<_>>();
            (cb, tr, fs)
        }
    };

    let file_selected = vec![true; files.len()];

    crate::app::AddTorrentPreview {
        name,
        total_size,
        file_count,
        created_by,
        trackers,
        files,
        file_selected,
        source,
    }
}

/// Extract tracker URLs from a v2 torrent as a comma-separated string.
fn extract_trackers_v2(v2: &irontide::core::TorrentMetaV2) -> String {
    let mut urls = Vec::new();
    if let Some(ref ann) = v2.announce {
        urls.push(ann.clone());
    }
    if let Some(ref tiers) = v2.announce_list {
        for tier in tiers {
            for url in tier {
                if !urls.contains(url) {
                    urls.push(url.clone());
                }
            }
        }
    }
    urls.join(", ")
}

/// Extract tracker URLs from a v1 torrent as a comma-separated string.
fn extract_trackers_v1(v1: &irontide::core::TorrentMetaV1) -> String {
    let mut urls = Vec::new();
    if let Some(ref ann) = v1.announce {
        urls.push(ann.clone());
    }
    if let Some(ref tiers) = v1.announce_list {
        for tier in tiers {
            for url in tier {
                if !urls.contains(url) {
                    urls.push(url.clone());
                }
            }
        }
    }
    urls.join(", ")
}

/// Build a `Vec<AddTorrentFileRow>` from a preview (Send-safe).
pub(crate) fn build_sendable_file_rows(
    preview: &crate::app::AddTorrentPreview,
) -> Vec<crate::AddTorrentFileRow> {
    preview
        .files
        .iter()
        .zip(preview.file_selected.iter())
        .map(|(f, sel)| crate::AddTorrentFileRow {
            name: f.name.clone().into(),
            size: crate::format::format_size(f.size).into(),
            is_folder: f.is_folder,
            depth: i32::try_from(f.depth).unwrap_or(0),
            selected: *sel,
        })
        .collect()
}

/// Push updated file selection state from the preview to the Slint model.
pub fn push_add_torrent_preview_files(
    weak: &slint::Weak<crate::MainWindow>,
    preview: &crate::app::AddTorrentPreview,
) {
    let rows = build_sendable_file_rows(preview);
    let _ = weak.upgrade_in_event_loop(move |win| {
        let model = slint::ModelRc::new(slint::VecModel::from(rows));
        win.set_add_torrent_preview_files(model);
    });
}

/// Extract name, total size, and file count from parsed torrent metadata.
fn extract_torrent_info(meta: &irontide::core::TorrentMeta) -> (String, u64, usize) {
    use irontide::core::TorrentMeta;
    match meta {
        TorrentMeta::V1(v1) => {
            let name = v1.info.name.clone();
            let files = v1.info.files();
            let total_size: u64 = files.iter().map(|f| f.length).sum();
            let file_count = files.len();
            (name, total_size, file_count)
        }
        TorrentMeta::V2(v2) => {
            let name = v2.info.name.clone();
            let total_size = v2.info.total_length();
            let file_count = v2.info.files().len();
            (name, total_size, file_count)
        }
        TorrentMeta::Hybrid(v1, _v2) => {
            // Use v1 info — it has the most straightforward API.
            let name = v1.info.name.clone();
            let files = v1.info.files();
            let total_size: u64 = files.iter().map(|f| f.length).sum();
            let file_count = files.len();
            (name, total_size, file_count)
        }
    }
}

pub(crate) fn extract_file_extensions(preview: &crate::app::AddTorrentPreview) -> Vec<String> {
    preview
        .files
        .iter()
        .filter_map(|f| {
            if f.is_folder {
                return None;
            }
            f.name.rsplit('.').next().map(str::to_lowercase)
        })
        .collect()
}

pub(crate) fn extract_tracker_urls(preview: &crate::app::AddTorrentPreview) -> Vec<String> {
    preview
        .trackers
        .split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Dispatch a `GuiCommand` to the appropriate session method.
async fn handle_gui_command(
    cmd: GuiCommand,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let start = std::time::Instant::now();

    match cmd {
        GuiCommand::AddMagnet { uri, download_dir } => {
            handle_add_magnet(uri, download_dir, session, weak).await;
        }
        GuiCommand::AddTorrentFile { path, download_dir } => {
            handle_add_torrent_file(path, download_dir, session, weak).await;
        }
        GuiCommand::PauseTorrents { hashes } => {
            let msg = batch_action(&hashes, session, "Paused", |s, id| {
                Box::pin(s.pause_torrent(id))
            })
            .await;
            show_toast(weak, &msg, false);
        }
        GuiCommand::ResumeTorrents { hashes } => {
            let msg = batch_action(&hashes, session, "Resumed", |s, id| {
                Box::pin(s.resume_torrent(id))
            })
            .await;
            show_toast(weak, &msg, false);
        }
        GuiCommand::RemoveTorrents {
            hashes,
            delete_files,
        } => {
            handle_remove_torrents(&hashes, delete_files, session, weak).await;
        }
        GuiCommand::SetSeedMode { hashes, enabled } => {
            let label = if enabled {
                "Set seed mode"
            } else {
                "Cleared seed mode"
            };
            let msg = batch_action(&hashes, session, label, |s, id| {
                Box::pin(s.set_seed_mode(id, enabled))
            })
            .await;
            show_toast(weak, &msg, false);
        }
        GuiCommand::ForceRecheck { hashes } => {
            let msg = batch_action(&hashes, session, "Rechecking", |s, id| {
                Box::pin(s.force_recheck(id))
            })
            .await;
            show_toast(weak, &msg, false);
        }
        GuiCommand::ForceReannounce { hashes } => {
            let msg = batch_action(&hashes, session, "Reannounced", |s, id| {
                Box::pin(s.force_reannounce(id))
            })
            .await;
            show_toast(weak, &msg, false);
        }
        GuiCommand::ForceResumeTorrents { hashes } => {
            let msg = batch_action(&hashes, session, "Force started", |s, id| {
                Box::pin(s.force_resume_torrent(id))
            })
            .await;
            show_toast(weak, &msg, false);
        }
        GuiCommand::MoveTorrentStorage {
            info_hash,
            new_path,
        } => {
            handle_move_torrent_storage(&info_hash, &new_path, session, weak).await;
        }
        GuiCommand::SetTorrentSeedRatio { info_hash, limit } => {
            handle_set_torrent_seed_ratio(&info_hash, limit, session, weak).await;
        }
        GuiCommand::SetDefaultDownloadDir { dir } => {
            handle_set_default_download_dir(&dir, session, weak).await;
        }
        GuiCommand::SetSequentialDownload { info_hash, enabled } => {
            handle_set_sequential_download(&info_hash, enabled, session, weak).await;
        }
        GuiCommand::SetFilePriority {
            info_hash,
            file_indices,
            priority,
        } => {
            handle_set_file_priority(&info_hash, &file_indices, priority, session, weak).await;
        }
        GuiCommand::SetTorrentRateLimit {
            info_hash,
            download_limit,
            upload_limit,
        } => {
            handle_set_torrent_rate_limit(&info_hash, download_limit, upload_limit, session, weak)
                .await;
        }
        GuiCommand::ApplySettings { engine_prefs } => {
            handle_apply_engine_prefs(*engine_prefs, session, weak).await;
        }
        GuiCommand::ReannounceTracker { info_hash, url: _ } => {
            // M178: Per-tracker reannounce is not yet exposed via SessionHandle;
            // fall back to a torrent-wide reannounce (M178 ships the action,
            // M180 polish refines to per-URL when the engine API lands).
            if let Ok(id) = irontide::core::Id20::from_hex(&info_hash) {
                let _ = session.force_reannounce(id).await;
            }
        }
        GuiCommand::AddTorrentFromPreview {
            preview,
            download_dir,
            start_paused,
            skip_checking,
        } => {
            handle_add_torrent_from_preview(
                preview,
                download_dir,
                start_paused,
                skip_checking,
                session,
                weak,
            )
            .await;
        }
        GuiCommand::CreateTorrent { state } => {
            handle_create_torrent(state, weak);
        }
        GuiCommand::PauseAll => {
            handle_pause_all(session, weak).await;
        }
        GuiCommand::ResumeAll => {
            handle_resume_all(session, weak).await;
        }
        GuiCommand::OpenTorrentFile { path } => {
            handle_open_torrent_file(path, session, weak).await;
        }
        GuiCommand::OpenMagnet { uri } => {
            handle_open_magnet(&uri, session, weak).await;
        }
        GuiCommand::SearchQuery { query, plugin_name } => {
            handle_search_query(&query, plugin_name.as_deref(), weak).await;
        }
        GuiCommand::SearchAddResult { magnet_url } => {
            handle_open_magnet(&magnet_url, session, weak).await;
        }
        GuiCommand::RssAddFeed { url } => {
            handle_rss_add_feed(&url, weak).await;
        }
        GuiCommand::RssRemoveFeed { index } => {
            handle_rss_remove_feed(index, weak);
        }
        GuiCommand::RssRefreshFeeds => {
            handle_rss_refresh_feeds(weak).await;
        }
        GuiCommand::RssFeedSelected { index } => {
            handle_rss_feed_selected(index, weak);
        }
        GuiCommand::RssDownloadItem {
            index,
            selected_feed,
        } => {
            handle_rss_download_item(index, selected_feed, session, weak).await;
        }
        GuiCommand::RssMarkItemRead {
            index,
            selected_feed,
        } => {
            handle_rss_mark_item_read(index, selected_feed, weak);
        }
        GuiCommand::SchedulerToggleEnabled => {
            handle_scheduler_toggle_enabled(weak);
        }
        GuiCommand::SchedulerCellClicked { day, hour } => {
            handle_scheduler_cell_clicked(day, hour, weak);
        }
        GuiCommand::SchedulerApplyPreset { name } => {
            handle_scheduler_apply_preset(&name, weak);
        }
        GuiCommand::SchedulerLimitedRateChanged { rate_kib } => {
            handle_scheduler_limited_rate_changed(rate_kib, weak);
        }
        GuiCommand::IpFilterAddRule { label, range } => {
            handle_ip_filter_add_rule(&label, &range, weak);
        }
        GuiCommand::IpFilterRemoveRule { index } => {
            handle_ip_filter_remove_rule(index, weak);
        }
        GuiCommand::IpFilterToggleRule { index } => {
            handle_ip_filter_toggle_rule(index, weak);
        }
        GuiCommand::IpFilterUnbanPeer { ip } => {
            handle_ip_filter_unban_peer(&ip, session, weak).await;
        }
        GuiCommand::IpFilterImportFile => {
            handle_ip_filter_import_file(weak);
        }
        GuiCommand::IpFilterToggleEnabled => {
            handle_ip_filter_toggle_enabled(session, weak).await;
        }
        GuiCommand::LogsTabChanged { tab } => {
            handle_logs_tab_changed(tab, weak);
        }
        GuiCommand::LogsClear => {
            handle_logs_clear(weak);
        }
        GuiCommand::LogsSetFilter { level } => {
            handle_logs_set_filter(level, weak);
        }
        GuiCommand::CategorySuggestTrain {
            category,
            name,
            file_extensions,
            trackers,
        } => {
            handle_category_suggest_train(&category, &name, &file_extensions, &trackers);
        }
        GuiCommand::IntentSetMode { mode } => {
            handle_intent_set_mode(mode, weak);
        }
        GuiCommand::IntentApplyPreset { index } => {
            handle_intent_apply_preset(index, weak);
        }
        GuiCommand::IntentSetDetectedSpeeds { dl_kbps, ul_kbps } => {
            handle_intent_set_detected_speeds(dl_kbps, ul_kbps, weak);
        }
        GuiCommand::PhonePairRefresh => {
            push_phone_pair_state(weak);
        }
    }

    let elapsed = start.elapsed();
    if elapsed.as_millis() > 100 {
        tracing::warn!("GUI command took {elapsed:?}");
    }
}

/// Execute an async action for each info-hash and format the result.
async fn batch_action<F>(
    hashes: &[String],
    session: &irontide::session::SessionHandle,
    label: &str,
    action: F,
) -> String
where
    F: Fn(
        &irontide::session::SessionHandle,
        irontide::core::Id20,
    ) -> Pin<Box<dyn Future<Output = Result<(), irontide::session::Error>> + '_>>,
{
    if hashes.is_empty() {
        return format!("{label}: no torrents selected");
    }
    let mut success = 0usize;
    let mut failed = 0usize;
    for hash_str in hashes {
        let Ok(id) = irontide::core::Id20::from_hex(hash_str) else {
            tracing::warn!(hash = %hash_str, "invalid info hash in batch action");
            failed += 1;
            continue;
        };
        match action(session, id).await {
            Ok(()) => success += 1,
            Err(e) => {
                tracing::warn!(hash = %hash_str, error = %e, "{label} failed for torrent");
                failed += 1;
            }
        }
    }
    format_batch_result(label, success, failed)
}

/// Format the result of a batch action into a human-readable toast message.
fn format_batch_result(label: &str, success: usize, failed: usize) -> String {
    if failed == 0 {
        format!("{label} {success} torrent(s)")
    } else {
        format!("{label} {success} torrent(s), {failed} failed")
    }
}

/// Add a torrent from a magnet URI.
///
/// Parses the magnet link, constructs `AddTorrentParams`, and submits to the
/// session. Shows a toast on success or failure.
async fn handle_add_magnet(
    uri: String,
    download_dir: Option<String>,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let uri = uri.trim();
    let magnet = match irontide::core::Magnet::parse(uri) {
        Ok(m) => m,
        Err(e) => {
            show_toast(weak, &format!("Invalid magnet: {e}"), true);
            return;
        }
    };
    let display_name = magnet
        .display_name
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let mut params = irontide::AddTorrentParams::from_magnet(magnet);
    if let Some(dir) = download_dir {
        params = params.download_dir(dir);
    }
    match params.add_to(session).await {
        Ok(_id) => {
            show_toast(weak, &format!("Added: {display_name}"), false);
        }
        Err(e) => {
            show_toast(weak, &format!("Failed to add: {e}"), true);
        }
    }
}

/// Add a torrent from a `.torrent` file path.
///
/// Constructs `AddTorrentParams`, optionally overrides the download
/// directory, and submits to the session. Shows a toast on success or
/// failure. Clears the dialog's file-selection state on success.
async fn handle_add_torrent_file(
    path: String,
    download_dir: Option<String>,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let mut params = irontide::AddTorrentParams::from_file(&path);
    if let Some(dir) = download_dir {
        params = params.download_dir(dir);
    }
    let filename = std::path::Path::new(&path)
        .file_name()
        .map_or_else(|| path.clone(), |f| f.to_string_lossy().into_owned());
    match params.add_to(session).await {
        Ok(_id) => {
            // Clear file-selection state in the dialog.
            let _ = weak.upgrade_in_event_loop(|win| {
                win.set_add_torrent_file_path(slint::SharedString::new());
                win.set_add_torrent_preview_name(slint::SharedString::new());
                win.set_add_torrent_preview_size(slint::SharedString::new());
                win.set_add_torrent_preview_file_count(0);
            });
            show_toast(weak, &format!("Added: {filename}"), false);
        }
        Err(e) => {
            show_toast(weak, &format!("Failed to add torrent: {e}"), true);
        }
    }
}

/// M191: add a torrent from the unified dialog's parsed preview.
async fn handle_add_torrent_from_preview(
    preview: crate::app::AddTorrentPreview,
    download_dir: Option<String>,
    start_paused: bool,
    skip_checking: bool,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let display_name = preview.name.clone();
    let skipped_files: Vec<usize> = preview
        .file_selected
        .iter()
        .enumerate()
        .filter(|(_, sel)| !**sel)
        .map(|(i, _)| i)
        .collect();

    let result = match preview.source {
        crate::app::AddTorrentSource::File(ref path) => {
            let data = match std::fs::read(path) {
                Ok(d) => d,
                Err(e) => {
                    show_toast(weak, &format!("Failed to read file: {e}"), true);
                    return;
                }
            };
            let mut params = irontide::AddTorrentParams::from_bytes(data);
            if let Some(dir) = download_dir {
                params = params.download_dir(dir);
            }
            if start_paused {
                params = params.paused(true);
            }
            if skip_checking {
                params = params.skip_checking(true);
            }
            params.add_to(session).await
        }
        crate::app::AddTorrentSource::Magnet(ref uri) => {
            let magnet = match irontide::core::Magnet::parse(uri) {
                Ok(m) => m,
                Err(e) => {
                    show_toast(weak, &format!("Invalid magnet: {e}"), true);
                    return;
                }
            };
            let mut params = irontide::AddTorrentParams::from_magnet(magnet);
            if let Some(dir) = download_dir {
                params = params.download_dir(dir);
            }
            if start_paused {
                params = params.paused(true);
            }
            if skip_checking {
                params = params.skip_checking(true);
            }
            params.add_to(session).await
        }
        crate::app::AddTorrentSource::UrlBytes { ref bytes, .. } => {
            let mut params = irontide::AddTorrentParams::from_bytes(bytes.clone());
            if let Some(dir) = download_dir {
                params = params.download_dir(dir);
            }
            if start_paused {
                params = params.paused(true);
            }
            if skip_checking {
                params = params.skip_checking(true);
            }
            params.add_to(session).await
        }
    };

    match result {
        Ok(id) => {
            for &idx in &skipped_files {
                let _ = session
                    .set_file_priority(id, idx, irontide::core::FilePriority::Skip)
                    .await;
            }
            let _ = weak.upgrade_in_event_loop(|win| {
                win.set_show_add_torrent_dialog(false);
                win.set_add_torrent_preview_name(slint::SharedString::new());
                win.set_add_torrent_preview_size(slint::SharedString::new());
                win.set_add_torrent_preview_file_count(0);
                win.set_add_torrent_suggested_category(slint::SharedString::new());
            });
            show_toast(weak, &format!("Added: {display_name}"), false);
        }
        Err(e) => {
            show_toast(weak, &format!("Failed to add: {e}"), true);
        }
    }
}

/// Remove one or more torrents, optionally deleting their files from disk.
///
/// When `delete_files` is `true`, each torrent's data is located via
/// `torrent_stats().save_path` + `torrent_info().name`, canonicalised, and
/// verified to be within the save directory before deletion. This prevents
/// path-traversal attacks where a malicious torrent name like `../../etc`
/// could escape the download directory.
///
/// The torrent is removed from the session *before* files are deleted so the
/// session no longer holds any file handles.
async fn handle_remove_torrents(
    hashes: &[String],
    delete_files: bool,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    if hashes.is_empty() {
        show_toast(weak, "Remove: no torrents selected", false);
        return;
    }

    let mut success = 0usize;
    let mut failed = 0usize;

    for hash_str in hashes {
        let Ok(id) = irontide::core::Id20::from_hex(hash_str) else {
            tracing::warn!(hash = %hash_str, "invalid info hash for remove");
            failed += 1;
            continue;
        };

        if delete_files {
            // Gather info needed for file deletion *before* removing the torrent.
            let file_info = gather_delete_info(session, id).await;

            // Remove from session first so file handles are released.
            if let Err(e) = session.remove_torrent(id).await {
                tracing::warn!(hash = %hash_str, error = %e, "remove_torrent failed");
                failed += 1;
                continue;
            }

            // Attempt file deletion if we gathered enough info.
            if let Some((save_path, name, file_count)) = file_info {
                delete_torrent_files(&save_path, &name, file_count);
            }
        } else if let Err(e) = session.remove_torrent(id).await {
            tracing::warn!(hash = %hash_str, error = %e, "remove_torrent failed");
            failed += 1;
            continue;
        }

        success += 1;
    }

    let label = if delete_files {
        "Removed + deleted files"
    } else {
        "Removed"
    };
    show_toast(weak, &format_batch_result(label, success, failed), false);
}

/// Gather the save path, torrent name, and file count needed for deletion.
///
/// Returns `None` if either `torrent_stats` or `torrent_info` fails (e.g. the
/// torrent is a magnet that never fetched metadata).
///
/// If the torrent's `save_path` is relative (e.g. `.`), it is resolved against
/// the session's current `download_dir` to produce an absolute path.
async fn gather_delete_info(
    session: &irontide::session::SessionHandle,
    id: irontide::core::Id20,
) -> Option<(std::path::PathBuf, String, usize)> {
    let stats = match session.torrent_stats(id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "cannot get torrent_stats for file deletion");
            return None;
        }
    };
    let info = match session.torrent_info(id).await {
        Ok(i) => i,
        Err(e) => {
            tracing::debug!(error = %e, "cannot get torrent_info for file deletion");
            return None;
        }
    };
    let mut save_path = std::path::PathBuf::from(&stats.save_path);

    // If save_path is relative, resolve it. The session stores download_dir
    // which may be "." when no config file exists. Canonicalize to get the
    // actual absolute path.
    if save_path.is_relative() {
        save_path = match save_path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // Fallback: try the session's current download_dir.
                match session.settings().await {
                    Ok(s) => {
                        let abs = s.download_dir;
                        abs.canonicalize().unwrap_or(abs)
                    }
                    Err(_) => save_path,
                }
            }
        };
    }

    tracing::debug!(
        save_path = %save_path.display(),
        name = %info.name,
        files = info.files.len(),
        "gathered delete info"
    );
    Some((save_path, info.name.clone(), info.files.len()))
}

/// Delete a torrent's files from disk with path-traversal protection.
///
/// The target path is canonicalised and verified to be a child of the save
/// directory. Single-file torrents are removed with `remove_file`; multi-file
/// torrents (directories) with `remove_dir_all`.
fn delete_torrent_files(save_path: &std::path::Path, name: &str, file_count: usize) {
    let torrent_path = save_path.join(name);

    let Ok(canonical) = torrent_path.canonicalize() else {
        // File may not exist (magnet that never downloaded anything).
        tracing::debug!(
            path = %torrent_path.display(),
            "cannot canonicalize torrent path, skipping file deletion"
        );
        return;
    };

    let canonical_save = match save_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                path = %save_path.display(),
                error = %e,
                "cannot canonicalize save_path, skipping file deletion"
            );
            return;
        }
    };

    // PATH TRAVERSAL SAFETY: verify the file is within the save directory.
    if !canonical.starts_with(&canonical_save) {
        tracing::warn!(
            path = %canonical.display(),
            save_path = %canonical_save.display(),
            "path traversal detected, refusing to delete"
        );
        return;
    }

    if file_count > 1 {
        // Multi-file torrent: remove the directory tree.
        if let Err(e) = std::fs::remove_dir_all(&canonical) {
            tracing::warn!(
                path = %canonical.display(),
                error = %e,
                "failed to delete torrent directory"
            );
        } else {
            tracing::info!(path = %canonical.display(), "deleted torrent directory");
        }
    } else {
        // Single-file torrent: remove just the file.
        if let Err(e) = std::fs::remove_file(&canonical) {
            tracing::warn!(
                path = %canonical.display(),
                error = %e,
                "failed to delete torrent file"
            );
        } else {
            tracing::info!(path = %canonical.display(), "deleted torrent file");
        }
    }
}

/// Update the default download directory in the session settings, config file, and UI.
///
/// Updates the session's `download_dir`, persists to the TOML config file, and
/// pushes the new value to the UI for dialog pre-fill.
async fn handle_set_default_download_dir(
    dir: &str,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let path = std::path::PathBuf::from(dir);

    // Update session settings (download_dir only; resume files stay in XDG state dir).
    match session.settings().await {
        Ok(mut settings) => {
            settings.download_dir = path.clone();
            if let Err(e) = session.apply_settings(settings).await {
                tracing::warn!(error = %e, "failed to apply download dir to session");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to read session settings");
        }
    }

    // Persist to config file.
    if let Err(e) = irontide_config::save_session_download_dir(None, &path) {
        tracing::warn!(error = %e, "failed to persist download dir to config file");
    } else {
        tracing::info!(dir = %path.display(), "saved download dir to config");
    }

    // Update UI.
    let dir_owned = dir.to_owned();
    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_default_download_dir(dir_owned.into());
    });

    show_toast(weak, &format!("Download directory: {dir}"), false);
}

/// Toggle sequential-download mode for a torrent (M177 Content tab).
///
/// Mirrors the existing batch-action pattern but for a single torrent.
/// On failure, surfaces the engine error string in a toast so the user
/// knows the toggle didn't take.
async fn handle_set_sequential_download(
    info_hash: &str,
    enabled: bool,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let Ok(id) = irontide::core::Id20::from_hex(info_hash) else {
        show_toast(weak, &format!("Bad info-hash: {info_hash}"), true);
        return;
    };
    match session.set_sequential_download(id, enabled).await {
        Ok(()) => {
            let label = if enabled {
                "Sequential download enabled"
            } else {
                "Sequential download disabled"
            };
            show_toast(weak, label, false);
        }
        Err(e) => {
            show_toast(weak, &format!("Sequential toggle failed: {e}"), true);
        }
    }
}

async fn handle_move_torrent_storage(
    info_hash: &str,
    new_path: &str,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let Ok(id) = irontide::core::Id20::from_hex(info_hash) else {
        show_toast(weak, &format!("Bad info-hash: {info_hash}"), true);
        return;
    };
    match session
        .move_torrent_storage(id, std::path::PathBuf::from(new_path))
        .await
    {
        Ok(()) => show_toast(weak, &format!("Moved to {new_path}"), false),
        Err(e) => show_toast(weak, &format!("Move failed: {e}"), true),
    }
}

async fn handle_set_torrent_seed_ratio(
    info_hash: &str,
    limit: Option<f64>,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let Ok(id) = irontide::core::Id20::from_hex(info_hash) else {
        show_toast(weak, &format!("Bad info-hash: {info_hash}"), true);
        return;
    };
    match session.set_torrent_seed_ratio(id, limit).await {
        Ok(()) => {
            let msg = limit.map_or_else(
                || "Seed ratio: using session default".to_owned(),
                |r| format!("Seed ratio limit set to {r:.1}"),
            );
            show_toast(weak, &msg, false);
        }
        Err(e) => show_toast(weak, &format!("Set ratio failed: {e}"), true),
    }
}

/// M178: Apply a priority change to one or more files of a torrent.
///
/// Iterates the index list, calling `session.set_file_priority` per
/// index. Per-file errors (mid-flight torrent removal, invalid index)
/// are debug-logged and absorbed — selection state is independent of
/// dispatch outcome. A single toast summarises the operation.
async fn handle_set_file_priority(
    info_hash: &str,
    file_indices: &[usize],
    priority: irontide::core::FilePriority,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let Ok(id) = irontide::core::Id20::from_hex(info_hash) else {
        show_toast(weak, &format!("Bad info-hash: {info_hash}"), true);
        return;
    };
    let mut applied = 0usize;
    let mut failed = 0usize;
    for &idx in file_indices {
        match session.set_file_priority(id, idx, priority).await {
            Ok(()) => applied = applied.saturating_add(1),
            Err(e) => {
                failed = failed.saturating_add(1);
                tracing::debug!(
                    info_hash,
                    idx,
                    ?priority,
                    error = %e,
                    "set_file_priority failed",
                );
            }
        }
    }
    let total = file_indices.len();
    let label = match priority {
        irontide::core::FilePriority::Skip => "Skip",
        irontide::core::FilePriority::Low => "Low",
        irontide::core::FilePriority::Normal => "Normal",
        irontide::core::FilePriority::High => "High",
    };
    let msg = if failed == 0 {
        format!("{label} priority applied to {applied}/{total} files")
    } else {
        format!("{label} priority applied to {applied}/{total} files ({failed} failed)")
    };
    show_toast(weak, &msg, failed > 0);
}

async fn handle_set_torrent_rate_limit(
    info_hash: &str,
    download_limit: Option<u64>,
    upload_limit: Option<u64>,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let Ok(id) = irontide::core::Id20::from_hex(info_hash) else {
        show_toast(weak, &format!("Bad info-hash: {info_hash}"), true);
        return;
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(bytes) = download_limit {
        match session.set_download_limit(id, bytes).await {
            Ok(()) => {
                if bytes == 0 {
                    parts.push("DL limit cleared".to_owned());
                } else {
                    parts.push(format!(
                        "DL limit set to {}/s",
                        irontide_format::format_size(bytes)
                    ));
                }
            }
            Err(e) => {
                show_toast(weak, &format!("Failed to set DL limit: {e}"), true);
                return;
            }
        }
    }
    if let Some(bytes) = upload_limit {
        match session.set_upload_limit(id, bytes).await {
            Ok(()) => {
                if bytes == 0 {
                    parts.push("UL limit cleared".to_owned());
                } else {
                    parts.push(format!(
                        "UL limit set to {}/s",
                        irontide_format::format_size(bytes)
                    ));
                }
            }
            Err(e) => {
                show_toast(weak, &format!("Failed to set UL limit: {e}"), true);
                return;
            }
        }
    }
    if !parts.is_empty() {
        show_toast(weak, &parts.join(", "), false);
    }
}

async fn handle_apply_engine_prefs(
    ep: crate::app::EnginePrefs,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    if let Some(ref dir) = ep.download_dir {
        handle_set_default_download_dir(dir, session, weak).await;
    }

    let Ok(mut settings) = session.settings().await else {
        tracing::warn!("failed to read session settings for prefs apply");
        return;
    };

    let mut changed = false;

    if let Some(port) = ep.listen_port {
        settings.listen_port = port;
        changed = true;
    }
    if let Some(v) = ep.randomize_port_on_startup {
        settings.randomize_port_on_startup = v;
        changed = true;
    }
    if let Some(v) = ep.enable_upnp {
        settings.enable_upnp = v;
        changed = true;
    }
    if let Some(v) = ep.enable_natpmp {
        settings.enable_natpmp = v;
        changed = true;
    }
    if let Some(v) = ep.max_connections_global {
        settings.max_connections_global = v;
        changed = true;
    }
    if let Some(v) = ep.max_peers_per_torrent {
        settings.max_peers_per_torrent = v;
        changed = true;
    }
    if let Some(v) = ep.max_upload_slots_global {
        settings.max_upload_slots_global = v;
        changed = true;
    }
    if let Some(v) = ep.max_upload_slots_per_torrent {
        settings.max_upload_slots_per_torrent = v;
        changed = true;
    }
    if let Some(v) = ep.active_downloads {
        settings.active_downloads = v;
        changed = true;
    }
    if let Some(v) = ep.active_seeds {
        settings.active_seeds = v;
        changed = true;
    }
    if let Some(v) = ep.active_limit {
        settings.active_limit = v;
        changed = true;
    }
    if let Some(ref dl) = ep.download_rate_limit {
        settings.download_rate_limit = *dl;
        changed = true;
    }
    if let Some(ref ul) = ep.upload_rate_limit {
        settings.upload_rate_limit = *ul;
        changed = true;
    }
    if let Some(v) = ep.alt_download_rate_limit {
        settings.alt_download_rate_limit = v;
        changed = true;
    }
    if let Some(v) = ep.alt_upload_rate_limit {
        settings.alt_upload_rate_limit = v;
        changed = true;
    }
    if let Some(v) = ep.alt_speed_enabled {
        settings.alt_speed_enabled = v;
        changed = true;
    }
    if let Some(v) = ep.rate_limit_includes_overhead {
        settings.rate_limit_includes_overhead = v;
        changed = true;
    }
    if let Some(v) = ep.rate_limit_utp {
        settings.rate_limit_utp = v;
        changed = true;
    }
    if let Some(v) = ep.rate_limit_lan {
        settings.rate_limit_lan = v;
        changed = true;
    }
    if let Some(v) = ep.ip_filter_enabled {
        settings.ip_filter_enabled = v;
        changed = true;
    }
    if let Some(ref v) = ep.ip_filter_path {
        settings.ip_filter_path.clone_from(v);
        changed = true;
    }
    if let Some(v) = ep.ip_filter_auto_refresh {
        settings.ip_filter_auto_refresh = v;
        changed = true;
    }

    // BitTorrent
    if let Some(v) = ep.enable_dht {
        settings.enable_dht = v;
        changed = true;
    }
    if let Some(v) = ep.enable_pex {
        settings.enable_pex = v;
        changed = true;
    }
    if let Some(v) = ep.enable_lsd {
        settings.enable_lsd = v;
        changed = true;
    }
    if let Some(ref label) = ep.encryption_mode {
        settings.encryption_mode = match label.as_str() {
            "Prefer encryption" => irontide::wire::mse::EncryptionMode::Enabled,
            "Require encryption" => irontide::wire::mse::EncryptionMode::Forced,
            _ => irontide::wire::mse::EncryptionMode::Disabled,
        };
        changed = true;
    }
    if let Some(v) = ep.anonymous_mode {
        settings.anonymous_mode = v;
        changed = true;
    }
    if let Some(v) = ep.queueing_enabled {
        settings.queueing_enabled = v;
        changed = true;
    }
    if let Some(ref v) = ep.seed_ratio_limit {
        settings.seed_ratio_limit = *v;
        changed = true;
    }
    if let Some(ref label) = ep.max_ratio_action {
        use irontide::session::MaxRatioAction;
        settings.max_ratio_action = match label.as_str() {
            "Remove torrent" => MaxRatioAction::Remove,
            "Super-seeding mode" => MaxRatioAction::EnableSuperSeeding,
            _ => MaxRatioAction::Pause,
        };
        changed = true;
    }
    if let Some(ref v) = ep.seed_time_limit_secs {
        settings.seed_time_limit_secs = *v;
        changed = true;
    }
    if let Some(ref v) = ep.inactive_seed_time_limit_secs {
        settings.inactive_seed_time_limit_secs = *v;
        changed = true;
    }

    // Web UI (qbt_compat)
    if let Some(v) = ep.qbt_compat_enabled {
        settings.qbt_compat.enabled = v;
        changed = true;
    }
    if let Some(ref v) = ep.qbt_compat_username {
        settings.qbt_compat.username.clone_from(v);
        changed = true;
    }
    if let Some(v) = ep.qbt_compat_bypass_local_auth {
        settings.qbt_compat.bypass_local_auth = v;
        changed = true;
    }
    if let Some(v) = ep.qbt_compat_session_ttl {
        settings.qbt_compat.session_ttl_secs = v;
        changed = true;
    }
    if let Some(v) = ep.qbt_compat_max_failed_auth {
        settings.qbt_compat.max_failed_auth_count = v;
        changed = true;
    }
    if let Some(v) = ep.qbt_compat_ban_duration {
        settings.qbt_compat.ban_duration_secs = v;
        changed = true;
    }
    if let Some(v) = ep.qbt_compat_csrf {
        settings.qbt_compat.csrf_protection_enabled = v;
        changed = true;
    }
    if let Some(v) = ep.qbt_compat_host_validation {
        settings.qbt_compat.host_header_validation_enabled = v;
        changed = true;
    }
    if let Some(v) = ep.qbt_compat_reverse_proxy {
        settings.qbt_compat.web_ui_reverse_proxy_enabled = v;
        changed = true;
    }
    // v0.187.3 / 2A: Web UI port + bind, single source of truth under
    // `qbt_compat`. Both are restart-required; the runtime apply will
    // classify them via `apply_settings_classified` and the bridge will
    // post a toast (Step 4.2).
    if let Some(v) = ep.qbt_compat_port {
        settings.qbt_compat.port = v;
        changed = true;
    }
    if let Some(ref v) = ep.qbt_compat_bind_address {
        settings.qbt_compat.bind_address.clone_from(v);
        changed = true;
    }

    // Advanced
    if let Some(v) = ep.hashing_threads {
        settings.hashing_threads = v;
        changed = true;
    }
    if let Some(v) = ep.save_resume_interval_secs {
        settings.save_resume_interval_secs = v;
        changed = true;
    }
    if let Some(v) = ep.enable_utp {
        settings.enable_utp = v;
        changed = true;
    }
    if let Some(v) = ep.enable_fast_extension {
        settings.enable_fast_extension = v;
        changed = true;
    }
    if let Some(v) = ep.enable_holepunch {
        settings.enable_holepunch = v;
        changed = true;
    }
    if let Some(v) = ep.enable_bep40_eviction {
        settings.enable_bep40_eviction = v;
        changed = true;
    }

    // Proxy
    if let Some(ref pt) = ep.proxy_type {
        use irontide::session::ProxyType;
        settings.proxy.proxy_type = match pt.as_str() {
            "SOCKS4" => ProxyType::Socks4,
            "SOCKS5" => ProxyType::Socks5,
            "SOCKS5 (password)" => ProxyType::Socks5Password,
            "HTTP" => ProxyType::Http,
            "HTTP (password)" => ProxyType::HttpPassword,
            _ => ProxyType::None,
        };
        changed = true;
    }
    if let Some(ref h) = ep.proxy_host {
        settings.proxy.hostname.clone_from(h);
        changed = true;
    }
    if let Some(p) = ep.proxy_port {
        settings.proxy.port = p;
        changed = true;
    }
    if let Some(v) = ep.proxy_peer_connections {
        settings.proxy.proxy_peer_connections = v;
        changed = true;
    }
    if let Some(v) = ep.proxy_hostnames {
        settings.proxy.proxy_hostnames = v;
        changed = true;
    }

    if changed {
        // v0.187.3 / Step 4.2: switch to apply_settings_classified so the
        // bridge can drive a "restart required to apply: <fields>" toast
        // when listen_port, dht, lsd, pex, encryption, anonymous_mode, save_path,
        // qbt_compat.port, or qbt_compat.bind_address change.
        match session.apply_settings_classified(settings).await {
            Ok(applied) => {
                if applied.restart_required.is_empty() {
                    show_toast(weak, "Settings applied", false);
                } else {
                    // v0.187.3 / 2A: Web UI port/bind changes are
                    // restart-required; flag them in the same channel.
                    let fields = applied.restart_required.join(", ");
                    show_toast(weak, &format!("Restart required to apply: {fields}"), false);
                }
                confirm_settings_to_gui(session, weak).await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to apply engine prefs");
                show_toast(weak, &format!("Settings apply failed: {e}"), true);
            }
        }
    }
}

async fn confirm_settings_to_gui(
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let Ok(live) = session.settings().await else {
        return;
    };
    let dl_limit = crate::format::format_rate(live.download_rate_limit);
    let ul_limit = crate::format::format_rate(live.upload_rate_limit);
    let alt_dl = crate::format::format_rate(live.alt_download_rate_limit);
    let alt_ul = crate::format::format_rate(live.alt_upload_rate_limit);
    let dl_limit_raw = live.download_rate_limit;
    let ul_limit_raw = live.upload_rate_limit;
    let alt_dl_raw = live.alt_download_rate_limit;
    let alt_ul_raw = live.alt_upload_rate_limit;
    let enable_dht = live.enable_dht;
    let enable_pex = live.enable_pex;
    let enable_lsd = live.enable_lsd;
    let anonymous = live.anonymous_mode;
    let max_peers = live.max_peers_per_torrent;
    let max_conn = live.max_connections_global;
    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_pref_dl_limit_value(if dl_limit_raw == 0 {
            "0".into()
        } else {
            dl_limit.into()
        });
        win.set_pref_ul_limit_value(if ul_limit_raw == 0 {
            "0".into()
        } else {
            ul_limit.into()
        });
        win.set_pref_alt_dl_limit(if alt_dl_raw == 0 {
            "0".into()
        } else {
            alt_dl.into()
        });
        win.set_pref_alt_ul_limit(if alt_ul_raw == 0 {
            "0".into()
        } else {
            alt_ul.into()
        });
        win.set_pref_enable_dht(enable_dht);
        win.set_pref_enable_pex(enable_pex);
        win.set_pref_enable_lsd(enable_lsd);
        win.set_pref_anonymous_mode(anonymous);
        #[allow(clippy::cast_possible_truncation, reason = "peer count fits i32")]
        {
            win.set_pref_max_peers_per_torrent(max_peers.to_string().into());
            win.set_pref_max_connections_global(max_conn.to_string().into());
        }
    });
}

/// Recursively sum file sizes in a directory tree (no external dependency).
fn dir_total_size(dir: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                let ft = entry.file_type();
                if let Ok(ft) = ft {
                    if ft.is_file() {
                        total += entry.metadata().map_or(0, |m| m.len());
                    } else if ft.is_dir() {
                        stack.push(entry.path());
                    }
                }
            }
        }
    }
    total
}

/// M192: Handle "Browse..." for the Create Torrent source (file or folder).
pub fn handle_browse_create_torrent_source(
    weak: &slint::Weak<crate::MainWindow>,
    state: &Arc<Mutex<AppState>>,
) {
    let weak = weak.clone();
    let state = state.clone();
    std::thread::spawn(move || {
        let path = rfd::FileDialog::new()
            .pick_folder()
            .or_else(|| rfd::FileDialog::new().pick_file());
        if let Some(p) = path {
            let path_str = p.to_string_lossy().into_owned();
            let name = p
                .file_name()
                .map_or_else(|| path_str.clone(), |n| n.to_string_lossy().into_owned());
            let total_size = if p.is_dir() {
                dir_total_size(&p)
            } else {
                std::fs::metadata(&p).map_or(0, |m| m.len())
            };
            let size_str = crate::format::format_size(total_size);
            let default_output = p
                .parent()
                .unwrap_or(p.as_path())
                .join(format!("{name}.torrent"))
                .to_string_lossy()
                .into_owned();

            {
                let mut st = state.lock();
                st.create_torrent.source_path.clone_from(&path_str);
                st.create_torrent.source_name.clone_from(&name);
                st.create_torrent.source_size_bytes = total_size;
                st.create_torrent.output_path.clone_from(&default_output);
                st.create_torrent.create_error.clear();
            }

            let _ = weak.upgrade_in_event_loop(move |win| {
                win.set_create_torrent_source_path(path_str.into());
                win.set_create_torrent_source_name(name.into());
                win.set_create_torrent_source_size(size_str.into());
                win.set_create_torrent_output_path(default_output.into());
                win.set_create_torrent_error(slint::SharedString::default());
            });
        }
    });
}

/// M192: Handle "Browse..." for the Create Torrent output file path.
pub fn handle_browse_create_torrent_output(
    weak: &slint::Weak<crate::MainWindow>,
    state: &Arc<Mutex<AppState>>,
) {
    let weak = weak.clone();
    let state = state.clone();
    std::thread::spawn(move || {
        let file = rfd::FileDialog::new()
            .add_filter("Torrent", &["torrent"])
            .set_file_name("output.torrent")
            .save_file();
        if let Some(p) = file {
            let path_str = p.to_string_lossy().into_owned();
            state
                .lock()
                .create_torrent
                .output_path
                .clone_from(&path_str);
            let _ = weak.upgrade_in_event_loop(move |win| {
                win.set_create_torrent_output_path(path_str.into());
            });
        }
    });
}

/// M192: parse tracker text into `(url, tier)` pairs.
///
/// Blank lines separate tiers. Within a tier, each non-empty line is a tracker URL.
fn parse_tracker_tiers(text: &str) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    let mut tier: usize = 0;
    let mut tier_had_url = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if tier_had_url {
                tier += 1;
                tier_had_url = false;
            }
        } else {
            result.push((trimmed.to_owned(), tier));
            tier_had_url = true;
        }
    }
    result
}

/// M192: resolve the piece size label to bytes, or `None` for auto.
fn resolve_piece_size(label: &str, total_size: u64) -> u64 {
    match label {
        "256 KiB" => 256 * 1024,
        "512 KiB" => 512 * 1024,
        "1 MiB" => 1024 * 1024,
        "2 MiB" => 2 * 1024 * 1024,
        "4 MiB" => 4 * 1024 * 1024,
        _ => irontide::core::auto_piece_size(total_size),
    }
}

/// M192: run `CreateTorrent::generate_with_progress` on a background thread.
fn handle_create_torrent(
    ct_state: crate::app::CreateTorrentState,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let weak = weak.clone();
    let weak_progress = weak.clone();
    let weak_done = weak.clone();
    let weak_err = weak.clone();

    // Signal creation started
    let _ = weak.upgrade_in_event_loop(|win| {
        win.set_create_torrent_is_creating(true);
        win.set_create_torrent_progress(0.0);
        win.set_create_torrent_error(slint::SharedString::default());
    });

    std::thread::spawn(move || {
        let source = std::path::Path::new(&ct_state.source_path);
        let mut builder = irontide::core::CreateTorrent::new();

        if source.is_dir() {
            builder = builder.add_directory(source);
        } else {
            builder = builder.add_file(source);
        }

        let piece_size = resolve_piece_size(&ct_state.piece_size_label, ct_state.source_size_bytes);
        builder = builder.set_piece_size(piece_size);

        let version = match ct_state.format {
            crate::app::CreateTorrentFormat::V1 => irontide::core::TorrentVersion::V1Only,
            crate::app::CreateTorrentFormat::V2 => irontide::core::TorrentVersion::V2Only,
            crate::app::CreateTorrentFormat::Hybrid => irontide::core::TorrentVersion::Hybrid,
        };
        builder = builder.set_version(version);

        if ct_state.is_private {
            builder = builder.set_private(true);
        }

        if !ct_state.comment.is_empty() {
            builder = builder.set_comment(ct_state.comment);
        }

        if !ct_state.source_tag.is_empty() {
            builder = builder.set_source(ct_state.source_tag);
        }

        builder = builder.set_creator("IronTide".to_owned());

        for (url, tier) in parse_tracker_tiers(&ct_state.tracker_text) {
            builder = builder.add_tracker(url, tier);
        }

        for line in ct_state.web_seed_text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                builder = builder.add_web_seed(trimmed.to_owned());
            }
        }

        let result = builder.generate_with_progress(|current, total| {
            if total > 0 {
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "M192: piece count ratio — precision is ample for a progress bar"
                )]
                let progress = current as f32 / total as f32;
                let weak_p = weak_progress.clone();
                let _ = weak_p.upgrade_in_event_loop(move |win| {
                    win.set_create_torrent_progress(progress);
                });
            }
        });

        match result {
            Ok(create_result) => {
                if let Err(e) = std::fs::write(&ct_state.output_path, &create_result.bytes) {
                    let msg = format!("Failed to write .torrent file: {e}");
                    let _ = weak_err.upgrade_in_event_loop(move |win| {
                        win.set_create_torrent_is_creating(false);
                        win.set_create_torrent_error(msg.into());
                    });
                    return;
                }
                let output = ct_state.output_path.clone();
                let _ = weak_done.upgrade_in_event_loop(move |win| {
                    win.set_create_torrent_is_creating(false);
                    win.set_create_torrent_progress(1.0);
                    win.set_create_torrent_error(slint::SharedString::default());
                    win.set_show_create_torrent_dialog(false);
                });
                show_toast(&weak, &format!("Created {output}"), false);
            }
            Err(e) => {
                let msg = format!("Torrent creation failed: {e}");
                let _ = weak_err.upgrade_in_event_loop(move |win| {
                    win.set_create_torrent_is_creating(false);
                    win.set_create_torrent_error(msg.into());
                });
            }
        }
    });
}

async fn handle_pause_all(
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let Ok(summaries) = session.list_torrent_summaries().await else {
        return;
    };
    let mut paused = 0u32;
    for s in &summaries {
        if s.state != irontide::session::TorrentState::Paused
            && let Ok(h) = irontide::core::Id20::from_hex(&s.info_hash)
        {
            let _ = session.pause_torrent(h).await;
            paused += 1;
        }
    }
    let msg = format!("Paused {paused} torrent(s)");
    show_toast(weak, &msg, false);
}

async fn handle_resume_all(
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let Ok(summaries) = session.list_torrent_summaries().await else {
        return;
    };
    let mut resumed = 0u32;
    for s in &summaries {
        if (s.state == irontide::session::TorrentState::Paused
            || s.state == irontide::session::TorrentState::Queued)
            && let Ok(h) = irontide::core::Id20::from_hex(&s.info_hash)
        {
            let _ = session.resume_torrent(h).await;
            resumed += 1;
        }
    }
    let msg = format!("Resumed {resumed} torrent(s)");
    show_toast(weak, &msg, false);
}

async fn handle_search_query(
    query: &str,
    plugin_name: Option<&str>,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let plugins = crate::search::load_plugins();
    let active: Vec<&crate::search::SearchPlugin> = plugins
        .iter()
        .filter(|p| p.enabled)
        .filter(|p| plugin_name.is_none_or(|n| p.name == n))
        .collect();

    if active.is_empty() {
        show_toast(weak, "No search plugins configured", true);
        push_search_results(weak, &[]);
        return;
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    let mut all_results = Vec::new();

    for plugin in &active {
        let url = crate::search::build_search_url(plugin, query);
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.text().await {
                    let crate::search::ResultFormat::Json {
                        ref results_path,
                        ref fields,
                    } = plugin.result_format;
                    let results = crate::search::parse_json_results(
                        &body,
                        results_path,
                        fields,
                        &plugin.name,
                    );
                    all_results.extend(results);
                }
            }
            Ok(resp) => {
                tracing::warn!(
                    plugin = %plugin.name,
                    status = %resp.status(),
                    "search plugin returned error"
                );
            }
            Err(e) => {
                tracing::warn!(plugin = %plugin.name, error = %e, "search plugin request failed");
            }
        }
    }

    all_results.sort_by_key(|r| std::cmp::Reverse(r.seeds));
    push_search_results(weak, &all_results);

    if all_results.is_empty() {
        show_toast(weak, "No results found", false);
    } else {
        let msg = format!("Found {} result(s)", all_results.len());
        show_toast(weak, &msg, false);
    }
}

fn push_search_results(
    weak: &slint::Weak<crate::MainWindow>,
    results: &[crate::search::SearchResult],
) {
    let count = i32::try_from(results.len()).unwrap_or(0);
    let rows: Vec<crate::SearchResultRow> = results
        .iter()
        .map(|r| crate::SearchResultRow {
            name: r.name.clone().into(),
            magnet_url: r.magnet_url.clone().into(),
            size: r.size.clone().into(),
            seeds: r.seeds,
            leechers: r.leechers,
            source: r.source.clone().into(),
        })
        .collect();
    let _ = weak.upgrade_in_event_loop(move |win| {
        let model = std::rc::Rc::new(slint::VecModel::from(rows));
        win.set_search_results(slint::ModelRc::from(model));
        win.set_search_result_count(count);
    });
}

async fn handle_open_torrent_file(
    path: std::path::PathBuf,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let display = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );

    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(e) => {
            let msg = format!("Failed to read {display}: {e}");
            show_toast(weak, &msg, true);
            return;
        }
    };

    match irontide::AddTorrentParams::from_bytes(bytes)
        .add_to(session)
        .await
    {
        Ok(_) => {
            let msg = format!("Added: {display}");
            show_toast(weak, &msg, false);
        }
        Err(e) => {
            let msg = format!("Failed to add {display}: {e}");
            show_toast(weak, &msg, true);
        }
    }
}

async fn handle_open_magnet(
    uri: &str,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let magnet = match irontide::core::Magnet::parse(uri) {
        Ok(m) => m,
        Err(e) => {
            let msg = format!("Invalid magnet URI: {e}");
            show_toast(weak, &msg, true);
            return;
        }
    };
    let name = magnet
        .display_name
        .clone()
        .unwrap_or_else(|| "magnet".to_owned());
    match irontide::AddTorrentParams::from_magnet(magnet)
        .add_to(session)
        .await
    {
        Ok(_) => {
            let msg = format!("Added: {name}");
            show_toast(weak, &msg, false);
        }
        Err(e) => {
            let msg = format!("Failed to add {name}: {e}");
            show_toast(weak, &msg, true);
        }
    }
}

// ── RSS handlers (M197) ────────────────────────────────────────────────────

async fn handle_rss_add_feed(url: &str, weak: &slint::Weak<crate::MainWindow>) {
    let url = url.trim().to_owned();
    if url.is_empty() {
        return;
    }

    let _ = weak.upgrade_in_event_loop({
        let url2 = url.clone();
        move |win| {
            win.set_rss_refreshing(true);
            let _ = url2;
        }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    let body = match client.get(&url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(text) => text,
            Err(e) => {
                show_toast(weak, &format!("Failed to read feed: {e}"), true);
                let _ = weak.upgrade_in_event_loop(|win| win.set_rss_refreshing(false));
                return;
            }
        },
        Err(e) => {
            show_toast(weak, &format!("Failed to fetch feed: {e}"), true);
            let _ = weak.upgrade_in_event_loop(|win| win.set_rss_refreshing(false));
            return;
        }
    };

    let title = crate::rss::extract_feed_title(&body);
    let new_items = crate::rss::parse_rss_feed(&body, &url);

    let mut state = crate::rss::load_state();
    if state.feeds.iter().any(|f| f.url == url) {
        show_toast(weak, "Feed already exists", false);
        let _ = weak.upgrade_in_event_loop(|win| win.set_rss_refreshing(false));
        return;
    }

    state.feeds.push(crate::rss::RssFeed {
        url: url.clone(),
        title,
        enabled: true,
        alias: None,
        last_refresh: Some(chrono::Utc::now().timestamp()),
        error: None,
    });

    let item_count = new_items.len();
    for item in new_items {
        if !state
            .items
            .iter()
            .any(|existing| existing.title == item.title && existing.feed_url == item.feed_url)
        {
            state.items.push(item);
        }
    }

    if let Err(e) = crate::rss::save_state(&state) {
        tracing::warn!("failed to save RSS state: {e}");
    }

    push_rss_state(weak, &state, -1);
    show_toast(weak, &format!("Added feed with {item_count} items"), false);
    let _ = weak.upgrade_in_event_loop(|win| win.set_rss_refreshing(false));
}

fn handle_rss_remove_feed(index: usize, weak: &slint::Weak<crate::MainWindow>) {
    let mut state = crate::rss::load_state();
    if index >= state.feeds.len() {
        return;
    }
    let feed_url = state.feeds[index].url.clone();
    state.feeds.remove(index);
    state.items.retain(|item| item.feed_url != feed_url);
    if let Err(e) = crate::rss::save_state(&state) {
        tracing::warn!("failed to save RSS state: {e}");
    }
    push_rss_state(weak, &state, -1);
}

async fn handle_rss_refresh_feeds(weak: &slint::Weak<crate::MainWindow>) {
    let _ = weak.upgrade_in_event_loop(|win| win.set_rss_refreshing(true));

    let mut state = crate::rss::load_state();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    let mut new_count = 0usize;
    for feed in &mut state.feeds {
        if !feed.enabled {
            continue;
        }
        match client.get(&feed.url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(body) => {
                    let items = crate::rss::parse_rss_feed(&body, &feed.url);
                    feed.last_refresh = Some(chrono::Utc::now().timestamp());
                    feed.error = None;
                    for item in items {
                        if !state.items.iter().any(|existing| {
                            existing.title == item.title && existing.feed_url == item.feed_url
                        }) {
                            new_count += 1;
                            state.items.push(item);
                        }
                    }
                }
                Err(e) => {
                    feed.error = Some(e.to_string());
                }
            },
            Err(e) => {
                feed.error = Some(e.to_string());
            }
        }
    }

    if let Err(e) = crate::rss::save_state(&state) {
        tracing::warn!("failed to save RSS state: {e}");
    }

    push_rss_state(weak, &state, -1);
    let msg = if new_count > 0 {
        format!("{new_count} new item(s)")
    } else {
        "Feeds up to date".to_owned()
    };
    show_toast(weak, &msg, false);
    let _ = weak.upgrade_in_event_loop(|win| win.set_rss_refreshing(false));
}

fn handle_rss_feed_selected(index: i32, weak: &slint::Weak<crate::MainWindow>) {
    let state = crate::rss::load_state();
    push_rss_state(weak, &state, index);
    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_rss_selected_feed_index(index);
    });
}

async fn handle_rss_download_item(
    index: usize,
    selected_idx: i32,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let mut state = crate::rss::load_state();

    let visible_items: Vec<usize> = state
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            if selected_idx < 0 {
                return true;
            }
            let feed_idx = usize::try_from(selected_idx).unwrap_or(0);
            state
                .feeds
                .get(feed_idx)
                .is_some_and(|f| f.url == item.feed_url)
        })
        .map(|(i, _)| i)
        .collect();

    let Some(&real_idx) = visible_items.get(index) else {
        return;
    };

    let item = &state.items[real_idx];
    let Some(url) = item.best_download_url().map(String::from) else {
        show_toast(weak, "No download URL for this item", true);
        return;
    };

    if url.starts_with("magnet:") {
        handle_open_magnet(&url, session, weak).await;
    } else {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        match client.get(&url).send().await {
            Ok(resp) => match resp.bytes().await {
                Ok(bytes) => {
                    let params = irontide::AddTorrentParams::from_bytes(bytes.to_vec());
                    match params.add_to(session).await {
                        Ok(_) => {
                            show_toast(weak, "Added torrent from RSS", false);
                        }
                        Err(e) => {
                            show_toast(weak, &format!("Failed to add: {e}"), true);
                        }
                    }
                }
                Err(e) => {
                    show_toast(weak, &format!("Failed to download torrent: {e}"), true);
                    return;
                }
            },
            Err(e) => {
                show_toast(weak, &format!("Failed to fetch torrent: {e}"), true);
                return;
            }
        }
    }

    state.items[real_idx].downloaded = true;
    state.items[real_idx].read = true;
    if let Err(e) = crate::rss::save_state(&state) {
        tracing::warn!("failed to save RSS state: {e}");
    }
    push_rss_state(weak, &state, selected_idx);
}

fn handle_rss_mark_item_read(
    index: usize,
    selected_idx: i32,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let mut state = crate::rss::load_state();

    let visible_items: Vec<usize> = state
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            if selected_idx < 0 {
                return true;
            }
            let feed_idx = usize::try_from(selected_idx).unwrap_or(0);
            state
                .feeds
                .get(feed_idx)
                .is_some_and(|f| f.url == item.feed_url)
        })
        .map(|(i, _)| i)
        .collect();

    if let Some(&real_idx) = visible_items.get(index) {
        state.items[real_idx].read = true;
        if let Err(e) = crate::rss::save_state(&state) {
            tracing::warn!("failed to save RSS state: {e}");
        }
        push_rss_state(weak, &state, selected_idx);
    }
}

fn push_rss_state(
    weak: &slint::Weak<crate::MainWindow>,
    state: &crate::rss::RssState,
    selected_feed_index: i32,
) {
    let feed_rows: Vec<crate::RssFeedRow> = state
        .feeds
        .iter()
        .map(|f| {
            let item_count = state.items.iter().filter(|i| i.feed_url == f.url).count();
            crate::RssFeedRow {
                url: f.url.clone().into(),
                title: f.alias.as_deref().unwrap_or(&f.title).into(),
                enabled: f.enabled,
                item_count: i32::try_from(item_count).unwrap_or(0),
                error: f.error.as_deref().unwrap_or("").into(),
            }
        })
        .collect();

    let item_rows: Vec<crate::RssItemRow> = state
        .items
        .iter()
        .filter(|item| {
            if selected_feed_index < 0 {
                return true;
            }
            let idx = usize::try_from(selected_feed_index).unwrap_or(0);
            state.feeds.get(idx).is_some_and(|f| f.url == item.feed_url)
        })
        .map(|item| {
            let feed_title = state
                .feeds
                .iter()
                .find(|f| f.url == item.feed_url)
                .map_or("", |f| f.alias.as_deref().unwrap_or(&f.title));
            crate::RssItemRow {
                title: item.display_title().into(),
                pub_date: item.pub_date.as_deref().unwrap_or("").into(),
                size: item.size.as_deref().unwrap_or("").into(),
                feed_title: feed_title.into(),
                has_download: item.best_download_url().is_some(),
                downloaded: item.downloaded,
                read: item.read,
            }
        })
        .collect();

    let rule_rows: Vec<crate::RssRuleRow> = state
        .rules
        .iter()
        .map(|r| crate::RssRuleRow {
            name: r.name.clone().into(),
            enabled: r.enabled,
            must_contain: r.must_contain.clone().into(),
            must_not_contain: r.must_not_contain.clone().into(),
        })
        .collect();

    let _ = weak.upgrade_in_event_loop(move |win| {
        let feed_model = std::rc::Rc::new(slint::VecModel::from(feed_rows));
        win.set_rss_feeds(slint::ModelRc::from(feed_model));
        let item_model = std::rc::Rc::new(slint::VecModel::from(item_rows));
        win.set_rss_items(slint::ModelRc::from(item_model));
        let rule_model = std::rc::Rc::new(slint::VecModel::from(rule_rows));
        win.set_rss_rules(slint::ModelRc::from(rule_model));
    });
}

// ── Bandwidth Scheduler handlers (M198) ─────────────────────────────────────

fn handle_scheduler_toggle_enabled(weak: &slint::Weak<crate::MainWindow>) {
    let mut schedule = crate::scheduler::load_schedule();
    schedule.enabled = !schedule.enabled;
    if let Err(e) = crate::scheduler::save_schedule(&schedule) {
        tracing::warn!("failed to save schedule: {e}");
    }
    push_scheduler_state(weak);
}

fn handle_scheduler_cell_clicked(day: usize, hour: usize, weak: &slint::Weak<crate::MainWindow>) {
    let mut schedule = crate::scheduler::load_schedule();
    schedule.toggle_cell(day, hour);
    if let Err(e) = crate::scheduler::save_schedule(&schedule) {
        tracing::warn!("failed to save schedule: {e}");
    }
    push_scheduler_state(weak);
}

fn handle_scheduler_apply_preset(name: &str, weak: &slint::Weak<crate::MainWindow>) {
    let schedule = match name {
        "always_on" => crate::scheduler::BandwidthSchedule::preset_always_on(),
        "night_only" => crate::scheduler::BandwidthSchedule::preset_night_only(),
        "work_limited" => crate::scheduler::BandwidthSchedule::preset_work_hours_limited(),
        _ => return,
    };
    if let Err(e) = crate::scheduler::save_schedule(&schedule) {
        tracing::warn!("failed to save schedule: {e}");
    }
    push_scheduler_state(weak);
}

fn handle_scheduler_limited_rate_changed(rate_kib: u32, weak: &slint::Weak<crate::MainWindow>) {
    let mut schedule = crate::scheduler::load_schedule();
    schedule.limited_rate_kib = rate_kib.max(1);
    if let Err(e) = crate::scheduler::save_schedule(&schedule) {
        tracing::warn!("failed to save schedule: {e}");
    }
    push_scheduler_state(weak);
}

// ── IP Filter handlers (M199) ───────────────────────────────────────────────

fn handle_ip_filter_add_rule(label: &str, range: &str, weak: &slint::Weak<crate::MainWindow>) {
    if let Some((first, last)) = crate::ip_filter_page::parse_ip_range(range) {
        let mut state = crate::ip_filter_page::load_state();
        state.rules.push(crate::ip_filter_page::ManualRule {
            label: if label.is_empty() {
                range.to_string()
            } else {
                label.to_string()
            },
            first: first.to_string(),
            last: last.to_string(),
            enabled: true,
        });
        if let Err(e) = crate::ip_filter_page::save_state(&state) {
            tracing::warn!("failed to save IP filter state: {e}");
        }
        push_ip_filter_state(weak);
    }
}

fn handle_ip_filter_remove_rule(index: usize, weak: &slint::Weak<crate::MainWindow>) {
    let mut state = crate::ip_filter_page::load_state();
    if index < state.rules.len() {
        state.rules.remove(index);
        if let Err(e) = crate::ip_filter_page::save_state(&state) {
            tracing::warn!("failed to save IP filter state: {e}");
        }
    }
    push_ip_filter_state(weak);
}

fn handle_ip_filter_toggle_rule(index: usize, weak: &slint::Weak<crate::MainWindow>) {
    let mut state = crate::ip_filter_page::load_state();
    if let Some(rule) = state.rules.get_mut(index) {
        rule.enabled = !rule.enabled;
        if let Err(e) = crate::ip_filter_page::save_state(&state) {
            tracing::warn!("failed to save IP filter state: {e}");
        }
    }
    push_ip_filter_state(weak);
}

async fn handle_ip_filter_unban_peer(
    ip: &str,
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    if let Ok(addr) = ip.parse::<std::net::IpAddr>()
        && let Err(e) = session.unban_peer(addr).await
    {
        tracing::warn!("failed to unban {ip}: {e}");
    }
    push_ip_filter_state_with_session(weak, session).await;
}

fn handle_ip_filter_import_file(weak: &slint::Weak<crate::MainWindow>) {
    let dialog = rfd::FileDialog::new()
        .add_filter("IP filter files", &["p2p", "dat", "txt"])
        .set_title("Import IP filter file");
    if let Some(path) = dialog.pick_file() {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let result = if path.extension().is_some_and(|e| e == "dat") {
                    irontide::session::parse_dat(&content)
                } else {
                    irontide::session::parse_p2p(&content)
                };
                match result {
                    Ok(filter) => {
                        let count = filter.num_ranges();
                        let mut state = crate::ip_filter_page::load_state();
                        let filename = path.file_name().map_or_else(
                            || "imported".to_string(),
                            |n| n.to_string_lossy().to_string(),
                        );
                        if !state.imported_files.contains(&filename) {
                            state.imported_files.push(filename);
                        }
                        if let Err(e) = crate::ip_filter_page::save_state(&state) {
                            tracing::warn!("failed to save IP filter state: {e}");
                        }
                        tracing::info!("imported {count} IP filter ranges from {}", path.display());
                    }
                    Err(e) => {
                        tracing::warn!("failed to parse IP filter file {}: {e}", path.display());
                    }
                }
            }
            Err(e) => {
                tracing::warn!("failed to read IP filter file {}: {e}", path.display());
            }
        }
    }
    push_ip_filter_state(weak);
}

async fn handle_ip_filter_toggle_enabled(
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    if let Ok(mut settings) = session.settings().await {
        settings.ip_filter_enabled = !settings.ip_filter_enabled;
        if let Err(e) = session.apply_settings(settings).await {
            tracing::warn!("failed to toggle IP filter: {e}");
        }
    }
    push_ip_filter_state_with_session(weak, session).await;
}

pub fn push_ip_filter_state(weak: &slint::Weak<crate::MainWindow>) {
    let state = crate::ip_filter_page::load_state();
    let rule_rows: Vec<crate::IpFilterRuleRow> = state
        .rules
        .iter()
        .map(|r| crate::IpFilterRuleRow {
            label: r.label.clone().into(),
            first: r.first.clone().into(),
            last: r.last.clone().into(),
            enabled: r.enabled,
        })
        .collect();

    let _ = weak.upgrade_in_event_loop(move |win| {
        let model = std::rc::Rc::new(slint::VecModel::from(rule_rows));
        win.set_ip_filter_rules(slint::ModelRc::from(model));
    });
}

async fn push_ip_filter_state_with_session(
    weak: &slint::Weak<crate::MainWindow>,
    session: &irontide::session::SessionHandle,
) {
    let state = crate::ip_filter_page::load_state();
    let rule_rows: Vec<crate::IpFilterRuleRow> = state
        .rules
        .iter()
        .map(|r| crate::IpFilterRuleRow {
            label: r.label.clone().into(),
            first: r.first.clone().into(),
            last: r.last.clone().into(),
            enabled: r.enabled,
        })
        .collect();

    let banned_rows: Vec<crate::BannedPeerRow> = session
        .banned_peers()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|ip| crate::BannedPeerRow {
            ip: ip.to_string().into(),
        })
        .collect();

    let filter_enabled = session.settings().await.is_ok_and(|s| s.ip_filter_enabled);

    let _ = weak.upgrade_in_event_loop(move |win| {
        let rule_model = std::rc::Rc::new(slint::VecModel::from(rule_rows));
        win.set_ip_filter_rules(slint::ModelRc::from(rule_model));
        let banned_model = std::rc::Rc::new(slint::VecModel::from(banned_rows));
        win.set_ip_filter_banned_peers(slint::ModelRc::from(banned_model));
        win.set_ip_filter_enabled(filter_enabled);
    });
}

pub fn push_scheduler_state(weak: &slint::Weak<crate::MainWindow>) {
    let schedule = crate::scheduler::load_schedule();
    let flat = schedule.to_flat_grid();
    let enabled = schedule.enabled;
    let limited_rate = i32::try_from(schedule.limited_rate_kib).unwrap_or(512);

    let cells: Vec<crate::SchedulerCell> = flat
        .into_iter()
        .map(|s| crate::SchedulerCell {
            state: i32::from(s),
        })
        .collect();

    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_scheduler_enabled(enabled);
        win.set_scheduler_limited_rate_kib(limited_rate);
        let model = std::rc::Rc::new(slint::VecModel::from(cells));
        win.set_scheduler_cells(slint::ModelRc::from(model));
    });
}

// ── Bandwidth Intent (M203) ─────────────────────────────────────────────────

fn handle_intent_set_mode(mode: u8, weak: &slint::Weak<crate::MainWindow>) {
    let mut intent = crate::bandwidth_intent::BandwidthIntent::load();
    intent.mode = match mode {
        1 => crate::bandwidth_intent::IntentMode::ManualLimits,
        2 => crate::bandwidth_intent::IntentMode::LeaveReserve,
        _ => crate::bandwidth_intent::IntentMode::Unlimited,
    };
    let _ = intent.save();
    push_bandwidth_intent_state(weak);
}

fn handle_intent_apply_preset(index: usize, weak: &slint::Weak<crate::MainWindow>) {
    let mut intent = crate::bandwidth_intent::BandwidthIntent::load();
    if let Some(preset) = crate::bandwidth_intent::PRESETS.get(index) {
        intent.apply_preset(preset);
        let _ = intent.save();
    }
    push_bandwidth_intent_state(weak);
}

fn handle_intent_set_detected_speeds(
    dl_kbps: u64,
    ul_kbps: u64,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let mut intent = crate::bandwidth_intent::BandwidthIntent::load();
    intent.detected_download_kbps = dl_kbps;
    intent.detected_upload_kbps = ul_kbps;
    let _ = intent.save();
    push_bandwidth_intent_state(weak);
}

pub fn push_bandwidth_intent_state(weak: &slint::Weak<crate::MainWindow>) {
    let intent = crate::bandwidth_intent::BandwidthIntent::load();
    let mode = match intent.mode {
        crate::bandwidth_intent::IntentMode::Unlimited => 0,
        crate::bandwidth_intent::IntentMode::ManualLimits => 1,
        crate::bandwidth_intent::IntentMode::LeaveReserve => 2,
    };
    let detected_dl = intent.detected_download_kbps.to_string();
    let detected_ul = intent.detected_upload_kbps.to_string();
    let reserved_dl = crate::bandwidth_intent::format_speed_kbps(intent.reserved_download_kbps);
    let reserved_ul = crate::bandwidth_intent::format_speed_kbps(intent.reserved_upload_kbps);
    let limits = intent.effective_limits();
    let effective_dl = crate::bandwidth_intent::format_speed_bytes(limits.download_bytes_per_sec);
    let effective_ul = crate::bandwidth_intent::format_speed_bytes(limits.upload_bytes_per_sec);

    let presets: Vec<crate::IntentPresetRow> = crate::bandwidth_intent::PRESETS
        .iter()
        .map(|p| crate::IntentPresetRow {
            name: p.name.into(),
            description: p.description.into(),
            reserve_dl: crate::bandwidth_intent::format_speed_kbps(p.reserve_download_kbps).into(),
            reserve_ul: crate::bandwidth_intent::format_speed_kbps(p.reserve_upload_kbps).into(),
        })
        .collect();

    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_intent_mode(mode);
        win.set_intent_detected_dl_text(detected_dl.into());
        win.set_intent_detected_ul_text(detected_ul.into());
        win.set_intent_reserved_dl_text(reserved_dl.into());
        win.set_intent_reserved_ul_text(reserved_ul.into());
        win.set_intent_effective_dl_text(effective_dl.into());
        win.set_intent_effective_ul_text(effective_ul.into());
        let model = std::rc::Rc::new(slint::VecModel::from(presets));
        win.set_intent_presets(slint::ModelRc::from(model));
    });
}

// ── Phone Pair QR (M204) ───────────────────────────────────────────────────

pub fn push_phone_pair_state(weak: &slint::Weak<crate::MainWindow>) {
    let weak = weak.clone();
    let _ = weak.upgrade_in_event_loop(move |win| {
        let port_str = win.get_phone_pair_port_text();
        let port: u16 = port_str.to_string().parse().unwrap_or(9080);
        let info = crate::phone_pair::generate_pair_info(port);
        let status = if info.local_ip == "127.0.0.1" {
            "No network detected \u{2014} QR will only work locally"
        } else {
            "Ready \u{2014} scan from your phone"
        };
        win.set_phone_pair_qr_image(info.qr_image);
        win.set_phone_pair_url_text(info.url.into());
        win.set_phone_pair_ip_text(info.local_ip.into());
        win.set_phone_pair_port_text(info.port.to_string().into());
        win.set_phone_pair_status_text(status.into());
    });
}

// ── Logs + Statistics (M200) ────────────────────────────────────────────────

static LOG_BUFFER: std::sync::LazyLock<crate::logs_stats_page::LogBuffer> =
    std::sync::LazyLock::new(crate::logs_stats_page::LogBuffer::new);

#[allow(dead_code)]
pub fn log_buffer() -> &'static crate::logs_stats_page::LogBuffer {
    &LOG_BUFFER
}

fn handle_logs_tab_changed(tab: i32, weak: &slint::Weak<crate::MainWindow>) {
    let weak = weak.clone();
    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_logs_active_tab(tab);
    });
}

fn handle_logs_clear(weak: &slint::Weak<crate::MainWindow>) {
    LOG_BUFFER.clear();
    let weak = weak.clone();
    let _ = weak.upgrade_in_event_loop(move |win| {
        let model = std::rc::Rc::new(slint::VecModel::<crate::LogRow>::default());
        win.set_log_entries(slint::ModelRc::from(model));
        win.set_log_count(0);
    });
}

fn handle_logs_set_filter(level: i32, weak: &slint::Weak<crate::MainWindow>) {
    let weak = weak.clone();
    let min_level = match level {
        1 => crate::logs_stats_page::LogLevel::Warning,
        2 => crate::logs_stats_page::LogLevel::Error,
        _ => crate::logs_stats_page::LogLevel::Info,
    };
    let entries = LOG_BUFFER.snapshot_filtered(min_level);
    let rows: Vec<crate::LogRow> = entries
        .iter()
        .map(|e| crate::LogRow {
            timestamp: e.format_timestamp().into(),
            level: i32::from(e.level.as_u8()),
            level_label: e.level.label().into(),
            category: e.category.clone().into(),
            message: e.message.clone().into(),
        })
        .collect();
    let count = i32::try_from(rows.len()).unwrap_or(0);
    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_log_filter_level(level);
        let model = std::rc::Rc::new(slint::VecModel::from(rows));
        win.set_log_entries(slint::ModelRc::from(model));
        win.set_log_count(count);
    });
}

pub fn push_logs_stats_state(weak: &slint::Weak<crate::MainWindow>) {
    let entries = LOG_BUFFER.snapshot();
    let rows: Vec<crate::LogRow> = entries
        .iter()
        .map(|e| crate::LogRow {
            timestamp: e.format_timestamp().into(),
            level: i32::from(e.level.as_u8()),
            level_label: e.level.label().into(),
            category: e.category.clone().into(),
            message: e.message.clone().into(),
        })
        .collect();
    let count = i32::try_from(rows.len()).unwrap_or(0);
    let weak = weak.clone();
    let _ = weak.upgrade_in_event_loop(move |win| {
        let model = std::rc::Rc::new(slint::VecModel::from(rows));
        win.set_log_entries(slint::ModelRc::from(model));
        win.set_log_count(count);
        win.set_log_filter_level(0);
    });
}

#[allow(dead_code)]
pub async fn push_logs_stats_state_with_session(
    session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    let entries = LOG_BUFFER.snapshot();
    let rows: Vec<crate::LogRow> = entries
        .iter()
        .map(|e| crate::LogRow {
            timestamp: e.format_timestamp().into(),
            level: i32::from(e.level.as_u8()),
            level_label: e.level.label().into(),
            category: e.category.clone().into(),
            message: e.message.clone().into(),
        })
        .collect();
    let log_count = i32::try_from(rows.len()).unwrap_or(0);

    let stats = session.session_stats().await.ok();
    let torrents = session.list_torrents().await.unwrap_or_default();
    let total_torrents = torrents.len();
    let (active, total_dl_rate, total_ul_rate, total_peers) = {
        let mut active = 0usize;
        let mut dl = 0u64;
        let mut ul = 0u64;
        let mut peers = 0usize;
        for id in &torrents {
            if let Ok(s) = session.torrent_stats(*id).await {
                if s.download_rate > 0 || s.upload_rate > 0 {
                    active += 1;
                }
                dl += s.download_rate;
                ul += s.upload_rate;
                peers += s.num_peers;
            }
        }
        (active, dl, ul, peers)
    };

    let total_downloaded = stats.as_ref().map_or(0, |s| s.total_downloaded);
    let total_uploaded = stats.as_ref().map_or(0, |s| s.total_uploaded);
    let dht_nodes = stats.as_ref().map_or(0, |s| s.dht_nodes);

    let uptime_secs = session.counters().uptime_secs();

    let cards =
        crate::logs_stats_page::build_stat_cards(&crate::logs_stats_page::SessionSnapshot {
            total_torrents,
            active_torrents: active,
            dl_rate: total_dl_rate,
            ul_rate: total_ul_rate,
            total_downloaded,
            total_uploaded,
            dht_nodes,
            total_peers,
            uptime_secs,
        });

    let card_rows: Vec<crate::StatCardData> = cards
        .into_iter()
        .map(|c| crate::StatCardData {
            label: c.label.into(),
            value: c.value.into(),
        })
        .collect();

    let weak = weak.clone();
    let _ = weak.upgrade_in_event_loop(move |win| {
        let log_model = std::rc::Rc::new(slint::VecModel::from(rows));
        win.set_log_entries(slint::ModelRc::from(log_model));
        win.set_log_count(log_count);
        let card_model = std::rc::Rc::new(slint::VecModel::from(card_rows));
        win.set_stat_cards(slint::ModelRc::from(card_model));
    });
}

// ── M202: category suggestion ─────────────────────────────────────────

pub fn suggest_category(
    name: &str,
    file_extensions: &[String],
    trackers: &[String],
) -> Option<String> {
    let model = crate::category_suggest::ClassifierModel::load();
    model
        .suggest(name, file_extensions, trackers)
        .map(|r| r.category)
}

fn handle_category_suggest_train(
    category: &str,
    name: &str,
    file_extensions: &[String],
    trackers: &[String],
) {
    let mut model = crate::category_suggest::ClassifierModel::load();
    model.train(category, name, file_extensions, trackers);
    if let Err(e) = model.save() {
        tracing::warn!("failed to save category classifier: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_oneshot_round_trip() {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        assert!(tx.send(()).is_ok());
        // Use a temp runtime to receive.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(rx);
        assert!(result.is_ok());
    }

    #[test]
    fn command_channel_roundtrip() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<GuiCommand>();
        tx.send(GuiCommand::PauseTorrents {
            hashes: vec!["abc123".into()],
        })
        .expect("send should succeed");

        let cmd = rx.try_recv().expect("should receive command");
        match cmd {
            GuiCommand::PauseTorrents { hashes } => {
                assert_eq!(hashes, vec!["abc123".to_owned()]);
            }
            _ => panic!("expected PauseTorrents variant"),
        }
    }

    #[test]
    fn batch_toast_format_success() {
        let msg = format_batch_result("Paused", 3, 0);
        assert_eq!(msg, "Paused 3 torrent(s)");
    }

    #[test]
    fn batch_toast_format_partial() {
        let msg = format_batch_result("Paused", 2, 1);
        assert_eq!(msg, "Paused 2 torrent(s), 1 failed");
    }

    #[test]
    fn batch_toast_format_all_failed() {
        let msg = format_batch_result("Removed", 0, 3);
        assert_eq!(msg, "Removed 0 torrent(s), 3 failed");
    }

    #[test]
    fn batch_toast_format_empty_label() {
        let msg = format_batch_result("", 1, 0);
        assert_eq!(msg, " 1 torrent(s)");
    }

    #[test]
    fn valid_magnet_uri_parses() {
        let uri = "magnet:?xt=urn:btih:aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d&dn=test";
        let magnet = irontide::core::Magnet::parse(uri).expect("should parse valid magnet URI");
        assert_eq!(magnet.display_name.as_deref(), Some("test"));
    }

    #[test]
    fn invalid_magnet_uri_fails() {
        let uri = "not a magnet";
        assert!(irontide::core::Magnet::parse(uri).is_err());
    }

    #[test]
    fn add_magnet_gui_command_round_trip() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<GuiCommand>();
        tx.send(GuiCommand::AddMagnet {
            uri: "magnet:?xt=urn:btih:aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into(),
            download_dir: Some("/tmp/downloads".into()),
        })
        .expect("send should succeed");

        let cmd = rx.try_recv().expect("should receive command");
        match cmd {
            GuiCommand::AddMagnet { uri, download_dir } => {
                assert!(uri.starts_with("magnet:"));
                assert_eq!(download_dir, Some("/tmp/downloads".to_owned()));
            }
            _ => panic!("expected AddMagnet variant"),
        }
    }

    #[test]
    fn add_magnet_gui_command_no_dir() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<GuiCommand>();
        tx.send(GuiCommand::AddMagnet {
            uri: "magnet:?xt=urn:btih:aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into(),
            download_dir: None,
        })
        .expect("send should succeed");

        let cmd = rx.try_recv().expect("should receive command");
        match cmd {
            GuiCommand::AddMagnet { download_dir, .. } => {
                assert!(download_dir.is_none());
            }
            _ => panic!("expected AddMagnet variant"),
        }
    }

    #[test]
    fn add_torrent_file_gui_command_round_trip() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<GuiCommand>();
        tx.send(GuiCommand::AddTorrentFile {
            path: "/tmp/test.torrent".into(),
            download_dir: Some("/tmp/downloads".into()),
        })
        .expect("send should succeed");

        let cmd = rx.try_recv().expect("should receive command");
        match cmd {
            GuiCommand::AddTorrentFile { path, download_dir } => {
                assert_eq!(path, "/tmp/test.torrent");
                assert_eq!(download_dir, Some("/tmp/downloads".to_owned()));
            }
            _ => panic!("expected AddTorrentFile variant"),
        }
    }

    #[test]
    fn add_torrent_file_gui_command_no_dir() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<GuiCommand>();
        tx.send(GuiCommand::AddTorrentFile {
            path: "/home/user/big.torrent".into(),
            download_dir: None,
        })
        .expect("send should succeed");

        let cmd = rx.try_recv().expect("should receive command");
        match cmd {
            GuiCommand::AddTorrentFile { download_dir, .. } => {
                assert!(download_dir.is_none());
            }
            _ => panic!("expected AddTorrentFile variant"),
        }
    }

    #[test]
    fn extract_torrent_info_single_file() {
        // Create a single-file torrent via CreateTorrent.
        let tmp = std::env::temp_dir().join("irontide_gui_test_single.bin");
        std::fs::write(&tmp, b"hello torrent gui test").expect("write tmp file");
        let result = irontide::core::CreateTorrent::new()
            .add_file(&tmp)
            .set_piece_size(16384)
            .generate()
            .expect("generate torrent");
        let _ = std::fs::remove_file(&tmp);

        let meta =
            irontide::core::torrent_from_bytes_any(&result.bytes).expect("parse generated torrent");
        let (name, total_size, file_count) = extract_torrent_info(&meta);
        assert!(!name.is_empty());
        assert_eq!(total_size, 22); // "hello torrent gui test" is 22 bytes
        assert_eq!(file_count, 1);
    }

    #[test]
    fn extract_torrent_info_multi_file() {
        // Create a multi-file torrent.
        let dir = tempfile::tempdir().expect("create temp dir");
        let file_a = dir.path().join("file_a.txt");
        let file_b = dir.path().join("file_b.txt");
        std::fs::write(&file_a, b"aaaa").expect("write file_a");
        std::fs::write(&file_b, b"bbbbb").expect("write file_b");
        let result = irontide::core::CreateTorrent::new()
            .add_file(&file_a)
            .add_file(&file_b)
            .set_piece_size(16384)
            .generate()
            .expect("generate torrent");

        let meta =
            irontide::core::torrent_from_bytes_any(&result.bytes).expect("parse generated torrent");
        let (name, total_size, file_count) = extract_torrent_info(&meta);
        assert!(!name.is_empty());
        assert_eq!(total_size, 9); // 4 + 5
        assert_eq!(file_count, 2);
    }

    // ── Path traversal safety tests ──────────────────────────────────────

    #[test]
    fn path_traversal_safe_path_within_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let safe_file = dir.path().join("safe_file.txt");
        std::fs::write(&safe_file, b"test").expect("write safe file");

        let canonical = safe_file.canonicalize().expect("canonicalize safe file");
        let canonical_dir = dir.path().canonicalize().expect("canonicalize dir");

        assert!(canonical.starts_with(&canonical_dir));
    }

    #[test]
    fn path_traversal_detects_escape() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let canonical_dir = dir.path().canonicalize().expect("canonicalize dir");

        // Construct a path that attempts to escape the save directory.
        let evil_path = dir.path().join("../");
        if let Ok(canonical_evil) = evil_path.canonicalize() {
            assert!(
                !canonical_evil.starts_with(&canonical_dir) || canonical_evil == canonical_dir,
                "traversal path should not be a strict child of save dir"
            );
        }
        // If canonicalize fails (target doesn't exist), that's also safe
        // because `delete_torrent_files` returns early in that case.
    }

    #[test]
    fn delete_torrent_files_removes_single_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let file = dir.path().join("test_torrent.bin");
        std::fs::write(&file, b"data").expect("write file");
        assert!(file.exists());

        delete_torrent_files(dir.path(), "test_torrent.bin", 1);
        assert!(!file.exists(), "single file should be deleted");
    }

    #[test]
    fn delete_torrent_files_removes_directory() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let torrent_dir = dir.path().join("multi_file_torrent");
        std::fs::create_dir(&torrent_dir).expect("create torrent dir");
        std::fs::write(torrent_dir.join("file_a.bin"), b"aaa").expect("write a");
        std::fs::write(torrent_dir.join("file_b.bin"), b"bbb").expect("write b");
        assert!(torrent_dir.exists());

        delete_torrent_files(dir.path(), "multi_file_torrent", 2);
        assert!(!torrent_dir.exists(), "torrent directory should be deleted");
    }

    #[test]
    fn delete_torrent_files_refuses_traversal() {
        let parent = tempfile::tempdir().expect("create parent dir");
        let save_dir = parent.path().join("downloads");
        std::fs::create_dir(&save_dir).expect("create save dir");

        // Create a file outside save_dir that a traversal attack would target.
        let outside_file = parent.path().join("secret.txt");
        std::fs::write(&outside_file, b"secret").expect("write secret");

        // Attempt traversal: name = "../secret.txt"
        delete_torrent_files(&save_dir, "../secret.txt", 1);
        assert!(
            outside_file.exists(),
            "file outside save dir must NOT be deleted"
        );
    }

    #[test]
    fn delete_torrent_files_nonexistent_is_noop() {
        let dir = tempfile::tempdir().expect("create temp dir");
        // Should not panic — just returns early.
        delete_torrent_files(dir.path(), "does_not_exist.bin", 1);
    }

    // ── M192: Create Torrent helpers ─────────────────────────────────────

    #[test]
    fn parse_tracker_tiers_single_tier() {
        let text = "http://tracker1.example.com/announce\nhttp://tracker2.example.com/announce";
        let tiers = parse_tracker_tiers(text);
        assert_eq!(tiers.len(), 2);
        assert_eq!(tiers[0].1, 0);
        assert_eq!(tiers[1].1, 0);
    }

    #[test]
    fn parse_tracker_tiers_multi_tier() {
        let text = "http://tier0.example.com/announce\n\nhttp://tier1a.example.com/announce\nhttp://tier1b.example.com/announce\n\nhttp://tier2.example.com/announce";
        let tiers = parse_tracker_tiers(text);
        assert_eq!(tiers.len(), 4);
        assert_eq!(tiers[0].1, 0);
        assert_eq!(tiers[1].1, 1);
        assert_eq!(tiers[2].1, 1);
        assert_eq!(tiers[3].1, 2);
    }

    #[test]
    fn parse_tracker_tiers_empty_input() {
        assert!(parse_tracker_tiers("").is_empty());
        assert!(parse_tracker_tiers("   \n  \n").is_empty());
    }

    #[test]
    fn parse_tracker_tiers_trims_whitespace() {
        let text = "  http://t.example.com/announce  ";
        let tiers = parse_tracker_tiers(text);
        assert_eq!(tiers.len(), 1);
        assert_eq!(tiers[0].0, "http://t.example.com/announce");
    }

    #[test]
    fn resolve_piece_size_auto_uses_core() {
        let size = resolve_piece_size("Auto", 500_000_000);
        assert_eq!(size, irontide::core::auto_piece_size(500_000_000));
    }

    #[test]
    fn resolve_piece_size_explicit_values() {
        assert_eq!(resolve_piece_size("256 KiB", 0), 256 * 1024);
        assert_eq!(resolve_piece_size("512 KiB", 0), 512 * 1024);
        assert_eq!(resolve_piece_size("1 MiB", 0), 1024 * 1024);
        assert_eq!(resolve_piece_size("2 MiB", 0), 2 * 1024 * 1024);
        assert_eq!(resolve_piece_size("4 MiB", 0), 4 * 1024 * 1024);
    }

    #[test]
    fn resolve_piece_size_unknown_falls_back_to_auto() {
        let size = resolve_piece_size("garbage", 1_000_000);
        assert_eq!(size, irontide::core::auto_piece_size(1_000_000));
    }

    #[test]
    fn dir_total_size_empty_dir() {
        let dir = tempfile::tempdir().expect("create temp dir");
        assert_eq!(dir_total_size(dir.path()), 0);
    }

    #[test]
    fn dir_total_size_with_files() {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"world!").unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("c.txt"), b"nested").unwrap();
        assert_eq!(dir_total_size(dir.path()), 5 + 6 + 6);
    }
}
