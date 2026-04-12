use anyhow::Context;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use irontide::core::{DEFAULT_CHUNK_SIZE, Lengths, TorrentMeta};
use irontide::session::SessionState;
use irontide::storage::{FilesystemStorage, PreallocateMode, TorrentStorage};

use crate::format::{format_rate, format_size};

pub struct DownloadOpts<'a> {
    pub source: &'a str,
    pub output: &'a Path,
    pub no_dht: bool,
    pub seed: bool,
    pub port: u16,
    pub quiet: bool,
    pub settings: irontide::session::Settings,
    pub api_port: u16,
    pub api_bind: String,
    pub diagnose: bool,
}

pub async fn run(opts: DownloadOpts<'_>) -> anyhow::Result<()> {
    let DownloadOpts {
        source,
        output,
        no_dht,
        seed,
        port,
        quiet,
        settings,
        api_port,
        api_bind,
        diagnose,
    } = opts;

    let state_path = state_file_path();

    // Build session
    let mut builder = irontide::ClientBuilder::from_settings(settings);
    builder = builder.listen_port(port).download_dir(output);

    if no_dht {
        builder = builder.enable_dht(false);
    }

    if let Some((saved_nodes, saved_node_id)) = load_dht_state(&state_path) {
        if !quiet {
            eprintln!(
                "Loaded {} saved DHT nodes from previous session",
                saved_nodes.len()
            );
        }
        builder = builder.dht_saved_nodes(saved_nodes);
        if let Some(id) = saved_node_id {
            builder = builder.dht_node_id(id);
        }
    }

    let session = builder.start().await?;
    let mut alerts = session.subscribe();

    // Start HTTP API if configured
    let _api_handle = if api_port > 0 {
        let addr: std::net::SocketAddr = format!("{api_bind}:{api_port}")
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid API bind address: {e}"))?;
        let server = irontide_api::ApiServer::bind(addr, session.clone())
            .await
            .map_err(|e| anyhow::anyhow!("failed to bind API server: {e}"))?;
        if !quiet {
            eprintln!("API listening on http://{}", server.local_addr());
        }
        let handle = tokio::spawn(async move {
            if let Err(e) = server.run().await {
                eprintln!("API server error: {e}");
            }
        });
        Some(handle)
    } else {
        None
    };

    // Add torrent
    let info_hash = if source.starts_with("magnet:") {
        let magnet = irontide::core::Magnet::parse(source)
            .map_err(|e| anyhow::anyhow!("invalid magnet URI: {e}"))?;
        if let Some(ref name) = magnet.display_name
            && !quiet
        {
            eprintln!("Adding: {name}");
        }
        session.add_magnet(magnet).await?
    } else {
        let data = std::fs::read(source)
            .with_context(|| format!("failed to read torrent file: {source}"))?;
        let meta = irontide::core::torrent_from_bytes_any(&data)
            .map_err(|e| anyhow::anyhow!("failed to parse torrent: {e}"))?;
        let ih = meta.info_hashes().best_v1();
        if !quiet {
            let name = meta
                .as_v1()
                .map(|v| v.info.name.as_str())
                .or_else(|| meta.as_v2().map(|v| v.info.name.as_str()))
                .unwrap_or("unknown");
            eprintln!("Adding: {name}");
        }
        let storage = make_filesystem_storage(&meta, output)?;
        session.add_torrent(meta, Some(storage)).await?;
        ih
    };

    // --- Signal handling (rqbit-style 2-tier shutdown) ---
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let cancelled = cancelled.clone();
        let force_quit = Arc::new(AtomicBool::new(false));
        let fq = force_quit.clone();

        tokio::spawn(async move {
            // Register both SIGINT and SIGTERM
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("failed to register SIGTERM handler");

            // First signal: graceful shutdown
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = sigterm.recv() => {},
            }
            cancelled.store(true, Ordering::SeqCst);

            // Spawn force-quit watchdog: if second signal arrives within 5s, exit immediately
            let fq2 = fq.clone();
            tokio::spawn(async move {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = tokio::time::sleep(Duration::from_secs(5)) => return,
                }
                fq2.store(true, Ordering::SeqCst);
                std::process::exit(1);
            });
        });
    }

    // --- Progress loop ---
    let start_time = Instant::now();
    let mut finished = false;
    let mut peak_peers: usize = 0;
    let mut total_bytes: u64 = 0;
    let mut total_wanted: u64 = 0;
    let mut last_save = Instant::now();
    let mut last_diagnose = Instant::now();
    const SAVE_INTERVAL: Duration = Duration::from_secs(60);
    const POLL_INTERVAL: Duration = Duration::from_secs(1);
    const DIAGNOSE_INTERVAL: Duration = Duration::from_secs(5);

    loop {
        // Check for cancellation
        if cancelled.load(Ordering::SeqCst) {
            if !quiet {
                eprintln!();
            }
            break;
        }

        // Drain alerts
        while let Ok(alert) = alerts.try_recv() {
            if let irontide::session::AlertKind::TorrentFinished { info_hash: ih } = alert.kind
                && ih == info_hash
            {
                finished = true;
            }
        }

        // Update stats
        if let Ok(stats) = session.torrent_stats(info_hash).await {
            peak_peers = peak_peers.max(stats.peers_connected);
            total_bytes = stats.total_done;
            total_wanted = stats.total_wanted;

            // Only treat as finished when we actually have data to download
            // and it's all done. Before magnet metadata resolves, total_wanted
            // is 0 and progress is 1.0 — that's not "finished".
            if stats.progress >= 1.0 && stats.total_wanted > 0 {
                finished = true;
            }

            if !quiet && !finished {
                let pct = stats.progress * 100.0;
                let done = format_size(stats.total_done);
                let total = format_size(stats.total_wanted);
                let down = format_rate(stats.download_rate);
                let up = format_rate(stats.upload_rate);
                let peers = stats.peers_connected;
                let elapsed = start_time.elapsed().as_secs_f64();
                let eta = if stats.download_rate > 0 && stats.total_wanted > stats.total_done {
                    let remaining = stats.total_wanted - stats.total_done;
                    format!("{:.0}s", remaining as f64 / stats.download_rate as f64)
                } else if finished {
                    "done".to_string()
                } else {
                    "---".to_string()
                };

                // M137: Show pipeline stats instead of bare peer count
                let pipeline_info = if let Some(ref p) = stats.pipeline {
                    format!(
                        "{{live: {}, connecting: {}, queued: {}, dead: {}, known: {}}}",
                        p.live, p.connecting, p.queued, p.dead, p.known
                    )
                } else {
                    format!("{peers} peers")
                };
                eprint!(
                    "\r\x1b[2K{pct:5.1}% ({done}/{total}) \u{2193}{down} \u{2191}{up} | {pipeline_info} | ETA {eta} [{elapsed:.0}s]",
                );
            }

            // Pipeline diagnostics (every 5s when --diagnose enabled)
            if diagnose && !finished && last_diagnose.elapsed() >= DIAGNOSE_INTERVAL {
                last_diagnose = Instant::now();
                if let Ok(peers) = session.get_peer_info(info_hash).await {
                    let stats = session.torrent_stats(info_hash).await.ok();
                    let pipeline = stats.as_ref().and_then(|s| s.pipeline);
                    let choke_rotations = stats.map(|s| s.choke_rotations).unwrap_or(0);
                    print_pipeline_diagnostics(
                        &peers,
                        start_time.elapsed(),
                        pipeline,
                        choke_rotations,
                    );
                }
            }
        }

        if finished {
            let elapsed = start_time.elapsed();
            let elapsed_secs = elapsed.as_secs_f64();
            let avg_speed = if elapsed_secs > 0.0 {
                total_bytes as f64 / elapsed_secs / 1_048_576.0
            } else {
                0.0
            };

            if !quiet {
                eprintln!(
                    "\r\x1b[2KDownloaded {} in {:.1}s ({:.1} MB/s avg, {} peers)",
                    format_size(total_wanted),
                    elapsed_secs,
                    avg_speed,
                    peak_peers,
                );
            }

            if diagnose && let Ok(peers) = session.get_peer_info(info_hash).await {
                let stats_snapshot = session.torrent_stats(info_hash).await.ok();
                let unique_attempted = stats_snapshot
                    .as_ref()
                    .map(|s| s.unique_peers_attempted)
                    .unwrap_or(0);
                let choke_rotations = stats_snapshot
                    .as_ref()
                    .map(|s| s.choke_rotations)
                    .unwrap_or(0);
                let pipeline = stats_snapshot.and_then(|s| s.pipeline);
                print_final_summary(
                    &peers,
                    peak_peers,
                    unique_attempted,
                    pipeline,
                    choke_rotations,
                );
            }

            if seed {
                if !quiet {
                    eprintln!("Seeding... press Ctrl-C to stop");
                }
                // Wait for cancellation
                while !cancelled.load(Ordering::SeqCst) {
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }

            break;
        }

        tokio::time::sleep(POLL_INTERVAL).await;

        if last_save.elapsed() >= SAVE_INTERVAL {
            save_session_state(&session, &state_path, false).await;
            last_save = Instant::now();
        }
    }

    // --- Shutdown ---
    save_session_state(&session, &state_path, !quiet).await;
    session.shutdown().await?;

    Ok(())
}

