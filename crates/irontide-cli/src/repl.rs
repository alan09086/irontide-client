//! Interactive REPL mode (`irontide shell`).
//!
//! A rustyline-powered prompt that reuses the batch-mode command
//! dispatch (`crate::commands`) and the typed DTOs (`crate::client`),
//! giving the REPL exactly the same behaviour as batch subcommands plus
//! a few REPL-only extras (`help`, `watch`, `clear`, `quit`).
//!
//! # Runtime model
//!
//! The readline call is a blocking syscall; we therefore hold the
//! `tokio::runtime::Runtime` on the main thread and only cross into
//! async via `rt.block_on(..)` for the command dispatch, the background
//! prompt refresh, and the `watch` event stream. Nothing inside the
//! readline loop awaits.
//!
//! # Background prompt refresh
//!
//! A small `tokio::task` polls `client.list_torrents()` every 5s and
//! caches the aggregate `(connected, torrents, ↓rate, ↑rate)` snapshot
//! in an `Arc<Mutex<CachedState>>`. The readline loop reads the latest
//! snapshot when rendering the prompt; it never blocks on the daemon.
//! When a poll fails, the cached state flips to `connected = false`
//! and the prompt switches to `irontide (disconnected) > `.
//!
//! # Parsing
//!
//! Command lines are split via `shlex::split` (to preserve quoted
//! magnet URIs that embed query parameters) and re-parsed through
//! `Cli::try_parse_from`. REPL-only verbs (`help`, `clear`, `quit`,
//! `watch`, …) are intercepted *before* clap sees them.

use std::io::{IsTerminal as _, Write as _};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::Parser as _;
use futures::StreamExt as _;
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::{Config, Editor};

use crate::client::ApiClient;
use crate::commands::{self, ListArgs, Output};
use crate::error::CliError;
use crate::format::format_rate;
use crate::{Cli, Command};

/// Options for running the interactive shell.
pub(crate) struct ShellOpts {
    /// Value of the top-level `--api-url` flag (if any). Falls back to
    /// `IRONTIDE_API_URL` → `http://127.0.0.1:9080` inside `ApiClient`.
    pub(crate) api_url: Option<String>,
}

/// Shared prompt-refresh snapshot.
///
/// The background polling task writes this every 5s; the readline loop
/// reads it on every render. `connected = false` flips the prompt into
/// disconnected mode until the next successful poll.
#[derive(Debug, Clone, Copy)]
struct CachedState {
    /// Whether the last `list_torrents()` call succeeded.
    connected: bool,
    /// Number of torrents in the daemon.
    num_torrents: usize,
    /// Aggregate download rate (bytes/sec).
    total_download_rate: u64,
    /// Aggregate upload rate (bytes/sec).
    total_upload_rate: u64,
}

impl CachedState {
    const fn initial() -> Self {
        Self {
            connected: false,
            num_torrents: 0,
            total_download_rate: 0,
            total_upload_rate: 0,
        }
    }
}

/// How often the background task polls the daemon.
const PROMPT_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// Shell-parsed line as interpreted by the REPL.
///
/// This is the in-memory output of the pure `parse_shell_line` helper,
/// which makes the parser testable without running the event loop.
///
/// We intentionally do NOT derive `Debug` because `Cli` / `Command`
/// are clap-derived types that don't carry their own `Debug` impl,
/// and adding one in `main.rs` would risk printing user flag values
/// in panics. Tests use `matches!` against variants instead.
pub(crate) enum ReplLine {
    /// Empty line (whitespace-only) — no-op, just re-prompt.
    Empty,
    /// `help` / `?` — show the REPL cheat sheet.
    Help,
    /// `clear` — clear the screen (TTY only).
    Clear,
    /// `quit` / `exit` — leave the REPL cleanly.
    Quit,
    /// `watch <hash-or-prefix>` — subscribe to daemon events.
    ///
    /// The argument is currently unused by the dispatcher (T6 prints
    /// all events regardless of hash — hash-scoped filtering is a
    /// follow-up), but it is retained here so the REPL can display it
    /// in the "watching …" banner.
    Watch(String),
    /// A line that parsed successfully through the top-level `Cli`
    /// clap definition — the dispatcher will forward it to the
    /// matching `commands::cmd_*` entry point.
    Batch(Box<Cli>),
}

