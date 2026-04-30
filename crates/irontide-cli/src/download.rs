#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: CLI download command — display formatting and progress bar arithmetic with values bounded by realistic torrent sizes"
)]

use anyhow::Context;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use irontide::core::{DEFAULT_CHUNK_SIZE, Lengths, TorrentMeta};
use irontide::session::SessionState;
use irontide::storage::{FilesystemStorage, PreallocateMode, TorrentStorage};

use crate::client::{TorrentInfoDto, TorrentStatsDto};
use crate::format::{format_rate, format_size};
use crate::progress::{RenderOpts, render_human, render_json};

pub struct DownloadOpts<'a> {
    pub source: &'a str,
    pub output: &'a Path,
    pub no_dht: bool,
    pub seed: bool,
    pub port: u16,
    pub quiet: bool,
    pub json: bool,
    pub settings: irontide::session::Settings,
    pub api_port: u16,
    pub api_bind: String,
    pub diagnose: bool,
}

/// Presentation mode selected once at the start of the progress loop.
///
/// The four variants correspond to the decision tree in the M159 spec
/// and are encoded here so every tick can `match` rather than re-check
/// three flags.
#[derive(Debug, Clone, Copy)]
enum PresentationMode {
    /// `--quiet`: no progress output whatsoever.
    Quiet,
    /// `--json`: line-delimited JSON objects on stdout, one per tick.
    Json,
    /// Default + interactive stdout: single-line overwrite on 1-file
    /// torrents, cursor-up multi-line block on multi-file torrents.
    TtyLine,
    /// Default + non-interactive stdout (pipes, log redirection): plain
    /// text blocks every 10s, no carriage-return tricks.
    Plain,
}

/// Emit one compact JSON object for the current tick on stdout.
///
/// Matches `progress::render_json` so JSON mode is a straight
/// line-delimited dump of the same shape the WebSocket stream emits.
/// Uses `serde_json::to_string` (compact, no indentation) so tools like
/// `jq -c` can consume the stream one object per line.
fn emit_json_tick(stats: &TorrentStatsDto, info: Option<&TorrentInfoDto>) -> anyhow::Result<()> {
    let value = render_json(stats, info, None);
    let line = serde_json::to_string(&value)
        .map_err(|e| anyhow::anyhow!("failed to encode progress JSON: {e}"))?;
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(line.as_bytes())
        .map_err(|e| anyhow::anyhow!("failed to write progress JSON: {e}"))?;
    stdout
        .write_all(b"\n")
        .map_err(|e| anyhow::anyhow!("failed to write progress JSON: {e}"))?;
    stdout
        .flush()
        .map_err(|e| anyhow::anyhow!("failed to flush progress JSON: {e}"))?;
    Ok(())
}

/// Emit the legacy single-line progress bar with carriage-return
/// overwrite. Preserved for single-file torrents where a four-line
/// block would be wasted screen real estate. Writes to stderr so
/// pipeline redirection on stdout stays clean.
fn emit_tty_single_line(
    stats: &irontide::session::TorrentStats,
    elapsed: f64,
) -> anyhow::Result<()> {
    let pct = stats.progress * 100.0;
    let done = format_size(stats.total_done);
    let total = format_size(stats.total_wanted);
    let down = format_rate(stats.download_rate);
    let up = format_rate(stats.upload_rate);
    let peers = stats.peers_connected;
    let eta = if stats.download_rate > 0 && stats.total_wanted > stats.total_done {
        let remaining = stats.total_wanted - stats.total_done;
        format!("{:.0}s", remaining as f64 / stats.download_rate as f64)
    } else {
        "---".to_string()
    };
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
    std::io::Write::flush(&mut std::io::stderr())
        .map_err(|e| anyhow::anyhow!("failed to flush stderr: {e}"))?;
    Ok(())
}