fn print_pipeline_diagnostics(
    peers: &[irontide::session::PeerInfo],
    elapsed: Duration,
    pipeline: Option<irontide::session::PeerPipelineSnapshot>,
    choke_rotations: u64,
) {
    let total = peers.len();
    let unchoked = peers.iter().filter(|p| !p.peer_choking).count();
    let downloading = peers
        .iter()
        .filter(|p| !p.peer_choking && p.download_rate > 0)
        .count();
    let choked = total.saturating_sub(unchoked);

    // Aggregate pipeline stats
    let total_pending: usize = peers.iter().map(|p| p.num_pending_requests).sum();
    let total_dl_rate: u64 = peers.iter().map(|p| p.download_rate).sum();

    // Per-peer throughput distribution (unchoked peers, in MB/s)
    let mut rates: Vec<f64> = peers
        .iter()
        .filter(|p| !p.peer_choking)
        .map(|p| p.download_rate as f64 / 1_048_576.0)
        .collect();
    rates.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    // Throughput buckets
    let bucket_0 = rates.iter().filter(|&&r| r == 0.0).count();
    let bucket_low = rates.iter().filter(|&&r| r > 0.0 && r < 0.1).count();
    let bucket_mid = rates.iter().filter(|&&r| (0.1..0.5).contains(&r)).count();
    let bucket_high = rates.iter().filter(|&&r| (0.5..1.0).contains(&r)).count();
    let bucket_top = rates.iter().filter(|&&r| r >= 1.0).count();

    // Top 10 peers by throughput
    let mut top10: Vec<_> = peers.iter().filter(|p| p.download_rate > 0).collect();
    top10.sort_by(|a, b| b.download_rate.cmp(&a.download_rate));
    top10.truncate(10);

    let choke_pct = if total > 0 {
        choked as f64 / total as f64 * 100.0
    } else {
        0.0
    };
    let per_peer_avg = if unchoked > 0 {
        total_dl_rate as f64 / unchoked as f64 / 1_048_576.0
    } else {
        0.0
    };

    eprintln!(
        "\n\x1b[1;36m-- Pipeline Diagnostics ({:.0}s elapsed) ------------------\x1b[0m",
        elapsed.as_secs_f64()
    );
    // M137: Pipeline lifecycle stats
    if let Some(ref p) = pipeline {
        eprintln!(
            "  Pipeline: {{live: {}, connecting: {}, queued: {}, dead: {}, known: {}}}",
            p.live, p.connecting, p.queued, p.dead, p.known
        );
    }
    eprintln!("  Choke rotations: {choke_rotations}");
    eprintln!(
        "  Peers: {} total | {} unchoked | {} downloading | {} choked ({:.0}%)",
        total, unchoked, downloading, choked, choke_pct,
    );
    eprintln!(
        "  Pipeline: {} in-flight requests | {}/s aggregate",
        total_pending,
        format_rate(total_dl_rate),
    );
    eprintln!("  Per-peer avg: {per_peer_avg:.2} MB/s ({unchoked} unchoked peers)");
    eprintln!("  Throughput buckets (unchoked peers):");
    eprintln!(
        "    0 MB/s:       {:3}  |  0.1-0.5 MB/s: {:3}",
        bucket_0, bucket_mid,
    );
    eprintln!(
        "    0-0.1 MB/s:   {:3}  |  0.5-1.0 MB/s: {:3}",
        bucket_low, bucket_high,
    );
    eprintln!("    >=1.0 MB/s:   {:3}", bucket_top);

    if !top10.is_empty() {
        eprintln!("  Top peers:");
        for p in &top10 {
            let choke_info = if p.peer_choking {
                "CHOKED".to_owned()
            } else {
                "OK".to_owned()
            };
            eprintln!(
                "    {:22} {:>7}/s | {:3} pending | {:5}s connected | {}",
                p.addr.to_string(),
                format_rate(p.download_rate),
                p.num_pending_requests,
                p.connected_duration_secs,
                choke_info,
            );
        }
    }

    // Pipeline health warnings
    if unchoked > 0 && total_pending < unchoked.saturating_mul(10) {
        let avg_pending = total_pending as f64 / unchoked as f64;
        eprintln!(
            "  \x1b[1;33mWARN: LOW PIPELINE: {} in-flight for {} unchoked peers (avg {:.1}/peer, expected ~128)\x1b[0m",
            total_pending, unchoked, avg_pending,
        );
    }
    if total > 0 && choked as f64 / total as f64 > 0.8 {
        eprintln!(
            "  \x1b[1;33mWARN: HIGH CHOKE RATIO: {:.0}% of peers are choking us\x1b[0m",
            choked as f64 / total as f64 * 100.0,
        );
    }
    if downloading == 0 && total > 0 {
        eprintln!("  \x1b[1;31mERR: NO PEERS DOWNLOADING -- pipeline is empty\x1b[0m");
    }
    let idle_unchoked = unchoked.saturating_sub(downloading);
    if idle_unchoked > unchoked / 2 && unchoked > 5 {
        eprintln!(
            "  \x1b[1;33mWARN: {idle_unchoked}/{unchoked} unchoked peers have zero throughput\x1b[0m",
        );
    }
    eprintln!("\x1b[1;36m---------------------------------------------------------\x1b[0m");
}