/// Parser errors from `parse_shell_line`.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ShellParseError {
    /// The line could not be tokenized (unclosed quotes, bad escapes).
    #[error("failed to parse line: {0}")]
    Lex(String),
    /// Clap rejected the tokenized argv.
    #[error("{0}")]
    Clap(String),
}

/// Pure function: turn a raw REPL input line into a `ReplLine`.
///
/// This is the ONE piece of shell logic we unit-test; everything else
/// (history, prompt, async dispatch) is covered by manual smoke-testing.
pub(crate) fn parse_shell_line(line: &str) -> Result<ReplLine, ShellParseError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(ReplLine::Empty);
    }

    // REPL-local verbs first — they don't go through clap.
    match trimmed {
        "help" | "?" => return Ok(ReplLine::Help),
        "clear" => return Ok(ReplLine::Clear),
        "quit" | "exit" => return Ok(ReplLine::Quit),
        _ => {}
    }

    // `watch <hash>` — the rest of the line after `watch ` is the
    // hash-or-prefix (we take it raw, no clap involvement, so we
    // keep T6's minimalism).
    if let Some(rest) = trimmed.strip_prefix("watch ") {
        let arg = rest.trim().to_owned();
        if arg.is_empty() {
            return Err(ShellParseError::Clap(
                "usage: watch <hash-or-prefix>".to_owned(),
            ));
        }
        return Ok(ReplLine::Watch(arg));
    }
    if trimmed == "watch" {
        return Err(ShellParseError::Clap(
            "usage: watch <hash-or-prefix>".to_owned(),
        ));
    }

    // Tokenize. `shlex::split` returns `None` on a malformed quote,
    // which we surface as a specific lex error.
    let mut argv = shlex::split(trimmed)
        .ok_or_else(|| ShellParseError::Lex("unclosed quote or bad escape".to_owned()))?;

    if argv.is_empty() {
        return Ok(ReplLine::Empty);
    }

    // Prepend the binary name so clap sees the same argv layout as
    // batch mode. We insert (rather than overwrite) because the user
    // typed only the subcommand, not the binary name.
    argv.insert(0, "irontide".to_owned());

    match Cli::try_parse_from(&argv) {
        Ok(cli) => Ok(ReplLine::Batch(Box::new(cli))),
        Err(e) => Err(ShellParseError::Clap(e.to_string())),
    }
}

