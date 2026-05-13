// CLI definition shared between the main binary and build.rs.
//
// **Constraint**: this file is included via `include!("src/cli_def.rs")` in
// `build.rs` for shell completion generation.  It must NOT use `use crate::`
// imports — only external crate imports (`clap`, `clap_complete`, `std`).
//
// Uses regular comments (not `//!`) because `include!()` in build.rs places
// this code in a non-crate-root position where inner doc comments are invalid.

use clap::{Parser, Subcommand};

/// `BitTorrent` client
#[derive(Parser)]
#[command(name = "irontide", version, about = "BitTorrent client")]
pub(crate) struct Cli {
    /// Log level (error, warn, info, debug, trace)
    #[arg(short, long, default_value = "error")]
    pub log_level: String,

    /// URL of the irontide daemon HTTP API
    /// (default: $`IRONTIDE_API_URL` or <http://127.0.0.1:9080>)
    #[arg(long, global = true)]
    pub api_url: Option<String>,

    /// Path to TOML configuration file
    #[arg(long, global = true)]
    pub config: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Download a torrent from a magnet link or .torrent file
    Download {
        /// Magnet URI or path to .torrent file
        source: String,
        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: std::path::PathBuf,
        /// Disable DHT
        #[arg(long)]
        no_dht: bool,
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
        /// Use `io_uring` for disk writes (Linux only, requires io-uring feature)
        #[arg(long)]
        io_uring: bool,
        /// Enable `O_DIRECT` for `io_uring` writes (implies --io-uring)
        #[arg(long)]
        direct_io: bool,
        /// `io_uring` submission queue depth (default: 256)
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
        /// Enable diagnostic counters (dispatch timing, backpressure, peer telemetry)
        #[arg(long)]
        diagnostics: bool,
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
        /// Maximum concurrent outbound connects (M147: `ConnectPool` size)
        #[arg(long)]
        max_concurrent_connects: Option<u16>,
        /// Seconds without TCP SYN-ACK before soft reap disconnects (default: 3)
        #[arg(long)]
        connect_soft_timeout: Option<u64>,
        /// Piece steal threshold multiplier (default: 10.0)
        #[arg(long)]
        steal_threshold: Option<f64>,
        /// Use per-peer CAS dispatch instead of actor-centralised dispatch (A/B benchmarking)
        #[arg(long)]
        no_actor_dispatch: bool,
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
        download_dir: std::path::PathBuf,
        /// `BitTorrent` listen port
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
        /// Directory for per-torrent resume files (default: ~/.local/state/irontide)
        #[arg(long)]
        resume_dir: Option<std::path::PathBuf>,
        /// Enable diagnostic counters (dispatch timing, backpressure, peer telemetry)
        #[arg(long)]
        diagnostics: bool,
    },
    /// Create a .torrent file
    Create {
        /// Path to file or directory
        path: std::path::PathBuf,
        /// Output .torrent file path
        #[arg(short, long, default_value = "output.torrent")]
        output: std::path::PathBuf,
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
    /// Open an interactive REPL shell against a running daemon
    Shell,
    /// Launch the full-screen TUI dashboard against a running daemon
    Tui,
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
}

/// Configuration management subcommands.
#[derive(Subcommand)]
pub(crate) enum ConfigAction {
    /// Create a default configuration file
    Init {
        /// Overwrite existing config file
        #[arg(long)]
        force: bool,
    },
    /// Print the resolved config file path
    Path,
    /// Print the merged configuration (defaults + file + env)
    Show,
    /// Validate a configuration file
    Validate {
        /// Path to validate (uses default path if omitted)
        path: Option<std::path::PathBuf>,
    },
}
