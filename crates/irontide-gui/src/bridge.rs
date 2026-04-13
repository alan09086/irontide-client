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
        crate::poll::init_model(&win);
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
            show_toast(weak, "Add Torrent File: coming in M164 Step 5", false);
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
fn show_toast(weak: &slint::Weak<crate::MainWindow>, msg: &str, is_error: bool) {
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
/// Stub implementation — shows a toast. Full `rfd` folder-picker in Step 5.
pub fn handle_browse_download_dir(weak: &slint::Weak<crate::MainWindow>) {
    show_toast(weak, "Browse: folder picker coming in Step 5", false);
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
            if delete_files {
                tracing::warn!(
                    "delete_files=true but file deletion not yet implemented (M164 Step 7)"
                );
            }
            let msg = batch_action(&hashes, session, "Removed", |s, id| {
                Box::pin(s.remove_torrent(id))
            })
            .await;
            show_toast(weak, &msg, false);
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
    let magnet = match irontide::core::Magnet::parse(&uri) {
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

/// Stub handler for adding a torrent from a `.torrent` file.
///
/// Full implementation in M164 Step 5.
async fn handle_add_torrent_file(
    path: String,
    _download_dir: Option<String>,
    _session: &irontide::session::SessionHandle,
    weak: &slint::Weak<crate::MainWindow>,
) {
    show_toast(weak, &format!("Add Torrent: stub for '{path}'"), false);
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
}
