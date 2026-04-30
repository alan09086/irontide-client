//! Full-screen ratatui dashboard (`irontide tui`).
//!
//! Live-monitoring UI that polls the daemon via the T3 [`ApiClient`]
//! on a 500ms tick and subscribes to the WebSocket event stream for
//! immediate invalidation on torrent add/remove/state-change events.
//!
//! Layout, widgets, input handling, and app state live in sibling
//! submodules (`ui`, `events`, `state`). The main event loop is in
//! [`run`] and is deliberately kept small — the rule is "no business
//! logic inside the `tokio::select!` arms, just I/O dispatch".
//!
//! # Terminal lifecycle
//!
//! Entering the TUI puts the terminal into raw mode + alternate
//! screen. A panic hook is installed *before* raw mode is enabled so
//! that a panic inside the event loop restores the terminal before
//! printing the backtrace — otherwise the panic message would land
//! in the raw-mode screen buffer and the user would see nothing but
//! a broken prompt. A matching [`TerminalGuard`] RAII guard restores
//! the terminal on normal exit (including `?` propagation).
//!
//! # Event loop shape
//!
//! ```ignore
//! loop {
//!     draw(state)
//!     if state.should_quit { break }
//!     select! {
//!         _ = refresh_interval.tick() => refresh_from_api().await,
//!         Some(Ok(ev)) = events.next() => dispatch_key(ev).await,
//!         Some(Ok(_)) = ws_stream_next() => refresh_list_only().await,
//!     }
//! }
//! ```
//!
//! The WebSocket stream is optional — if the subscription fails at
//! startup, we proceed with polling only. If the stream drops mid
//! session, the branch never fires again but the tick refresher
//! keeps the UI live.

pub(crate) mod events;
pub(crate) mod state;
pub(crate) mod ui;

use std::io::{Stdout, stdout};
use std::pin::Pin;
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt as _;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use self::events::Action;
use self::state::AppState;
use crate::client::ApiClient;
use crate::error::CliError;

/// Options for the TUI mode.
pub(crate) struct TuiOpts {
    /// Value of the top-level `--api-url` flag, if any. Resolution
    /// order matches the other modes: flag → `IRONTIDE_API_URL` env
    /// var → `http://127.0.0.1:9080` default.
    pub(crate) api_url: Option<String>,
}

/// Tick interval for the poll-based refresh branch.
const REFRESH_TICK: Duration = Duration::from_millis(500);

/// Freshness window for cached per-torrent detail. A re-fetch is
/// issued when the cached entry is older than this.
const DETAIL_STALE: Duration = Duration::from_millis(500);

/// Heap-pinned, lifetime-erased WebSocket stream of JSON strings.
///
/// The `'a` lifetime is tied to the `ApiClient` borrow that
/// produced the stream — we cannot use `'static` because the
/// client is held by reference, not owned.
type EventStreamBox<'a> = Pin<Box<dyn futures::Stream<Item = Result<String, CliError>> + 'a>>;

/// Enter the TUI dashboard.
///
/// Builds a current-thread tokio runtime, sets up the terminal,
/// installs a panic hook + RAII guard, and drives the event loop
/// to completion.
///
/// # Errors
///
/// Returns an `anyhow::Error` if runtime construction, terminal
/// setup, or the initial `ApiClient` handshake fails. Transient
/// daemon errors during the loop are rendered in the status bar
/// but do not terminate the session.
pub(crate) fn run(opts: &TuiOpts) -> anyhow::Result<()> {
    install_panic_hook();

    // Raw mode + alt screen. The guard restores them on every exit
    // path, including `?` propagation.
    enable_raw_mode()?;
    let mut stdout_handle = stdout();
    execute!(stdout_handle, EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let url = ApiClient::resolve_url(opts.api_url.as_deref());
    let client = ApiClient::new(url);

    let outcome = rt.block_on(event_loop(&mut terminal, &client));

    // Restore the terminal even if the loop returned an error. The
    // `TerminalGuard` would cover this too, but being explicit makes
    // the shutdown ordering obvious.
    terminal.show_cursor()?;

    // Drop the runtime with a short timeout so dangling HTTP / WS
    // tasks don't hold the process alive.
    rt.shutdown_timeout(Duration::from_millis(250));

    outcome
}

/// The async event loop. Extracted so it can `?`-propagate cleanly
/// while the outer `run` still owns the terminal restoration path.
async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    client: &ApiClient,
) -> anyhow::Result<()> {
    let mut state = AppState::new();

    // Prime the list so the first draw has something to show.
    match client.list_torrents().await {
        Ok(list) => {
            state.replace_torrents(list);
            recompute_rates(&mut state);
            state.clear_error();
        }
        Err(e) => {
            state.set_error(format_err(&e));
        }
    }

    // Crossterm's EventStream is pinned internally; the wrapper is
    // `Unpin`, so we can keep it on the stack.
    let mut key_events = EventStream::new();

    // Refresh timer. `Skip` so a long render doesn't queue up a
    // burst of catch-up ticks.
    let mut refresh = tokio::time::interval(REFRESH_TICK);
    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Optional WebSocket stream. On failure we proceed with polling
    // only — the ticker covers the gap. The stream type is opaque
    // (`impl Stream<…>`) so we box it behind [`EventStreamBox`].
    let mut ws_stream: Option<EventStreamBox<'_>> = match client.subscribe_events().await {
        Ok(s) => Some(Box::pin(s)),
        Err(_) => None,
    };

    loop {
        terminal.draw(|f| ui::draw(f, &state))?;

        if state.should_quit {
            return Ok(());
        }

        tokio::select! {
            // Timer branch — refresh the torrent list and, if
            // applicable, the cached detail for the selected row.
            _ = refresh.tick() => {
                tick_refresh(client, &mut state).await;
            }

            // Keyboard / terminal input.
            maybe_ev = key_events.next() => {
                match maybe_ev {
                    Some(Ok(Event::Key(key))) => {
                        // Two-step: compute the action (borrows state
                        // mutably) first, then dispatch (borrows again).
                        let action = events::handle_key(key, &mut state);
                        dispatch_action(client, &mut state, action).await;
                    }
                    Some(Ok(Event::Resize(_, _) | _)) => {
                        // Ratatui handles resize on the next draw; other
                        // events (mouse, focus, paste) ignored.
                    }
                    Some(Err(e)) => {
                        state.set_error(format!("input error: {e}"));
                    }
                    None => {
                        // Input stream closed — rare, but treat as
                        // a clean exit so we don't spin.
                        state.should_quit = true;
                    }
                }
            }

            // WebSocket event — any message triggers an immediate
            // list refresh without parsing the payload.
            ws_msg = next_ws(&mut ws_stream) => {
                match ws_msg {
                    Some(Ok(_)) => {
                        if let Ok(list) = client.list_torrents().await {
                            state.replace_torrents(list);
                            recompute_rates(&mut state);
                        }
                    }
                    Some(Err(_)) | None => {
                        // Drop the stream so future selects fall
                        // back to polling only. We don't surface
                        // the error — polling still works.
                        ws_stream = None;
                    }
                }
            }
        }
    }
}

