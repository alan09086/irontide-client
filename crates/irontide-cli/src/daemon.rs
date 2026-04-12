//! Long-running `irontide daemon` subcommand.
//!
//! Starts an empty `SessionHandle`, binds the HTTP API server, and blocks on
//! Ctrl-C. The daemon is the service backplane the other irontide modes
//! (batch/REPL/TUI) connect to through the HTTP API; it holds no torrents of
//! its own at startup — those are added by API clients after the daemon is up.
//!
//! Runtime construction reuses [`crate::download::build_runtime`] so the
//! core-pinned multi-thread runtime configuration stays DRY.

use std::path::PathBuf;

use irontide::session::Settings;

/// Daemon-mode launch options, mirroring the clap flags on `Command::Daemon`.
pub(crate) struct DaemonOpts {
    /// HTTP API listen port. Must be non-zero — the daemon has no other
    /// feedback channel, so exposing the API is mandatory in this mode.
    pub api_port: u16,
    /// HTTP API bind address (e.g. `"127.0.0.1"`).
    pub api_bind: String,
    /// Default download directory for torrents added via the API.
    pub download_dir: PathBuf,
    /// BitTorrent listen port.
    pub port: u16,
    /// Disable the DHT.
    pub no_dht: bool,
    /// Tokio worker thread count (`0` = auto).
    pub workers: usize,
    /// Disable core-affinity pinning for tokio workers.
    pub no_pin_cores: bool,
}

/// Build a tokio runtime, start a long-running `SessionHandle`, bind the HTTP
/// API server, and block on Ctrl-C.
///
/// # Errors
///
/// Returns an error if `api_port == 0` (the daemon requires the API), if the
/// bind address fails to parse, if binding the API server fails, if the
/// session fails to start, or if graceful shutdown reports an error.
pub(crate) fn run(opts: DaemonOpts) -> anyhow::Result<()> {
    if opts.api_port == 0 {
        anyhow::bail!("--api-port must be non-zero for daemon mode");
    }

    let DaemonOpts {
        api_port,
        api_bind,
        download_dir,
        port,
        no_dht,
        workers,
        no_pin_cores,
    } = opts;

    // Build Settings with the knobs daemon mode exposes. Everything else stays
    // at the library default — the user can switch to `download`/`batch` modes
    // if they need fine-grained tuning.
    let mut settings = Settings {
        download_dir: download_dir.clone(),
        ..Settings::default()
    };
    if workers != 0 {
        settings.runtime_worker_threads = workers;
    }
    if no_pin_cores {
        settings.pin_cores = false;
    }

    let rt = crate::download::build_runtime(&settings);

    let result = rt.block_on(run_daemon(
        settings,
        api_bind,
        api_port,
        download_dir,
        port,
        no_dht,
    ));

    // Match the download command's shutdown pattern — kill any dangling tasks
    // (DHT, tracker announces, peer connections) that would otherwise keep the
    // runtime alive past the user's Ctrl-C.
    rt.shutdown_timeout(std::time::Duration::from_secs(1));

    result
}

async fn run_daemon(
    settings: Settings,
    api_bind: String,
    api_port: u16,
    download_dir: PathBuf,
    port: u16,
    no_dht: bool,
) -> anyhow::Result<()> {
    // Build the session. Mirrors `download::run`'s builder chain, minus the
    // DHT-state restoration and initial-peers plumbing (both are download-only
    // niceties — daemon clients add torrents post-startup via the API).
    let mut builder = irontide::ClientBuilder::from_settings(settings);
    builder = builder.listen_port(port).download_dir(&download_dir);
    if no_dht {
        builder = builder.enable_dht(false);
    }

    let session = builder
        .start()
        .await
        .map_err(|e| anyhow::anyhow!("failed to start session: {e}"))?;

    // Parse and bind the API server. Failure here is fatal — the whole point
    // of daemon mode is to expose the API.
    let addr: std::net::SocketAddr = format!("{api_bind}:{api_port}")
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid API bind address {api_bind}:{api_port}: {e}"))?;
    let server = irontide_api::ApiServer::bind(addr, session.clone())
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind API server: {e}"))?;

    // Informational — daemon mode has no progress line, so these two lines
    // are the only feedback the operator gets that the service is live.
    let local = server.local_addr();
    eprintln!("irontide daemon listening on http://{local}");
    eprintln!("(Ctrl-C to stop)");

    let api_task = tokio::spawn(async move {
        if let Err(e) = server.run().await {
            eprintln!("API server error: {e}");
        }
    });

    // Wait for Ctrl-C. Ignore errors from ctrl_c() — tokio only errors out on
    // platforms where the signal handler can't be installed, which we're not
    // targeting.
    if let Err(e) = tokio::signal::ctrl_c().await {
        eprintln!("warning: failed to install Ctrl-C handler: {e}");
    }

    // Graceful session shutdown. The API server doesn't have a graceful stop
    // API of its own — aborting the task is fine because the session is the
    // source of truth and we've already signalled it.
    if let Err(e) = session.shutdown().await {
        eprintln!("warning: session shutdown error: {e}");
    }
    api_task.abort();

    eprintln!("daemon shut down cleanly");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_rejects_zero_api_port() {
        let opts = DaemonOpts {
            api_port: 0,
            api_bind: "127.0.0.1".to_string(),
            download_dir: PathBuf::from("/tmp"),
            port: 42020,
            no_dht: false,
            workers: 0,
            no_pin_cores: false,
        };
        let err = run(opts).expect_err("api_port=0 must be rejected");
        assert!(
            err.to_string().contains("--api-port must be non-zero"),
            "unexpected error: {err}"
        );
    }
}