/// Entry point for `irontide shell`.
///
/// Builds the tokio runtime, the API client, a rustyline editor, and
/// runs the main input loop until the user leaves (EOF, `quit`, or
/// `exit`). Errors from individual command dispatches are printed and
/// the loop continues — the REPL only exits on fatal failures
/// (runtime build, editor create).
pub(crate) fn run(opts: ShellOpts) -> anyhow::Result<()> {
    // ── runtime + client ──────────────────────────────────────────────
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let url = ApiClient::resolve_url(opts.api_url.as_deref());
    let client = Arc::new(ApiClient::new(url.clone()));

    // ── background prompt-refresh task ────────────────────────────────
    let cached = Arc::new(Mutex::new(CachedState::initial()));
    let refresh_task = {
        let cached = Arc::clone(&cached);
        let client = Arc::clone(&client);
        rt.spawn(async move {
            let mut ticker = tokio::time::interval(PROMPT_REFRESH_INTERVAL);
            // Use `Skip` so a paused runtime (e.g. the main thread
            // blocking on readline for 10+s) doesn't generate a burst
            // of catch-up ticks when async gets CPU again.
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                match client.list_torrents().await {
                    Ok(list) => {
                        let total_dl: u64 = list.iter().map(|t| t.download_rate).sum();
                        let total_ul: u64 = list.iter().map(|t| t.upload_rate).sum();
                        if let Ok(mut guard) = cached.lock() {
                            *guard = CachedState {
                                connected: true,
                                num_torrents: list.len(),
                                total_download_rate: total_dl,
                                total_upload_rate: total_ul,
                            };
                        }
                    }
                    Err(_) => {
                        // Mark disconnected; preserve last-known counts
                        // so the prompt doesn't flicker to zero.
                        if let Ok(mut guard) = cached.lock() {
                            guard.connected = false;
                        }
                    }
                }
            }
        })
    };

    // ── prime the cache synchronously so the very first prompt
    //    reflects reality (otherwise it starts as "disconnected"). ────
    rt.block_on(async {
        if let Ok(list) = client.list_torrents().await
            && let Ok(mut guard) = cached.lock()
        {
            let total_dl: u64 = list.iter().map(|t| t.download_rate).sum();
            let total_ul: u64 = list.iter().map(|t| t.upload_rate).sum();
            *guard = CachedState {
                connected: true,
                num_torrents: list.len(),
                total_download_rate: total_dl,
                total_upload_rate: total_ul,
            };
        }
    });

    // ── rustyline editor + persistent history ─────────────────────────
    let config = Config::builder().auto_add_history(true).build();
    let mut editor: Editor<(), FileHistory> = Editor::with_config(config)?;

    let history_path = history_file_path();
    if let Some(parent) = history_path.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    // `NotFound` is expected on first run — ignore it so we don't
    // spam the user with a warning every cold start.
    if let Err(e) = editor.load_history(&history_path)
        && !matches!(e, ReadlineError::Io(ref io) if io.kind() == std::io::ErrorKind::NotFound)
    {
        eprintln!("warning: failed to load history from {history_path:?}: {e}");
    }

    let tty = std::io::stdout().is_terminal();

    // ── main loop ─────────────────────────────────────────────────────
    println!("irontide shell — type 'help' for commands, 'quit' to exit");

    loop {
        // Snapshot the cached state into a local copy so we release
        // the mutex before the blocking readline call.
        let snapshot = cached
            .lock().map_or_else(|poisoned| *poisoned.into_inner(), |g| *g);
        let prompt = render_prompt(&snapshot, tty);
        let line = match editor.readline(&prompt) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C on an empty line: just re-prompt.
                println!();
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D: clean exit.
                println!();
                break;
            }
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        };

        // Explicitly record the history entry. `auto_add_history`
        // only fires on interactive (TTY) input, so piped stdin would
        // otherwise leave the history file empty. We do this BEFORE
        // dispatch so crash-recovery still captures the last command.
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            let _ = editor.add_history_entry(trimmed);
        }

        match parse_shell_line(&line) {
            Ok(ReplLine::Empty) => continue,
            Ok(ReplLine::Help) => print_help(),
            Ok(ReplLine::Clear) => {
                if tty {
                    // ANSI: clear screen + home cursor.
                    print!("\x1b[2J\x1b[H");
                    let _ = std::io::stdout().flush();
                }
            }
            Ok(ReplLine::Quit) => break,
            Ok(ReplLine::Watch(arg)) => {
                if let Err(e) = rt.block_on(run_watch(&client, &arg)) {
                    eprintln!("error: {e}");
                }
            }
            Ok(ReplLine::Batch(cli)) => {
                dispatch_batch(&rt, &client, *cli);
            }
            Err(e) => {
                eprintln!("error: {e}");
            }
        }
    }

    // ── shutdown: save history, kill refresh task, drop runtime ───────
    if let Err(e) = editor.save_history(&history_path) {
        eprintln!("warning: failed to save history to {history_path:?}: {e}");
    }
    refresh_task.abort();
    // Allow the aborted task to unwind before dropping the runtime so
    // we don't leak pending I/O handles.
    rt.shutdown_timeout(Duration::from_millis(250));
    Ok(())
}

