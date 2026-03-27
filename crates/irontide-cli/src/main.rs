mod create;
mod download;
mod info;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "irontide", version, about = "BitTorrent client")]
struct Cli {
    /// Log level (error, warn, info, debug, trace)
    #[arg(short, long, default_value = "error")]
    log_level: String,

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
        /// Overwrite existing files
        #[arg(long)]
        overwrite: bool,
        /// Only list torrent contents, don't download
        #[arg(short, long)]
        list: bool,
        /// Initial peers to connect to (host:port)
        #[arg(long)]
        initial_peers: Vec<String>,
        /// Disable tracker announces
        #[arg(long)]
        disable_trackers: bool,
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
    /// Display torrent file information
    Info {
        /// Path to .torrent file
        path: PathBuf,
    },
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
            overwrite: _,
            list: _,
            initial_peers: _,
            disable_trackers: _,
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
        Command::Info { path } => match info::run(&path) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
    };

    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;

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
