mod cli_def;
mod client;
mod commands;
#[allow(dead_code)] // Phase 1 scaffolding — types wired in Phase 4+6
mod config;
mod create;
mod daemon;
mod download;
mod error;
mod format;
mod info;
mod progress;
mod repl;
mod tui;

use cli_def::*;

use clap::Parser as _;
use std::io::Write as _;
use std::time::Duration;

use client::ApiClient;
use commands::{ListArgs, Output};
use error::CliError;

fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| cli.log_level.parse().unwrap_or_else(|_| "error".into())),
        )
        .with_target(false)
        .init();

    // Capture api_url before moving cli.command — the global flag is shared
    // by every batch subcommand's dispatch arm.
    let api_url_flag = cli.api_url.clone();

    let exit_code = match cli.command {
        Command::Download {
            source,
            output,
            no_dht,
            seed,
            port,
            quiet,
            workers,
            no_pin_cores,
            json, // T8: line-delimited JSON progress on stdout
            io_uring,
            direct_io,
            uring_sq_depth,
            mmap,
            iocp,
            api_port,
            api_bind,
            diagnose,
            max_peers,
            connect_timeout,
            data_timeout,
            choke_rotation,
            max_concurrent_connects,
            connect_soft_timeout,
            steal_threshold,
            min_pipeline_depth,
            max_pipeline_depth,
        } => {
            // Phase 6 will wire the full Figment pipeline (TOML + env + CLI
            // overrides).  For now, start from engine defaults.
            let mut settings = irontide::session::Settings::default();

            if workers != 0 {
                settings.runtime_worker_threads = workers;
            }
            if max_peers != 0 {
                settings.max_peers_per_torrent = max_peers;
            }
            if let Some(ct) = connect_timeout {
                settings.peer_connect_timeout = ct;
            }
            if let Some(dt) = data_timeout {
                settings.data_contribution_timeout_secs = dt;
            }
            if let Some(cr) = choke_rotation {
                settings.choke_rotation_max_evictions = cr;
            }
            if let Some(mc) = max_concurrent_connects {
                settings.max_concurrent_connects = mc;
            }
            if let Some(cst) = connect_soft_timeout {
                settings.connect_soft_timeout = cst;
            }
            if let Some(st) = steal_threshold {
                settings.steal_threshold_ratio = st;
            }
            if let Some(min_pd) = min_pipeline_depth {
                settings.min_pipeline_depth = min_pd;
            }
            if let Some(max_pd) = max_pipeline_depth {
                settings.max_pipeline_depth = max_pd;
            }
            if no_pin_cores {
                settings.pin_cores = false;
            }
            if io_uring || direct_io {
                settings.storage_mode = irontide::core::StorageMode::IoUring;
            }
            if mmap {
                settings.storage_mode = irontide::core::StorageMode::Mmap;
            }
            if direct_io {
                settings.io_uring_direct_io = true;
                // Only enable filesystem direct I/O when NOT using io_uring.
                // The io_uring backend handles O_DIRECT on its own pre-opened fds;
                // the inner PosixDiskIo's FilesystemStorage needs standard buffered
                // I/O for cache reads and hash verification.
                if settings.storage_mode != irontide::core::StorageMode::IoUring {
                    settings.filesystem_direct_io = true;
                }
            }
            if let Some(depth) = uring_sq_depth {
                settings.io_uring_sq_depth = depth;
                if !io_uring && !direct_io {
                    settings.storage_mode = irontide::core::StorageMode::IoUring;
                }
            }
            if iocp {
                settings.storage_mode = irontide::core::StorageMode::Iocp;
            }

            let rt = download::build_runtime(&settings);
            let result = rt.block_on(download::run(download::DownloadOpts {
                source: &source,
                output: &output,
                no_dht,
                seed,
                port,
                quiet,
                json,
                settings,
                api_port,
                api_bind,
                diagnose,
            }));

            // Force-shutdown the runtime like rqbit does — kills any dangling
            // tasks (peer connections, DHT, etc.) that would otherwise hang.
            rt.shutdown_timeout(Duration::from_secs(1));

            match result {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Command::Daemon {
            api_port,
            api_bind,
            download_dir,
            port,
            no_dht,
            workers,
            no_pin_cores,
        } => match daemon::run(daemon::DaemonOpts {
            api_port,
            api_bind,
            download_dir,
            port,
            no_dht,
            workers,
            no_pin_cores,
        }) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        Command::Create {
            path,
            output,
            tracker,
            private,
            piece_size,
        } => match create::run(&path, &output, &tracker, private, piece_size) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        Command::Info {
            source,
            files,
            peers,
            json,
        } => {
            // Disambiguate file path vs. daemon hash prefix.
            let path = std::path::Path::new(&source);
            if path.is_file() {
                if files || peers || json {
                    eprintln!(
                        "warning: --files/--peers/--json are ignored when inspecting a .torrent file"
                    );
                }
                match info::run(path) {
                    Ok(()) => 0,
                    Err(e) => {
                        eprintln!("error: {e}");
                        1
                    }
                }
            } else if is_hex_prefix(&source) {
                run_batch(api_url_flag.as_deref(), json, async |client, out| {
                    commands::cmd_info(client, &source, files, peers, out).await
                })
            } else {
                eprintln!("error: '{source}' is neither an existing file nor a valid hex prefix");
                1
            }
        }
        Command::Add { source, json } => {
            run_batch(api_url_flag.as_deref(), json, async |client, out| {
                commands::cmd_add(client, &source, out).await
            })
        }
        Command::List { json, filter } => {
            let args = ListArgs { filter };
            run_batch(api_url_flag.as_deref(), json, async |client, out| {
                commands::cmd_list(client, &args, out).await
            })
        }
        Command::Rm { hash, json } => {
            run_batch(api_url_flag.as_deref(), json, async |client, out| {
                commands::cmd_remove(client, &hash, out).await
            })
        }
        Command::Pause { hash, json } => {
            run_batch(api_url_flag.as_deref(), json, async |client, out| {
                commands::cmd_pause(client, &hash, out).await
            })
        }
        Command::Resume { hash, json } => {
            run_batch(api_url_flag.as_deref(), json, async |client, out| {
                commands::cmd_resume(client, &hash, out).await
            })
        }
        Command::Seed { hash, json } => {
            run_batch(api_url_flag.as_deref(), json, async |client, out| {
                commands::cmd_seed(client, &hash, true, out).await
            })
        }
        Command::Unseed { hash, json } => {
            run_batch(api_url_flag.as_deref(), json, async |client, out| {
                commands::cmd_seed(client, &hash, false, out).await
            })
        }
        Command::Shell => match repl::run(repl::ShellOpts {
            api_url: api_url_flag,
        }) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        Command::Tui => match tui::run(tui::TuiOpts {
            api_url: api_url_flag,
        }) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        Command::Config { action } => {
            // Placeholder — full implementation in Phase 4.
            match action {
                ConfigAction::Init { force } => {
                    eprintln!("config init (force={force}): not yet implemented");
                }
                ConfigAction::Path => {
                    let path = config::resolve_config_path(cli.config.as_deref());
                    println!("{}", path.display());
                }
                ConfigAction::Show => {
                    eprintln!("config show: not yet implemented");
                }
                ConfigAction::Validate { path } => {
                    eprintln!(
                        "config validate (path={:?}): not yet implemented",
                        path.as_deref().map(std::path::Path::display)
                    );
                }
            }
            0
        }
        Command::Completions { shell } => {
            // Placeholder — full implementation in Phase 5.
            eprintln!("completions ({shell}): not yet implemented");
            0
        }
    };

    std::process::exit(exit_code);
}

/// Whether `s` looks like a hex info-hash prefix the daemon might
/// accept: 2-40 lowercase ASCII hex chars.
fn is_hex_prefix(s: &str) -> bool {
    let len = s.len();
    (2..=40).contains(&len) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Dispatch a single batch-mode subcommand against the running daemon.
///
/// Builds a lightweight current-thread runtime, constructs the API
/// client, picks JSON or human output, and runs the supplied
/// async closure. All `CliError` variants map to exit codes via
/// `CliError::exit_code`; the human-readable error message is written
/// to stderr with a one-line `error: ...` prefix.
///
/// The closure signature threads `&mut Output<'_>` through the call so
/// the dispatcher owns the writer and can flush it before returning.
/// `async |...| { ... }` closures capture by reference, which means
/// borrows (e.g. `&hash`) stay valid for the lifetime of the async
/// invocation — no `'static` capture hoop required.
fn run_batch<F>(api_url_flag: Option<&str>, json: bool, op: F) -> i32
where
    F: for<'a> AsyncFnOnce(&'a ApiClient, &'a mut Output<'a>) -> Result<(), CliError>,
{
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: failed to build tokio runtime: {e}");
            return 1;
        }
    };

    let url = ApiClient::resolve_url(api_url_flag);
    let client = ApiClient::new(url);

    let stdout = std::io::stdout();
    let mut stdout_lock = stdout.lock();
    let exit_code = rt.block_on(async {
        let mut out = if json {
            Output::Json(&mut stdout_lock)
        } else {
            Output::Human(&mut stdout_lock)
        };
        match op(&client, &mut out).await {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("error: {err}");
                err.exit_code()
            }
        }
    });
    let _ = stdout_lock.flush();
    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse a Download command and return the max_peers field.
    fn parse_max_peers(args: &[&str]) -> usize {
        let cli = Cli::try_parse_from(args).expect("failed to parse args");
        match cli.command {
            Command::Download { max_peers, .. } => max_peers,
            _ => panic!("expected Download subcommand"),
        }
    }

    #[test]
    fn max_peers_flag_overrides_default() {
        let max_peers = parse_max_peers(&[
            "irontide",
            "download",
            "--max-peers",
            "64",
            "magnet:?xt=urn:btih:aabbccdd",
        ]);
        assert_eq!(max_peers, 64, "--max-peers 64 should parse as 64");

        // Verify the settings wire-up logic: non-zero value overrides default.
        let mut settings = irontide::session::Settings::default();
        if max_peers != 0 {
            settings.max_peers_per_torrent = max_peers;
        }
        assert_eq!(
            settings.max_peers_per_torrent, 64,
            "settings.max_peers_per_torrent should be 64 after wiring"
        );
    }

    #[test]
    fn max_peers_zero_uses_default() {
        let max_peers = parse_max_peers(&[
            "irontide",
            "download",
            "--max-peers",
            "0",
            "magnet:?xt=urn:btih:aabbccdd",
        ]);
        assert_eq!(max_peers, 0, "--max-peers 0 should parse as 0");

        // Verify the settings wire-up logic: zero is treated as "not specified",
        // so the settings default (128) is preserved.
        let mut settings = irontide::session::Settings::default();
        if max_peers != 0 {
            settings.max_peers_per_torrent = max_peers;
        }
        assert_eq!(
            settings.max_peers_per_torrent, 128,
            "--max-peers 0 should leave settings at the default (128)"
        );
    }
}