/// Dispatch a single `Cli` parsed from REPL input to the matching
/// `commands::cmd_*` entry point. Errors are printed and swallowed;
/// the caller resumes the REPL loop regardless.
#[allow(clippy::too_many_lines)] // straight-line match; splitting would hurt readability
fn dispatch_batch(rt: &tokio::runtime::Runtime, client: &ApiClient, cli: Cli) {
    let stdout = std::io::stdout();
    let mut stdout_lock = stdout.lock();

    let result: Result<(), CliError> = rt.block_on(async {
        match cli.command {
            Command::List { json, filter } => {
                let args = ListArgs { filter };
                let mut out = if json {
                    Output::Json(&mut stdout_lock)
                } else {
                    Output::Human(&mut stdout_lock)
                };
                commands::cmd_list(client, &args, &mut out).await
            }
            Command::Add { source, json } => {
                let mut out = if json {
                    Output::Json(&mut stdout_lock)
                } else {
                    Output::Human(&mut stdout_lock)
                };
                commands::cmd_add(client, &source, &mut out).await
            }
            Command::Rm { hash, json } => {
                let mut out = if json {
                    Output::Json(&mut stdout_lock)
                } else {
                    Output::Human(&mut stdout_lock)
                };
                commands::cmd_remove(client, &hash, &mut out).await
            }
            Command::Pause { hash, json } => {
                let mut out = if json {
                    Output::Json(&mut stdout_lock)
                } else {
                    Output::Human(&mut stdout_lock)
                };
                commands::cmd_pause(client, &hash, &mut out).await
            }
            Command::Resume { hash, json } => {
                let mut out = if json {
                    Output::Json(&mut stdout_lock)
                } else {
                    Output::Human(&mut stdout_lock)
                };
                commands::cmd_resume(client, &hash, &mut out).await
            }
            Command::Seed { hash, json } => {
                let mut out = if json {
                    Output::Json(&mut stdout_lock)
                } else {
                    Output::Human(&mut stdout_lock)
                };
                commands::cmd_seed(client, &hash, true, &mut out).await
            }
            Command::Unseed { hash, json } => {
                let mut out = if json {
                    Output::Json(&mut stdout_lock)
                } else {
                    Output::Human(&mut stdout_lock)
                };
                commands::cmd_seed(client, &hash, false, &mut out).await
            }
            Command::Info {
                source,
                files,
                peers,
                json,
            } => {
                let mut out = if json {
                    Output::Json(&mut stdout_lock)
                } else {
                    Output::Human(&mut stdout_lock)
                };
                // REPL only knows how to ask the daemon — reject
                // file-path info lookups with a clear message.
                if std::path::Path::new(&source).is_file() {
                    return Err(CliError::InvalidInput(
                        "file-path info is not available in the REPL — use batch mode".to_owned(),
                    ));
                }
                commands::cmd_info(client, &source, files, peers, &mut out).await
            }
            // Non-REPL commands — batch-only today.
            Command::Download { .. } => Err(CliError::InvalidInput(
                "'download' is not available inside the REPL — use batch mode".to_owned(),
            )),
            Command::Daemon { .. } => Err(CliError::InvalidInput(
                "'daemon' is not available inside the REPL — use batch mode".to_owned(),
            )),
            Command::Create { .. } => Err(CliError::InvalidInput(
                "'create' is not available inside the REPL — use batch mode".to_owned(),
            )),
            Command::Shell => Err(CliError::InvalidInput(
                "already inside 'shell' — ignoring nested invocation".to_owned(),
            )),
            Command::Tui => Err(CliError::InvalidInput(
                "'tui' is not available inside the REPL — use batch mode".to_owned(),
            )),
            Command::Config { .. } => Err(CliError::InvalidInput(
                "'config' is not available inside the REPL — use batch mode".to_owned(),
            )),
            Command::Completions { .. } => Err(CliError::InvalidInput(
                "'completions' is not available inside the REPL — use batch mode".to_owned(),
            )),
        }
    });

    let _ = stdout_lock.flush();
    drop(stdout_lock);

    if let Err(e) = result {
        eprintln!("error: {e}");
    }
}

