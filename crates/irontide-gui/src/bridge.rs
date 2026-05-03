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
    weak: slint::Weak<crate::MainWindow>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    state: Arc<Mutex<AppState>>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("irontide-session".into())
        .spawn(move || {
            let rt = irontide_config::build_runtime(&settings);
            rt.block_on(async {
                run_session(settings, weak, shutdown_rx, state).await;
            });
            rt.shutdown_timeout(std::time::Duration::from_secs(1));
        })
        .expect("failed to spawn session thread")
}

async fn run_session(
    settings: irontide::session::Settings,
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
                win.set_show_add_magnet_dialog(true);
            });
        }
        crate::app::MenuAction::AddTorrentFile => {
            let _ = weak.upgrade_in_event_loop(|win| {
                win.set_show_add_torrent_dialog(true);
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
/// selection, reads and parses the torrent file to extract name, total
/// size, and file count, then pushes the results to the main window
/// properties so the add-torrent dialog can display them.
pub fn handle_browse_torrent_file(weak: &slint::Weak<crate::MainWindow>) {
    let weak = weak.clone();
    std::thread::spawn(move || {
        let file = rfd::FileDialog::new()
            .add_filter("Torrent", &["torrent"])
            .pick_file();

        if let Some(path) = file {
            let path_str = path.to_string_lossy().into_owned();
            match std::fs::read(&path) {
                Ok(data) => match irontide::core::torrent_from_bytes_any(&data) {
                    Ok(meta) => {
                        let (name, total_size, file_count) = extract_torrent_info(&meta);
                        let size_str = crate::format::format_size(total_size);
                        let count = i32::try_from(file_count).unwrap_or(i32::MAX);
                        let _ = weak.upgrade_in_event_loop(move |win| {
                            win.set_add_torrent_file_path(path_str.into());
                            win.set_add_torrent_name(name.into());
                            win.set_add_torrent_size(size_str.into());
                            win.set_add_torrent_file_count(count);
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
        GuiCommand::ApplySettings {
            download_dir,
            create_subfolder: _,
        } => {
            if let Some(dir) = download_dir {
                handle_set_default_download_dir(&dir, session, weak).await;
            }
        }
        GuiCommand::ReannounceTracker { info_hash, url: _ } => {
            // M178: Per-tracker reannounce is not yet exposed via SessionHandle;
            // fall back to a torrent-wide reannounce (M178 ships the action,
            // M180 polish refines to per-URL when the engine API lands).
            if let Ok(id) = irontide::core::Id20::from_hex(&info_hash) {
                let _ = session.force_reannounce(id).await;
            }
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
        .file_name().map_or_else(|| path.clone(), |f| f.to_string_lossy().into_owned());
    match params.add_to(session).await {
        Ok(_id) => {
            // Clear file-selection state in the dialog.
            let _ = weak.upgrade_in_event_loop(|win| {
                win.set_add_torrent_file_path(slint::SharedString::new());
                win.set_add_torrent_name(slint::SharedString::new());
                win.set_add_torrent_size(slint::SharedString::new());
                win.set_add_torrent_file_count(0);
            });
            show_toast(weak, &format!("Added: {filename}"), false);
        }
        Err(e) => {
            show_toast(weak, &format!("Failed to add torrent: {e}"), true);
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
                        let abs = s.download_dir.clone();
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
}