/// Advance the timer-driven refresh branch.
async fn tick_refresh(client: &ApiClient, state: &mut AppState) {
    match client.list_torrents().await {
        Ok(list) => {
            state.replace_torrents(list);
            recompute_rates(state);
            state.clear_error();
        }
        Err(CliError::DaemonUnreachable { .. }) => {
            state.set_error("daemon unreachable");
            return;
        }
        Err(e) => {
            state.set_error(format_err(&e));
            return;
        }
    }

    // Refresh cached detail for the selected-expanded row.
    let Some(hash) = state.selected_hash().map(ToOwned::to_owned) else {
        return;
    };
    if !state.expanded.contains(&hash) {
        return;
    }
    let stale = state
        .detail_cache
        .get(&hash)
        .is_none_or(|d| d.fetched_at.elapsed() > DETAIL_STALE);
    if !stale {
        return;
    }

    let Ok(stats) = client.get_torrent(&hash).await else {
        return;
    };
    let Ok(info) = client.torrent_info(&hash).await else {
        return;
    };
    let peers = client.torrent_peers(&hash).await.unwrap_or_default();
    state.detail_cache.insert(
        hash,
        state::CachedDetail {
            info,
            stats,
            peers,
            fetched_at: Instant::now(),
        },
    );
}

/// Execute a user-triggered action against the daemon.
///
/// I/O errors are recorded in the status bar but don't tear down the
/// loop — the next tick will either recover or keep displaying the
/// error. `Action::Quit` sets the exit flag so the next draw tick
/// returns from the loop.
async fn dispatch_action(client: &ApiClient, state: &mut AppState, action: Action) {
    match action {
        Action::None | Action::RefreshNow => {
            // None = no-op; RefreshNow = next tick handles it.
        }
        Action::Quit => state.should_quit = true,
        Action::Pause(hash) => {
            if let Err(e) = client.pause(&hash).await {
                state.set_error(format!("pause failed: {e}"));
            }
        }
        Action::Resume(hash) => {
            if let Err(e) = client.resume(&hash).await {
                state.set_error(format!("resume failed: {e}"));
            }
        }
        Action::Seed(hash, enabled) => {
            if let Err(e) = client.set_seed_mode(&hash, enabled).await {
                state.set_error(format!("seed mode failed: {e}"));
            } else {
                // Bust the cache so the next tick pulls the new
                // user_seed_mode value immediately.
                state.detail_cache.remove(&hash);
            }
        }
        Action::RemoveConfirmed(hash) => {
            if let Err(e) = client.remove_torrent(&hash).await {
                state.set_error(format!("remove failed: {e}"));
            }
        }
        Action::AddMagnet(uri) => {
            if let Err(e) = client.add_magnet(&uri).await {
                state.set_error(format!("add magnet failed: {e}"));
            }
        }
    }
}

/// Helper to poll the (optional) WebSocket stream inside `tokio::select!`.
///
/// Returning a future that resolves to `None` when the stream is
/// absent lets the `select!` macro stay shape-stable. We use a
/// long sleep (rather than `futures::future::pending`) so the
/// compiler has a concrete future type to work with.
async fn next_ws(stream: &mut Option<EventStreamBox<'_>>) -> Option<Result<String, CliError>> {
    if let Some(s) = stream { s.next().await } else {
        tokio::time::sleep(Duration::from_hours(1)).await;
        None
    }
}

/// Recompute aggregate session rates from the current torrent list.
fn recompute_rates(state: &mut AppState) {
    state.agg_down = state.torrents.iter().map(|t| t.download_rate).sum();
    state.agg_up = state.torrents.iter().map(|t| t.upload_rate).sum();
}

/// Format a `CliError` for the status line. Keeps the one-line
/// length predictable by trimming trailing whitespace.
fn format_err(err: &CliError) -> String {
    let text = err.to_string();
    text.trim().to_owned()
}

/// Install a panic hook that restores the terminal before the
/// default panic handler prints its message.
///
/// Must be called *before* [`enable_raw_mode`]; otherwise a panic
/// during the setup sequence would leave the terminal in raw mode.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        default(info);
    }));
}

/// RAII guard that restores the terminal state on drop.
///
/// Installed immediately after `enable_raw_mode` + `EnterAlternateScreen`
/// so that any early-return path (including `?` propagation) leaves
/// the terminal usable.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}