/// Subscribe to the daemon's `/api/v1/events` WebSocket stream and
/// print frames until Ctrl-C. The `hash_hint` argument is displayed
/// in the banner but not used for filtering (T3's subscribe endpoint
/// does not expose per-hash filters — that's a follow-up).
async fn run_watch(client: &ApiClient, hash_hint: &str) -> Result<(), CliError> {
    println!("watching events for {hash_hint} (all events; Ctrl-C to return)…");
    // The tungstenite `FilterMap` stream is not `Unpin`, so pin it
    // on the heap before handing it to `tokio::select!`. `Box::pin`
    // is the idiomatic escape hatch here; the overhead is one
    // allocation per `watch` invocation, which is negligible against
    // a live WebSocket connection.
    let stream = client.subscribe_events().await?;
    let mut stream = Box::pin(stream);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!();
                println!("(stopped watching)");
                return Ok(());
            }
            next = stream.next() => {
                match next {
                    Some(Ok(text)) => {
                        // Raw-print the JSON line; keeps the dispatch
                        // simple and lets the user pipe to `jq` later
                        // if they want pretty output.
                        println!("{text}");
                    }
                    Some(Err(e)) => {
                        eprintln!("stream error: {e}");
                        return Err(e);
                    }
                    None => {
                        println!("(stream closed)");
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Render the dynamic prompt from the latest cached state.
fn render_prompt(state: &CachedState, tty: bool) -> String {
    let body = if state.connected {
        format!(
            "irontide ({} torrents, ↓{} ↑{}) > ",
            state.num_torrents,
            format_rate(state.total_download_rate),
            format_rate(state.total_upload_rate),
        )
    } else {
        "irontide (disconnected) > ".to_owned()
    };

    if tty {
        // 36 = cyan, 31 = red. `\x1b[0m` resets.
        let colour = if state.connected { "36" } else { "31" };
        format!("\x1b[{colour}m{body}\x1b[0m")
    } else {
        body
    }
}

/// Print the REPL help cheat sheet.
fn print_help() {
    println!();
    println!("irontide shell — available commands");
    println!("-----------------------------------");
    println!("  list [--json] [--filter <state>]   list active torrents");
    println!("  add <source> [--json]              add magnet or .torrent file");
    println!("  rm <hash> [--json]                 remove a torrent");
    println!("  pause <hash> [--json]              pause a torrent");
    println!("  resume <hash> [--json]             resume a paused torrent");
    println!("  seed <hash> [--json]               flip to seed-only mode");
    println!("  unseed <hash> [--json]             clear seed-only mode");
    println!("  info <hash> [--files] [--peers] [--json]");
    println!("                                     show torrent details");
    println!();
    println!("REPL-only commands");
    println!("  help, ?                            show this help");
    println!("  watch <hash>                       stream daemon events (Ctrl-C to stop)");
    println!("  clear                              clear the screen");
    println!("  quit, exit                         leave the shell");
    println!();
    println!("note: download / create / daemon are batch-only commands.");
    println!();
}

/// Locate the persistent history file.
///
/// Priority order:
/// 1. `$XDG_CACHE_HOME/irontide/history`
/// 2. `$HOME/.cache/irontide/history`
/// 3. `./.irontide_history` (CWD fallback — only reached when both
///    `XDG_CACHE_HOME` and `HOME` are unset, which is rare but happens
///    in sandboxed CI environments).
fn history_file_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("irontide").join("history");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home)
            .join(".cache")
            .join("irontide")
            .join("history");
    }
    PathBuf::from(".irontide_history")
}

#[cfg(test)]
mod tests {
    //! Unit tests for `parse_shell_line`.
    //!
    //! The tests live in the binary crate (rather than `tests/`) because
    //! `Cli`, `Command`, and `parse_shell_line` are all `pub(crate)` —
    //! external integration tests can't reach them. See the Task 6 deviation
    //! note in the completion report.

    use super::*;

    /// Tiny helper: unwrap a parsed line as a `Batch(Cli)` or panic.
    fn expect_batch(line: ReplLine) -> Box<Cli> {
        match line {
            ReplLine::Batch(cli) => cli,
            ReplLine::Empty => panic!("expected Batch, got Empty"),
            ReplLine::Help => panic!("expected Batch, got Help"),
            ReplLine::Clear => panic!("expected Batch, got Clear"),
            ReplLine::Quit => panic!("expected Batch, got Quit"),
            ReplLine::Watch(_) => panic!("expected Batch, got Watch"),
        }
    }

    #[test]
    fn test_parses_list_command() {
        let line = parse_shell_line("list --json").expect("should parse");
        let cli = expect_batch(line);
        match cli.command {
            Command::List { json, filter } => {
                assert!(json, "expected --json to set json=true");
                assert!(filter.is_none(), "expected no filter");
            }
            _ => panic!("expected Command::List"),
        }
    }

