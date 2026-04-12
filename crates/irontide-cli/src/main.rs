mod client;
mod commands;
mod create;
mod daemon;
mod download;
mod error;
mod format;
mod info;
mod progress;

use clap::{Parser, Subcommand};
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use client::ApiClient;
use commands::{ListArgs, Output};
use error::CliError;

#[derive(Parser)]
#[command(name = "irontide", version, about = "BitTorrent client")]
struct Cli {
    /// Log level (error, warn, info, debug, trace)
    #[arg(short, long, default_value = "error")]
    log_level: String,

    /// URL of the irontide daemon HTTP API
    /// (default: $IRONTIDE_API_URL or http://127.0.0.1:9080)
    #[arg(long, global = true)]
    api_url: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download a torrent from a magnet link or .torrent file
    Download {
        /// Magnet URI or path to .torrent file
        source: String,
        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        /// Disable DHT
        #[arg(long)]
        no_dht: bool,
        /// Path to JSON settings file
        #[arg(long)]
        config: Option<PathBuf>,
        /// Seed after completion instead of exiting
        #[arg(long)]
        seed: bool,
        /// Listen port
        #[arg(short, long, default_value = "42020")]
        port: u16,
        /// Quiet mode — suppress progress output
        #[arg(short, long)]
        quiet: bool,
        /// Number of tokio worker threads (0 = auto)
        #[arg(long, default_value_t = 0)]
        workers: usize,
        /// Disable core affinity pinning
        #[arg(long)]
        no_pin_cores: bool,
        /// Emit line-delimited JSON progress ticks (reserved for T8;
        /// parsed today but has no behavioural effect yet)
        #[arg(long)]
        json: bool,
        /// Use io_uring for disk writes (Linux only, requires io-uring feature)
        #[arg(long)]
        io_uring: bool,
        /// Enable O_DIRECT for io_uring writes (implies --io-uring)
        #[arg(long)]
        direct_io: bool,
        /// io_uring submission queue depth (default: 256)
        #[arg(long)]
        uring_sq_depth: Option<u32>,
        /// Use memory-mapped I/O (mmap) for disk operations
        #[arg(long)]
        mmap: bool,
        /// Use IOCP for disk I/O (Windows only, requires iocp feature)
        #[arg(long)]
        iocp: bool,
        /// HTTP API port (0 = disabled)
        #[arg(long, default_value_t = 0)]
        api_port: u16,
        /// HTTP API bind address
        #[arg(long, default_value = "127.0.0.1")]
        api_bind: String,
        /// Enable pipeline diagnostics (detailed per-peer stats every 5s)
        #[arg(long)]
        diagnose: bool,
        /// Maximum peer connections per torrent (0 = use default)
        #[arg(long, default_value_t = 0)]
        max_peers: usize,
        /// TCP connect timeout in seconds (default: 10)
        #[arg(long)]
        connect_timeout: Option<u64>,
        /// Data contribution timeout in seconds (default: 0 = disabled)
        #[arg(long)]
        data_timeout: Option<u64>,
        /// Max choke rotation evictions per tick (default: 0 = disabled)
        #[arg(long)]
        choke_rotation: Option<u32>,
        /// Maximum concurrent outbound connects (M147: ConnectPool size)
        #[arg(long)]
        max_concurrent_connects: Option<u16>,
        /// Seconds without TCP SYN-ACK before soft reap disconnects (default: 3)
        #[arg(long)]
        connect_soft_timeout: Option<u64>,
        /// Piece steal threshold multiplier (default: 10.0)
        #[arg(long)]
        steal_threshold: Option<f64>,
        /// Minimum per-peer pipeline depth (default: 16)
        #[arg(long)]
        min_pipeline_depth: Option<u32>,
        /// Maximum per-peer pipeline depth (default: 512)
        #[arg(long)]
        max_pipeline_depth: Option<u32>,
    },
    /// Run a long-running daemon that exposes the HTTP API
    Daemon {
        /// HTTP API port (required — daemon mode has no other feedback channel)
        #[arg(long, default_value_t = 9080)]
        api_port: u16,
        /// HTTP API bind address
        #[arg(long, default_value = "127.0.0.1")]
        api_bind: String,
        /// Default download directory for torrents added via the API
        #[arg(long, default_value = ".")]
        download_dir: PathBuf,
        /// BitTorrent listen port
        #[arg(short, long, default_value_t = 42020)]
        port: u16,
        /// Disable DHT
        #[arg(long)]
        no_dht: bool,
        /// Number of tokio worker threads (0 = auto)
        #[arg(long, default_value_t = 0)]
        workers: usize,
        /// Disable core affinity pinning
        #[arg(long)]
        no_pin_cores: bool,
    },
    /// Create a .torrent file
    Create {
        /// Path to file or directory
        path: PathBuf,
        /// Output .torrent file path
        #[arg(short, long, default_value = "output.torrent")]
        output: PathBuf,
        /// Tracker URL(s) — can specify multiple: -t url1 -t url2
        #[arg(short, long)]
        tracker: Vec<String>,
        /// Create as private torrent
        #[arg(long)]
        private: bool,
        /// Piece size in KiB (auto-selected if omitted)
        #[arg(long)]
        piece_size: Option<u64>,
    },
    /// Display torrent details — either a .torrent file path or a torrent
    /// hash in the running daemon. If the argument looks like a lowercase
    /// hex prefix (2-40 chars) AND the path does not exist on disk, it is
    /// dispatched to the daemon; otherwise it is treated as a .torrent file.
    Info {
        /// Path to .torrent file OR info-hash prefix of a daemon torrent
        source: String,
        /// Show the file list (daemon mode only)
        #[arg(long)]
        files: bool,
        /// Show the peer table (daemon mode only)
        #[arg(long)]
        peers: bool,
        /// Emit JSON instead of human-readable output (daemon mode only)
        #[arg(long)]
        json: bool,
    },
    /// Add a torrent to the running daemon (magnet URI or .torrent file path)
    Add {
        /// Magnet URI or path to .torrent file
        source: String,
        /// Emit JSON instead of human-readable output
        #[arg(long)]
        json: bool,
    },
    /// List torrents in the running daemon
    List {
        /// Emit JSON instead of a human-readable table
        #[arg(long)]
        json: bool,
        /// Filter by state: downloading, seeding, paused
        #[arg(long)]
        filter: Option<String>,
    },
    /// Remove a torrent from the running daemon
    Rm {
        /// Torrent info hash (or unique prefix)
        hash: String,
        /// Emit JSON instead of a confirmation line
        #[arg(long)]
        json: bool,
    },
    /// Pause an active torrent (both upload and download)
    Pause {
        /// Torrent info hash (or unique prefix)
        hash: String,
        /// Emit JSON instead of a confirmation line
        #[arg(long)]
        json: bool,
    },
    /// Resume a paused torrent
    Resume {
        /// Torrent info hash (or unique prefix)
        hash: String,
        /// Emit JSON instead of a confirmation line
        #[arg(long)]
        json: bool,
    },
    /// Flip an active torrent to seed-only mode (keep uploading, stop downloading)
    Seed {
        /// Torrent info hash (or unique prefix)
        hash: String,
        /// Emit JSON instead of a confirmation line
        #[arg(long)]
        json: bool,
    },
    /// Clear seed-only mode and resume downloading
    Unseed {
        /// Torrent info hash (or unique prefix)
        hash: String,
        /// Emit JSON instead of a confirmation line
        #[arg(long)]
        json: bool,
    },
    // `Settings` subcommand deferred to a future milestone (M159 scope trim).
}

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
            config,
            seed,
            port,
            quiet,
            workers,
            no_pin_cores,
            json: _json, // json flag is parsed for forward compatibility; T8 implements the behaviour
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
            let mut settings = if let Some(ref config_path) = config {
                let data = std::fs::read_to_string(config_path).unwrap_or_else(|e| {
                    eprintln!(
                        "error: failed to read config {}: {e}",
                        config_path.display()
                    );
                    std::process::exit(1);
                });
                let s: irontide::session::Settings =
                    serde_json::from_str(&data).unwrap_or_else(|e| {
                        eprintln!("error: failed to parse settings JSON: {e}");
                        std::process::exit(1);
                    });
                if let Err(e) = s.validate() {
                    eprintln!("error: invalid settings: {e}");
                    std::process::exit(1);
                }
                s
            } else {
                irontide::session::Settings::default()
            };

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