/// Emit the multi-line progress block with cursor-up redraw. The block
/// is always padded to `max_lines` rows so the ANSI cursor-up count
/// stays correct even if the progress renderer's `... and N more`
/// trailer appears or disappears between ticks.
///
/// Returns the number of lines this tick printed (always `max_lines`
/// after padding) so the next tick knows how many rows to rewind.
fn emit_tty_block(
    stats: &TorrentStatsDto,
    info: Option<&TorrentInfoDto>,
    last_block_lines: usize,
    max_lines: usize,
) -> anyhow::Result<usize> {
    let mut lines = render_human(stats, info, None, RenderOpts::default());
    // Pad to `max_lines` so cursor-up math stays stable across ticks.
    while lines.len() < max_lines {
        lines.push(String::new());
    }
    // Defensive: if the renderer ever outgrows our reservation (e.g.
    // someone changes MANY_FILES_THRESHOLD upstream), truncate — we
    // cannot retroactively increase `max_lines` without scrolling the
    // previous block into history.
    lines.truncate(max_lines);

    let mut stderr = std::io::stderr().lock();
    if last_block_lines > 0 {
        // Move cursor up `last_block_lines` rows so the new block
        // overwrites the old one. Matches `tput cuu N` / `\e[<N>A`.
        write!(stderr, "\x1b[{last_block_lines}A")
            .map_err(|e| anyhow::anyhow!("failed to write cursor-up: {e}"))?;
    }
    for line in &lines {
        // `\x1b[2K` clears the whole line first so leftover chars from
        // a longer previous tick don't bleed through when the new tick
        // is shorter.
        writeln!(stderr, "\x1b[2K{line}")
            .map_err(|e| anyhow::anyhow!("failed to write progress block: {e}"))?;
    }
    stderr
        .flush()
        .map_err(|e| anyhow::anyhow!("failed to flush progress block: {e}"))?;
    Ok(lines.len())
}

/// Emit the plain-text progress block for non-interactive stdout
/// (pipes, log redirection). Uses the same `render_human` path as
/// `emit_tty_block` but without any ANSI escape sequences — safe to
/// append to `downloads.log` without `cat -v` artefacts.
fn emit_plain_block(stats: &TorrentStatsDto, info: Option<&TorrentInfoDto>) -> anyhow::Result<()> {
    let lines = render_human(stats, info, None, RenderOpts::default());
    let mut stderr = std::io::stderr().lock();
    for line in &lines {
        writeln!(stderr, "{line}")
            .map_err(|e| anyhow::anyhow!("failed to write progress block: {e}"))?;
    }
    stderr
        .flush()
        .map_err(|e| anyhow::anyhow!("failed to flush progress block: {e}"))?;
    Ok(())
}