    #[test]
    fn test_parses_list_with_filter() {
        let line = parse_shell_line("list --filter downloading").expect("should parse");
        let cli = expect_batch(line);
        match cli.command {
            Command::List { json, filter } => {
                assert!(!json);
                assert_eq!(filter.as_deref(), Some("downloading"));
            }
            _ => panic!("expected Command::List"),
        }
    }

    #[test]
    fn test_parses_quoted_magnet() {
        // shlex strips the surrounding quotes and keeps the URI intact
        // — including the `?xt=...` query string, which would otherwise
        // trip up whitespace-only splitting.
        let line = parse_shell_line("add \"magnet:?xt=urn:btih:aabbcc\"").expect("should parse");
        let cli = expect_batch(line);
        match cli.command {
            Command::Add { source, json } => {
                assert_eq!(source, "magnet:?xt=urn:btih:aabbcc");
                assert!(!json);
            }
            _ => panic!("expected Command::Add"),
        }
    }

    #[test]
    fn test_parses_seed_hash() {
        let line = parse_shell_line("seed aabbcc").expect("should parse");
        let cli = expect_batch(line);
        match cli.command {
            Command::Seed { hash, json } => {
                assert_eq!(hash, "aabbcc");
                assert!(!json);
            }
            _ => panic!("expected Command::Seed"),
        }
    }

    #[test]
    fn test_rejects_unknown_command() {
        match parse_shell_line("frobnicate foo") {
            Ok(_) => panic!("expected unknown command to fail"),
            Err(err) => assert!(matches!(err, ShellParseError::Clap(_))),
        }
    }

    #[test]
    fn test_empty_line_noop() {
        let line = parse_shell_line("").expect("should parse");
        assert!(matches!(line, ReplLine::Empty));
        // Also tests whitespace-only.
        let line = parse_shell_line("   \t  ").expect("should parse");
        assert!(matches!(line, ReplLine::Empty));
    }

    #[test]
    fn test_help_command_recognized() {
        assert!(matches!(
            parse_shell_line("help").expect("should parse"),
            ReplLine::Help
        ));
        assert!(matches!(
            parse_shell_line("?").expect("should parse"),
            ReplLine::Help
        ));
    }

    #[test]
    fn test_quit_command_recognized() {
        assert!(matches!(
            parse_shell_line("quit").expect("should parse"),
            ReplLine::Quit
        ));
        assert!(matches!(
            parse_shell_line("exit").expect("should parse"),
            ReplLine::Quit
        ));
    }

    #[test]
    fn test_clear_command_recognized() {
        assert!(matches!(
            parse_shell_line("clear").expect("should parse"),
            ReplLine::Clear
        ));
    }

    #[test]
    fn test_watch_command_parses_arg() {
        let line = parse_shell_line("watch aabbccdd").expect("should parse");
        match line {
            ReplLine::Watch(arg) => assert_eq!(arg, "aabbccdd"),
            _ => panic!("expected Watch variant"),
        }
    }

    #[test]
    fn test_watch_without_arg_errors() {
        match parse_shell_line("watch") {
            Ok(_) => panic!("expected bare `watch` to fail"),
            Err(err) => assert!(matches!(err, ShellParseError::Clap(_))),
        }
    }

    #[test]
    fn test_render_prompt_connected() {
        let state = CachedState {
            connected: true,
            num_torrents: 3,
            total_download_rate: 2_048,
            total_upload_rate: 1_024,
        };
        let plain = render_prompt(&state, false);
        assert!(plain.contains("3 torrents"));
        assert!(plain.contains("2.0 KB/s"));
        assert!(plain.contains("1.0 KB/s"));
        // TTY prompt wraps the body in a cyan ANSI escape.
        let coloured = render_prompt(&state, true);
        assert!(coloured.starts_with("\x1b[36m"));
        assert!(coloured.ends_with("\x1b[0m"));
    }

    #[test]
    fn test_render_prompt_disconnected() {
        let state = CachedState {
            connected: false,
            num_torrents: 0,
            total_download_rate: 0,
            total_upload_rate: 0,
        };
        let plain = render_prompt(&state, false);
        assert_eq!(plain, "irontide (disconnected) > ");
        let coloured = render_prompt(&state, true);
        assert!(coloured.starts_with("\x1b[31m")); // red when disconnected
    }
}
