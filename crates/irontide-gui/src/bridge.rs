use std::sync::Arc;

use parking_lot::Mutex;
use slint::ComponentHandle as _;

use crate::app::{AppPhase, AppState};

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
    let session = match irontide::ClientBuilder::from_settings(settings).start().await {
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

    // Signal UI ready.
    state.lock().phase = AppPhase::Ready;
    let _ = weak.upgrade_in_event_loop(|win| {
        win.set_session_ready(true);
        win.set_status_text("Ready".into());
    });

    // Wait for shutdown signal.
    let _ = shutdown_rx.await;

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
            show_stub_toast(weak, "Add Magnet");
        }
        crate::app::MenuAction::AddTorrentFile => {
            show_stub_toast(weak, "Add Torrent File");
        }
    }
}

fn show_stub_toast(weak: &slint::Weak<crate::MainWindow>, label: &str) {
    let toast = format!("{label}: coming in M164");
    let _ = weak.upgrade_in_event_loop(move |win| {
        win.set_toast_text(toast.into());
        win.set_toast_visible(true);
    });
}

#[cfg(test)]
mod tests {
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
}