pub async fn run(opts: DownloadOpts<'_>) -> anyhow::Result<()> {
    let DownloadOpts {
        source,
        output,
        no_dht,
        seed,
        port,
        quiet,
        json,
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
        session.add_torrent_with_meta(meta, Some(storage)).await?;
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
                    () = tokio::time::sleep(Duration::from_secs(5)) => return,
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
    const SAVE_INTERVAL: Duration = Duration::from_mins(1);
    const POLL_INTERVAL: Duration = Duration::from_secs(1);
    const DIAGNOSE_INTERVAL: Duration = Duration::from_secs(5);
    const NON_TTY_INTERVAL: Duration = Duration::from_secs(10);

    // Presentation mode selection. The four modes from the M159 spec are:
    //   Quiet    — `--quiet`: no stdout/stderr progress chatter.
    //   Json     — `--json`:  one JSON object per tick, line-delimited.
    //   TtyLine  — isatty stdout + !quiet + !json: in-place overwrite
    //              (single line for 1-file torrents, cursor-up block for
    //              multi-file torrents).
    //   Plain    — not isatty + !quiet + !json: plain blocks every 10s.
    //
    // The tty check is cached at the start of the run — stdout cannot
    // change mid-process and re-checking per iteration would just be
    // noise.
    let is_tty = std::io::stdout().is_terminal();
    let mode = if quiet {
        PresentationMode::Quiet
    } else if json {
        PresentationMode::Json
    } else if is_tty {
        PresentationMode::TtyLine
    } else {
        PresentationMode::Plain
    };

    // Cached TorrentInfoDto: becomes Some once the engine has metadata.
    // Until then we only render the stats line (the DTO name / progress
    // bar works with `info = None`). The multi-file block path needs
    // `info` to compute file counts, so we cap `max_lines` the first
    // time we observe `files.len() > 1` and pad every subsequent redraw
    // to that exact height.
    let mut info_dto: Option<TorrentInfoDto> = None;
    let mut last_block_lines: usize = 0;
    let mut max_lines: usize = 0;
    // `last_plain_print` gates the non-tty plain-text cadence — print
    // one block every NON_TTY_INTERVAL rather than once per poll.
    let mut last_plain_print = Instant::now()
        .checked_sub(NON_TTY_INTERVAL)
        .unwrap_or_else(Instant::now);

    loop {
        // Check for cancellation
        if cancelled.load(Ordering::SeqCst) {
            if !quiet && matches!(mode, PresentationMode::TtyLine) {
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

            // Opportunistically fetch TorrentInfo once metadata lands.
            // This is the signal for the human renderer to switch into
            // the multi-file block path — it has no effect until the
            // engine has the info dict.
            if info_dto.is_none()
                && stats.has_metadata
                && let Ok(live_info) = session.torrent_info(info_hash).await
            {
                let dto = TorrentInfoDto::from_live(&live_info);
                // Pad height for cursor-up redraw: 2 header lines +
                // one line per file + 1 trailing slack for the
                // optional `... and N more` trailer. Capped to the
                // progress renderer's default `top_n` of 10 when
                // the torrent has > 20 files (mirrors the
                // `MANY_FILES_THRESHOLD` constant in progress.rs).
                let file_rows = dto.files.len().min(10);
                max_lines = 2 + file_rows + 1;
                info_dto = Some(dto);
            }

            if !matches!(mode, PresentationMode::Quiet) && !finished {
                let stats_dto = TorrentStatsDto::from_live(&stats);
                let elapsed = start_time.elapsed().as_secs_f64();
                match mode {
                    PresentationMode::Quiet => {}
                    PresentationMode::Json => {
                        emit_json_tick(&stats_dto, info_dto.as_ref())?;
                    }
                    PresentationMode::TtyLine => {
                        let files_known = info_dto
                            .as_ref()
                            .is_some_and(|i| i.files.len() > 1);
                        if files_known {
                            last_block_lines = emit_tty_block(
                                &stats_dto,
                                info_dto.as_ref(),
                                last_block_lines,
                                max_lines,
                            )?;
                        } else {
                            emit_tty_single_line(&stats, elapsed)?;
                        }
                    }
                    PresentationMode::Plain => {
                        if last_plain_print.elapsed() >= NON_TTY_INTERVAL {
                            emit_plain_block(&stats_dto, info_dto.as_ref())?;
                            last_plain_print = Instant::now();
                        }
                    }
                }
            }

            // Pipeline diagnostics (every 5s when --diagnose enabled)
            if diagnose && !finished && last_diagnose.elapsed() >= DIAGNOSE_INTERVAL {
                last_diagnose = Instant::now();
                if let Ok(peers) = session.get_peer_info(info_hash).await {
                    let stats = session.torrent_stats(info_hash).await.ok();
                    let pipeline = stats.as_ref().and_then(|s| s.pipeline);
                    let choke_rotations = stats.map_or(0, |s| s.choke_rotations);
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
                    .map_or(0, |s| s.unique_peers_attempted);
                let choke_rotations = stats_snapshot
                    .as_ref()
                    .map_or(0, |s| s.choke_rotations);
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
    top10.sort_by_key(|p| std::cmp::Reverse(p.download_rate));
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
        "  Peers: {total} total | {unchoked} unchoked | {downloading} downloading | {choked} choked ({choke_pct:.0}%)",
    );
    eprintln!(
        "  Pipeline: {} in-flight requests | {}/s aggregate",
        total_pending,
        format_rate(total_dl_rate),
    );
    eprintln!("  Per-peer avg: {per_peer_avg:.2} MB/s ({unchoked} unchoked peers)");
    eprintln!("  Throughput buckets (unchoked peers):");
    eprintln!(
        "    0 MB/s:       {bucket_0:3}  |  0.1-0.5 MB/s: {bucket_mid:3}",
    );
    eprintln!(
        "    0-0.1 MB/s:   {bucket_low:3}  |  0.5-1.0 MB/s: {bucket_high:3}",
    );
    eprintln!("    >=1.0 MB/s:   {bucket_top:3}");

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
            "  \x1b[1;33mWARN: LOW PIPELINE: {total_pending} in-flight for {unchoked} unchoked peers (avg {avg_pending:.1}/peer, expected ~128)\x1b[0m",
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
    active.sort_by_key(|p| std::cmp::Reverse(p.download_rate));

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

async fn save_session_state(
    session: &irontide::session::SessionHandle,
    state_path: &Path,
    announce: bool,
) {
    // Existing session state save (DHT + bans).
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

    // Per-torrent resume files (M161).
    match session.save_resume_state().await {
        Ok(count) if count > 0 && announce => {
            eprintln!("Saved {count} torrent resume file(s)");
        }
        Ok(_) => {}
        Err(e) => {
            eprintln!("warning: failed to save resume state: {e}");
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

    /// Round-trip a `TorrentStats` through `TorrentStatsDto::from_live`
    /// and confirm the critical fields map correctly. The DTO uses
    /// serde renames (`downloaded` ← `total_done`, `uploaded` ←
    /// `total_upload`), and the match-based state label must stay
    /// stable across `Debug` format changes — both are load-bearing
    /// for the M159 progress renderer.
    #[test]
    fn test_from_live_stats_round_trip() {
        use irontide::core::{Id20, InfoHashes};
        use irontide::session::{TorrentState, TorrentStats};

        let id = Id20::from([0x11; 20]);
        let stats = TorrentStats {
            state: TorrentState::Downloading,
            name: "test.iso".to_owned(),
            total_done: 12_345,
            total_upload: 6_789,
            total: 1_000_000,
            download_rate: 1024,
            upload_rate: 512,
            pieces_have: 3,
            pieces_total: 10,
            peers_connected: 4,
            peers_available: 8,
            is_paused: false,
            is_finished: false,
            is_seeding: false,
            user_seed_mode: false,
            progress: 0.25,
            progress_ppm: 250_000,
            info_hashes: InfoHashes::v1_only(id),
            ..TorrentStats::default()
        };

        let dto = TorrentStatsDto::from_live(&stats);

        assert_eq!(dto.name, "test.iso");
        assert_eq!(dto.state, "Downloading");
        // rename: total_done → downloaded
        assert_eq!(dto.downloaded, 12_345);
        // rename: total_upload → uploaded
        assert_eq!(dto.uploaded, 6_789);
        assert_eq!(dto.total, 1_000_000);
        assert_eq!(dto.download_rate, 1024);
        assert_eq!(dto.upload_rate, 512);
        assert_eq!(dto.pieces_have, 3);
        assert_eq!(dto.pieces_total, 10);
        assert_eq!(dto.peers_connected, 4);
        // f32 → f64 widening is lossless for small exact values.
        assert!((dto.progress - 0.25).abs() < f64::EPSILON);
        assert_eq!(dto.progress_ppm, 250_000);
        // info_hash_hex should round-trip the 0x11 byte pattern.
        assert_eq!(
            dto.info_hash_hex(),
            "1111111111111111111111111111111111111111"
        );
    }

    /// Build a `TorrentInfo` with three files, convert via
    /// `TorrentInfoDto::from_live`, and confirm file metadata survives
    /// the conversion. Exercises the `FileInfoDto::from_live` path too.
    #[test]
    fn test_from_live_info_maps_files() {
        use irontide::core::Id20;
        use irontide::session::{FileInfo, TorrentInfo};

        let info = TorrentInfo {
            info_hash: Id20::from([0u8; 20]),
            name: "bundle".to_owned(),
            total_length: 6_000,
            piece_length: 16_384,
            num_pieces: 1,
            files: vec![
                FileInfo {
                    path: PathBuf::from("a/first.bin"),
                    length: 1_000,
                },
                FileInfo {
                    path: PathBuf::from("a/second.bin"),
                    length: 2_000,
                },
                FileInfo {
                    path: PathBuf::from("a/third.bin"),
                    length: 3_000,
                },
            ],
            private: false,
        };

        let dto = TorrentInfoDto::from_live(&info);

        assert_eq!(dto.name, "bundle");
        assert_eq!(dto.total_length, 6_000);
        assert_eq!(dto.piece_length, 16_384);
        assert_eq!(dto.num_pieces, 1);
        assert_eq!(dto.files.len(), 3);
        assert_eq!(dto.files[0].path, "a/first.bin");
        assert_eq!(dto.files[0].length, 1_000);
        assert_eq!(dto.files[1].path, "a/second.bin");
        assert_eq!(dto.files[1].length, 2_000);
        assert_eq!(dto.files[2].path, "a/third.bin");
        assert_eq!(dto.files[2].length, 3_000);
        assert!(!dto.private);
    }
}