fn print_final_summary(
    peers: &[irontide::session::PeerInfo],
    peak_peers: usize,
    unique_peers_attempted: u64,
    pipeline: Option<irontide::session::PeerPipelineSnapshot>,
    choke_rotations: u64,
) {
    eprintln!("\n\x1b[1;36m-- Final Pipeline Summary --------------------------------\x1b[0m");
    let total_peers = peers.len();
    let contributing = peers.iter().filter(|p| p.download_rate > 0).count();
    eprintln!("  Total peers seen: {total_peers}");
    if let Some(ref p) = pipeline {
        eprintln!(
            "  Pipeline: {{live: {}, connecting: {}, queued: {}, dead: {}, known: {}}}",
            p.live, p.connecting, p.queued, p.dead, p.known
        );
    } else {
        eprintln!("  Unique peers attempted: {unique_peers_attempted}");
    }
    eprintln!("  Peak concurrent: {peak_peers}");
    eprintln!("  Choke rotations: {choke_rotations}");
    eprintln!("  Contributing peers (had throughput): {contributing}");

    let mut active: Vec<_> = peers.iter().filter(|p| p.download_rate > 0).collect();
    active.sort_by(|a, b| b.download_rate.cmp(&a.download_rate));

    if !active.is_empty() {
        eprintln!("  Active peers at completion:");
        for p in &active {
            eprintln!(
                "    {:22} {:>7}/s | {:3} pending | {}s",
                p.addr.to_string(),
                format_rate(p.download_rate),
                p.num_pending_requests,
                p.connected_duration_secs,
            );
        }
    }
    eprintln!("\x1b[1;36m---------------------------------------------------------\x1b[0m");
}

fn make_filesystem_storage(
    meta: &TorrentMeta,
    output: &Path,
) -> anyhow::Result<Arc<dyn TorrentStorage>> {
    let (file_paths, file_lengths, total_length, piece_length) = if let Some(v1) = meta.as_v1() {
        let files = v1.info.files();
        let paths: Vec<PathBuf> = files
            .iter()
            .map(|f| f.path.iter().collect::<PathBuf>())
            .collect();
        let lengths: Vec<u64> = files.iter().map(|f| f.length).collect();
        (paths, lengths, v1.info.total_length(), v1.info.piece_length)
    } else if let Some(v2) = meta.as_v2() {
        let files = v2.info.files();
        let paths: Vec<PathBuf> = files
            .iter()
            .map(|f| f.path.iter().collect::<PathBuf>())
            .collect();
        let lengths: Vec<u64> = files.iter().map(|f| f.attr.length).collect();
        (paths, lengths, v2.info.total_length(), v2.info.piece_length)
    } else {
        anyhow::bail!("torrent has no file metadata");
    };

    let lengths_calc = Lengths::new(total_length, piece_length, DEFAULT_CHUNK_SIZE);
    let storage = FilesystemStorage::new(
        output,
        file_paths,
        file_lengths,
        lengths_calc,
        None,
        PreallocateMode::None,
        false,
    )
    .map_err(|e| anyhow::anyhow!("failed to create storage: {e}"))?;
    Ok(Arc::new(storage))
}

fn state_file_path() -> PathBuf {
    let dir = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
        .join("torrent");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("session.dat")
}

fn load_dht_state(state_path: &Path) -> Option<(Vec<String>, Option<irontide::core::Id20>)> {
    let data = std::fs::read(state_path).ok()?;
    let state: SessionState = irontide::bencode::from_bytes(&data).ok()?;
    if state.dht_nodes.is_empty() {
        return None;
    }
    let nodes: Vec<String> = state
        .dht_nodes
        .iter()
        .map(|entry| format!("{}:{}", entry.host, entry.port))
        .collect();
    let node_id = state
        .dht_node_id
        .and_then(|hex| irontide::core::Id20::from_hex(&hex).ok());
    Some((nodes, node_id))
}

pub(crate) fn build_runtime(settings: &irontide::session::Settings) -> tokio::runtime::Runtime {
    let worker_count = if settings.runtime_worker_threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4)
    } else {
        settings.runtime_worker_threads
    };

    let pin = settings.pin_cores;
    let core_ids = if pin {
        core_affinity::get_core_ids().unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.worker_threads(worker_count);
    builder.enable_all();

    if pin && !core_ids.is_empty() {
        let core_ids = std::sync::Arc::new(core_ids);
        let counter = std::sync::Arc::new(AtomicUsize::new(0));
        builder.on_thread_start(move || {
            let idx = counter.fetch_add(1, Ordering::Relaxed);
            let core = core_ids[idx % core_ids.len()];
            if !core_affinity::set_for_current(core) {
                eprintln!("warning: failed to set core affinity for worker {idx}");
            }
        });
    }

    builder.build().expect("failed to build tokio runtime")
}

async fn save_session_state(
    session: &irontide::session::SessionHandle,
    state_path: &Path,
    announce: bool,
) {
    match session.save_session_state().await {
        Ok(state) => {
            if announce && !state.dht_nodes.is_empty() {
                eprintln!(
                    "Saving {} DHT nodes for next session",
                    state.dht_nodes.len()
                );
            }
            match irontide::bencode::to_bytes(&state) {
                Ok(bytes) => {
                    let tmp_path = state_path.with_extension("dat.tmp");
                    if let Err(e) = std::fs::write(&tmp_path, &bytes) {
                        eprintln!("warning: failed to write state file: {e}");
                    } else if let Err(e) = std::fs::rename(&tmp_path, state_path) {
                        eprintln!("warning: failed to rename state file: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("warning: failed to encode session state: {e}");
                }
            }
        }
        Err(e) => {
            eprintln!("warning: failed to save session state: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_affinity_available() {
        let core_ids = core_affinity::get_core_ids();
        assert!(
            core_ids.as_ref().is_some_and(|ids| !ids.is_empty()),
            "expected get_core_ids() to return a non-empty list, got: {core_ids:?}"
        );
    }

    #[test]
    fn build_runtime_creates_runtime() {
        let settings = irontide::session::Settings {
            runtime_worker_threads: 2,
            pin_cores: true,
            ..irontide::session::Settings::default()
        };
        let rt = build_runtime(&settings);
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn build_runtime_no_pin() {
        let settings = irontide::session::Settings {
            runtime_worker_threads: 2,
            pin_cores: false,
            ..irontide::session::Settings::default()
        };
        let rt = build_runtime(&settings);
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn build_runtime_auto_workers() {
        let settings = irontide::session::Settings {
            runtime_worker_threads: 0,
            pin_cores: false,
            ..irontide::session::Settings::default()
        };
        let rt = build_runtime(&settings);
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }
}
