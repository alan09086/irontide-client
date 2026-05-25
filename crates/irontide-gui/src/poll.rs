#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: Slint UI poll-loop — counters bounded by torrent count and refresh cadence; precision loss intentional for display formatting"
)]

//! 500ms polling loop that fetches torrent data from the session,
//! formats it into Slint `TorrentRow` structs, and pushes updates
//! to the main window.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use irontide::session::{SessionHandle, TorrentState, TorrentSummary};

use crate::columns::ColumnId;
use crate::sidebar::{RowView, SidebarPredicate, TrackerIndex};
use crate::sidebar_view;

// Thread-local storage for the VecModel so the upgrade_in_event_loop
// closure can access it without moving it into each closure.
thread_local! {
    static TORRENT_MODEL: RefCell<Option<Rc<VecModel<crate::TorrentRow>>>> = const { RefCell::new(None) };
    // M177: Weak handle to the main window so non-poll-loop callbacks
    // (row click, select-all, right-click) can push the primary
    // selection to Slint without owning a strong reference.
    static MAIN_WINDOW_WEAK: RefCell<Option<slint::Weak<crate::MainWindow>>> = const { RefCell::new(None) };
}

/// Push the *primary* selected info-hash to Slint (M177).
///
/// Per D-eng-1, this is split out from [`update_selection`] so that the
/// poll loop's per-tick model rebuild can refresh `selected: bool` on
/// every row without re-pushing the primary hash. Selection callbacks
/// (row click, Ctrl+A, right-click) call both helpers.
///
/// `None` clears the property — the detail pane then renders its empty
/// state. The Slint event loop is invoked synchronously via
/// `upgrade_in_event_loop`; the call is a no-op if the weak handle has
/// not yet been initialised (poll loop not yet running).
pub fn update_primary_selection(primary: Option<&str>) {
    let value = primary.unwrap_or("").to_owned();
    MAIN_WINDOW_WEAK.with(|w| {
        if let Some(weak) = w.borrow().as_ref() {
            let _ = weak.upgrade_in_event_loop(move |win| {
                win.set_detail_info_hash(SharedString::from(value));
            });
        }
    });
}

/// Update selection state on the model immediately (called from main-thread callbacks).
///
/// Iterates through all model rows and sets `selected` to match the given set.
/// Only touches rows whose selection state actually changed, to avoid redundant
/// model notifications.
pub fn update_selection(selected: &HashSet<String>) {
    TORRENT_MODEL.with(|m| {
        let borrow = m.borrow();
        let Some(model) = borrow.as_ref() else {
            return;
        };
        for i in 0..model.row_count() {
            if let Some(mut row) = model.row_data(i) {
                let should_select = selected.contains(row.info_hash.as_str());
                if row.selected != should_select {
                    row.selected = should_select;
                    model.set_row_data(i, row);
                }
            }
        }
    });
}

/// Check if all selected torrents are in the "paused" state.
///
/// Returns `true` only when every matched torrent has `state == "paused"` AND at
/// least one match was found. Returns `false` for empty or unmatched selections.
///
/// # Safety
///
/// Must be called from the Slint main thread (inside a Slint callback) because
/// it accesses the thread-local `TORRENT_MODEL`.
pub fn check_all_paused(hashes: &HashSet<String>) -> bool {
    TORRENT_MODEL.with(|m| {
        let borrow = m.borrow();
        let Some(model) = borrow.as_ref() else {
            return false;
        };
        let mut found_any = false;
        for i in 0..model.row_count() {
            if let Some(row) = model.row_data(i)
                && hashes.contains(row.info_hash.as_str())
            {
                found_any = true;
                if row.state.as_str() != "paused" && row.state.as_str() != "queued" {
                    return false;
                }
            }
        }
        found_any
    })
}

/// Initialise the thread-local torrent model and weak window handle,
/// binding the model to the window. M177: also caches a `slint::Weak`
/// for [`update_primary_selection`] so non-poll callbacks can push the
/// primary info-hash without owning a strong reference.
///
/// Must be called on the Slint main thread (inside `upgrade_in_event_loop`)
/// exactly once during startup.
pub fn init_window(win: &crate::MainWindow) {
    let model = Rc::new(VecModel::<crate::TorrentRow>::default());
    TORRENT_MODEL.with(|m| *m.borrow_mut() = Some(model.clone()));
    MAIN_WINDOW_WEAK.with(|w| *w.borrow_mut() = Some(win.as_weak()));
    win.set_torrent_model(ModelRc::from(model));
}

/// Run the 500ms polling loop that keeps the UI in sync with the session.
///
/// This function runs on the tokio runtime (background thread) and pushes
/// updates to the Slint main thread via `upgrade_in_event_loop`.
pub async fn poll_loop(
    session: SessionHandle,
    weak: slint::Weak<crate::MainWindow>,
    state: Arc<Mutex<crate::app::AppState>>,
) {
    // Fetch listen port once at startup.
    let listen_port = session
        .settings()
        .await
        .map_or(0, |s| i32::from(s.listen_port));

    // M173 Lane A: TrackerIndex carries previous-tick counts for diff-only
    // sidebar updates. The first call returns the full Added/Changed mix
    // for cold-start population.
    let mut tracker_index = TrackerIndex::new();

    // M177 detail-pane state carried across ticks:
    //   * `last_detail_hash` lets us detect selection changes so we know
    //     when to invalidate `cached_info` and emit a tracing::debug! to
    //     show snapshot deltas during development.
    //   * `cached_info` stays populated for the duration of a single
    //     selection — `torrent_info` is a one-shot per selection (it
    //     only changes on metadata resolve, which is rare).
    let mut last_detail_hash: Option<String> = None;
    let mut cached_info: Option<irontide::session::TorrentInfo> = None;

    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Fetch torrent IDs once, then enrich each with stats + trackers.
        // We hold a (TorrentSummary, RowView) tuple per torrent so the
        // existing model + the M173 sidebar share one fetch.
        let ids = match session.list_torrents().await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!("poll: list_torrents failed: {e}");
                continue;
            }
        };
        let mut summaries: Vec<TorrentSummary> = Vec::with_capacity(ids.len());
        let mut rich: Vec<RowView> = Vec::with_capacity(ids.len());
        for id in ids {
            let Ok(stats) = session.torrent_stats(id).await else {
                continue; // shutting down or vanishing — skip
            };
            // The tracker_list call is best-effort; on failure we still
            // produce a row, just with empty tracker buckets.
            let trackers = session.tracker_list(id).await.unwrap_or_default();
            summaries.push(TorrentSummary::from(&stats));
            rich.push(sidebar_view::rich_row_view(&stats, &trackers));
        }

        // M180: accumulate speed samples for ALL torrents every tick.
        {
            let mut st = state.lock();
            for rv in &rich {
                st.speed_histories
                    .entry(rv.info_hash.clone())
                    .or_insert_with(crate::speed::SpeedHistory::new)
                    .push(rv.download_rate, rv.upload_rate);
            }
        }

        let sess_stats = match session.session_stats().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("poll: session_stats failed: {e}");
                continue;
            }
        };

        // Read sort + selection + predicate state.
        let (sort, selected, predicate) = {
            let st = state.lock();
            (st.sort, st.selected.clone(), st.predicate.clone())
        };

        // Filter (M173 Lane A) then sort. The enricher reaches into the
        // pre-built rich slice so the predicate can match against
        // `error` / `category` / `tags` / tracker buckets.
        let rich_by_hash: std::collections::HashMap<&str, &RowView> =
            rich.iter().map(|r| (r.info_hash.as_str(), r)).collect();
        let mut sorted = apply_predicate(&summaries, &predicate, |s| {
            rich_by_hash
                .get(s.info_hash.as_str())
                .copied()
                .cloned()
                .unwrap_or_else(|| RowView::from_summary(s))
        });
        sort_summaries(&mut sorted, sort);

        // Convert to Slint rows.
        let new_rows: Vec<crate::TorrentRow> =
            sorted.iter().map(|s| to_slint_row(s, &selected)).collect();

        // Update current_order in state (for shift-click range).
        //
        // v0.187.3 / Bug 14A / multi-select anchor sync: if `last_clicked`
        // points at a torrent that's no longer in `current_order` (e.g. the
        // anchor torrent was removed), clear it. This avoids the "two-item
        // multi-select doesn't work" path where a stale anchor caused
        // `selection_shift_click` to fall back to single-click semantics
        // even when the user clearly Shift-clicked a valid pair.
        //
        // Intentionally we do NOT touch `selected` — the user's selection
        // intent is preserved; only the anchor (a transient interaction
        // breadcrumb) is reconciled with the current visible set.
        {
            let mut st = state.lock();
            st.current_order = sorted.iter().map(|s| s.info_hash.clone()).collect();
            if let Some(anchor) = st.last_clicked.clone()
                && !st.current_order.contains(&anchor)
            {
                st.last_clicked = None;
            }
        }

        // Compute status bar values.
        let (agg_down, agg_up) = aggregate_rates(&sorted);
        let total_torrents = sorted.len() as i32;

        // M193: per-state counts for system tray icon derivation.
        let (downloading_count, seeding_count, error_count) = count_states(&sorted);
        let dht_nodes = sess_stats.dht_nodes as i32;

        let status_down = if agg_down > 0 {
            crate::format::format_rate(agg_down)
        } else {
            "\u{2014}".to_owned()
        };
        let status_up = if agg_up > 0 {
            crate::format::format_rate(agg_up)
        } else {
            "\u{2014}".to_owned()
        };

        // ── M177 detail-pane snapshot (Step 3) ──────────────────────
        // Reads the primary selection + active tab from AppState while
        // holding the lock briefly, then performs all session fetches
        // outside the lock. The snapshot stays Rust-side for now —
        // Step 4+ adds the Slint property push.
        let (detail_primary, detail_active_tab, detail_expanded, detail_files_selected) = {
            let st = state.lock();
            (
                st.primary_selected().map(str::to_owned),
                st.detail_active_tab.clone(),
                st.detail_expanded.clone(),
                st.detail_files_selected.clone(),
            )
        };

        // Selection-change handling: invalidate cached_info on a hash
        // change, log the new selection at debug level for dev tracing.
        // M178 (D-eng-7 Iron Rule): also clear the file-selection set +
        // pending popup state so they don't leak across torrents.
        if detail_primary != last_detail_hash {
            cached_info = None;
            tracing::debug!(
                ?last_detail_hash,
                ?detail_primary,
                "detail: primary selection changed"
            );
            {
                let mut st = state.lock();
                st.clear_file_selection_for_torrent_change();
            }
            // Also dismiss any open file-priority popup since its target
            // belongs to the previous torrent.
            MAIN_WINDOW_WEAK.with(|w| {
                if let Some(weak) = w.borrow().clone() {
                    let _ = weak.upgrade_in_event_loop(|win| {
                        win.set_show_file_priority_popup(false);
                    });
                }
            });
            last_detail_hash = detail_primary.clone();
        }

        if let Some(hash_hex) = detail_primary.as_deref() {
            // Parse the hex hash into the typed Id20 the session APIs
            // require. A failed parse means the AppState had a malformed
            // hash, which would be a bug — log and skip.
            let id_opt = irontide::core::Id20::from_hex(hash_hex).ok();
            if let Some(id) = id_opt {
                // Fetch torrent_info one-shot per selection.
                if cached_info.is_none() {
                    match session.torrent_info(id).await {
                        Ok(info) => cached_info = Some(info),
                        Err(e) => {
                            tracing::debug!(hash = hash_hex, ?e, "detail: torrent_info failed");
                        }
                    }
                }

                if let Ok(detail_stats) = session.torrent_stats(id).await {
                    // Piece states drive the General-tab heatmap — small,
                    // fetch every tick.
                    let piece_states = session.get_piece_states(id).await.unwrap_or_default();
                    if !piece_states.is_empty() {
                        tracing::trace!(
                            len = piece_states.len(),
                            non_zero = piece_states.iter().filter(|&&s| s != 0).count(),
                            "piece_states for heatmap"
                        );
                    }
                    let buckets = crate::detail::bucket_piece_states(&piece_states, 512);

                    // Content-tab gated fetches (D-eng-2).
                    let files: Vec<crate::FileTreeRow> = if detail_active_tab == "Content"
                        && let Some(info) = cached_info.as_ref()
                    {
                        let priorities = session.file_priorities(id).await.unwrap_or_default();
                        let progress = session.file_progress(id).await.unwrap_or_default();
                        let flat = irontide_format::build_flat(info, &progress, &priorities);
                        // F9: cache flat files for folder-level priority resolution.
                        state.lock().detail_flat_files.clone_from(&flat);
                        crate::detail::flatten_files(
                            &flat,
                            &detail_expanded,
                            hash_hex,
                            &detail_files_selected,
                        )
                    } else {
                        Vec::new()
                    };

                    // M178 (D-eng-3): Peers / Trackers / HTTP Sources gated
                    // fetches. Each tab only fires its session call when that
                    // tab is the active one, keeping the hot path bounded for
                    // huge torrents.
                    let peers: Vec<crate::PeerRow> = if detail_active_tab == "Peers" {
                        let peer_info = session.get_peer_info(id).await.unwrap_or_default();
                        crate::detail::flatten_peer_rows(&peer_info)
                    } else {
                        Vec::new()
                    };

                    let trackers: Vec<crate::TrackerRow> = if detail_active_tab == "Trackers" {
                        let real = session.tracker_list(id).await.unwrap_or_default();
                        let (pex_count, lsd_count) = session
                            .pex_peer_count(id)
                            .await
                            .ok()
                            .zip(session.lsd_peer_count(id).await.ok())
                            .unwrap_or((0, 0));
                        let dht_count = session.dht_node_count().await.unwrap_or(0);
                        let merged = irontide_format::synthesize_pseudo_trackers(
                            &real, dht_count, pex_count, lsd_count,
                        );
                        crate::detail::flatten_tracker_rows(&merged)
                    } else {
                        Vec::new()
                    };

                    let http_sources: Vec<crate::WebSeedRow> =
                        if detail_active_tab == "HTTP Sources" {
                            let stats = session.web_seed_stats(id).await.unwrap_or_default();
                            crate::detail::flatten_web_seed_rows(&stats)
                        } else {
                            Vec::new()
                        };

                    // M180: Speed tab gated fetch.
                    let speed = if detail_active_tab == "Speed" {
                        let (dl_scaled, ul_scaled, max_rate, elapsed) = {
                            let st = state.lock();
                            if let Some(hist) = st.speed_histories.get(hash_hex) {
                                let (dl, ul) = hist.flatten_auto();
                                let mr = hist.max_rate();
                                let el = hist.elapsed_label();
                                (dl, ul, mr, el)
                            } else {
                                (Vec::new(), Vec::new(), 0, String::new())
                            }
                        };
                        let dl_path = crate::speed::build_path_commands(&dl_scaled, 1000, 1000);
                        let ul_path = crate::speed::build_path_commands(&ul_scaled, 1000, 1000);
                        let dl_lim = session.download_limit(id).await.unwrap_or(0);
                        let ul_lim = session.upload_limit(id).await.unwrap_or(0);
                        SpeedProps {
                            dl_path,
                            ul_path,
                            dl_limit: if dl_lim == 0 {
                                "0".into()
                            } else {
                                crate::format::format_rate(dl_lim)
                            },
                            ul_limit: if ul_lim == 0 {
                                "0".into()
                            } else {
                                crate::format::format_rate(ul_lim)
                            },
                            max_rate: if max_rate == 0 {
                                "\u{2014}".into()
                            } else {
                                crate::format::format_rate(max_rate)
                            },
                            current_dl: if detail_stats.download_rate > 0 {
                                crate::format::format_rate(detail_stats.download_rate)
                            } else {
                                "\u{2014}".into()
                            },
                            current_ul: if detail_stats.upload_rate > 0 {
                                crate::format::format_rate(detail_stats.upload_rate)
                            } else {
                                "\u{2014}".into()
                            },
                            elapsed,
                        }
                    } else {
                        SpeedProps::default()
                    };

                    let snapshot = build_detail_props(
                        &detail_stats,
                        cached_info.as_ref(),
                        buckets,
                        files,
                        peers,
                        trackers,
                        http_sources,
                        speed,
                    );

                    let _ = weak.upgrade_in_event_loop(move |win| {
                        apply_detail_props(&win, snapshot);
                    });
                } else {
                    // Selected torrent vanished (most commonly: user just
                    // hit Remove). Evict its hash from AppState selection
                    // so subsequent ticks treat the list as deselected,
                    // and clear the detail pane immediately. Falling
                    // through (no `continue`) lets the rest of the poll
                    // tick reach the model rebuild at the bottom of the
                    // loop, which prunes the now-deleted row from the
                    // torrent list.
                    {
                        let mut st = state.lock();
                        st.selected.remove(hash_hex);
                        if st.last_clicked.as_deref() == Some(hash_hex) {
                            st.last_clicked = None;
                        }
                        st.clear_file_selection_for_torrent_change();
                        st.pending_file_priority_target = None;
                    }
                    let _ = weak.upgrade_in_event_loop(|win| {
                        clear_detail_props(&win);
                        win.set_show_file_priority_popup(false);
                    });
                    cached_info = None;
                    last_detail_hash = None;
                }
            }
        } else {
            // Empty selection → clear the detail-* properties so the
            // empty-state branch renders.
            let _ = weak.upgrade_in_event_loop(|win| {
                clear_detail_props(&win);
            });
        }

        // M173 Lane A: refresh the TrackerIndex + build sidebar rows.
        // The Index is owned by the loop (per-tick state) so the diff is
        // computed against the previous tick. We discard the diff for
        // now — A8 just pushes the full counts to the UI; future work
        // can route the diff to a `row_changed`-style incremental
        // update on the Slint sidebar models.
        let _ = tracker_index.update(&rich);
        let counts = tracker_index.snapshot().cloned().unwrap_or_default();
        let category_names: Vec<String> = session
            .list_categories()
            .await
            .into_iter()
            .map(|c| c.name)
            .collect();
        let tag_names = session.list_tags().await;
        let sidebar_rows =
            sidebar_view::build_sidebar_rows(&counts, &category_names, &tag_names, &predicate);

        // Push to UI.
        let sort_col = sort.column.to_index();
        let sort_asc = sort.ascending;
        let _ = weak.upgrade_in_event_loop(move |win| {
            // Incremental model update.
            TORRENT_MODEL.with(|m| {
                let borrow = m.borrow();
                let Some(model) = borrow.as_ref() else {
                    return;
                };
                let old_count = model.row_count();
                let new_count = new_rows.len();

                // Update existing rows (only if changed).
                for (i, new_row) in new_rows.iter().enumerate().take(old_count.min(new_count)) {
                    if let Some(existing) = model.row_data(i)
                        && rows_differ(&existing, new_row)
                    {
                        model.set_row_data(i, new_row.clone());
                    }
                }

                // Add new torrents.
                for new_row in new_rows.iter().skip(old_count) {
                    model.push(new_row.clone());
                }

                // Remove deleted torrents (reverse order).
                for i in (new_count..old_count).rev() {
                    model.remove(i);
                }
            });

            // Update status bar.
            win.set_status_down_rate(SharedString::from(&status_down));
            win.set_status_up_rate(SharedString::from(&status_up));
            win.set_status_total_torrents(total_torrents);
            win.set_status_dht_nodes(dht_nodes);
            win.set_status_listen_port(listen_port);
            win.set_status_downloading_count(downloading_count);
            win.set_status_seeding_count(seeding_count);
            win.set_status_error_count(error_count);
            win.set_sort_column(sort_col);
            win.set_sort_ascending(sort_asc);

            // M173 Lane A: push the four sidebar lists.
            win.set_sidebar_library_rows(slint::ModelRc::new(slint::VecModel::from(
                sidebar_rows.library,
            )));
            win.set_sidebar_category_rows(slint::ModelRc::new(slint::VecModel::from(
                sidebar_rows.categories,
            )));
            win.set_sidebar_tag_rows(slint::ModelRc::new(slint::VecModel::from(
                sidebar_rows.tags,
            )));
            win.set_sidebar_tracker_rows(slint::ModelRc::new(slint::VecModel::from(
                sidebar_rows.trackers,
            )));
        });
    }
}

// ── M177 detail-pane snapshot push ──────────────────────────────────────

/// Plain-data bag of every Slint detail-* property value, computed
/// off the UI thread and applied inside `upgrade_in_event_loop`.
struct DetailProps {
    info_hash: String,
    name: String,
    error: String,
    state_label: String,
    down_rate: String,
    up_rate: String,
    all_time_down: String,
    all_time_up: String,
    ratio: String,
    eta: String,
    num_peers: String,
    num_seeds: String,
    share_fraction: String,
    save_path: String,
    total_size: String,
    piece_count_text: String,
    piece_size: String,
    added_time: String,
    last_seen_complete: String,
    active_duration: String,
    piece_buckets: Vec<i32>,
    pieces_text: String,
    sequential: bool,
    files: Vec<crate::FileTreeRow>,
    // M178: per-tab data populated when the corresponding tab is active.
    peers: Vec<crate::PeerRow>,
    trackers: Vec<crate::TrackerRow>,
    http_sources: Vec<crate::WebSeedRow>,
    // M180: Speed tab data.
    speed_dl_path: String,
    speed_ul_path: String,
    speed_dl_limit: String,
    speed_ul_limit: String,
    speed_max_rate: String,
    speed_current_dl: String,
    speed_current_ul: String,
    speed_elapsed: String,
}

struct SpeedProps {
    dl_path: String,
    ul_path: String,
    dl_limit: String,
    ul_limit: String,
    max_rate: String,
    current_dl: String,
    current_ul: String,
    elapsed: String,
}

impl Default for SpeedProps {
    fn default() -> Self {
        Self {
            dl_path: String::new(),
            ul_path: String::new(),
            dl_limit: String::from("0"),
            ul_limit: String::from("0"),
            max_rate: String::from("\u{2014}"),
            current_dl: String::from("\u{2014}"),
            current_ul: String::from("\u{2014}"),
            elapsed: String::new(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_detail_props(
    stats: &irontide::session::TorrentStats,
    info: Option<&irontide::session::TorrentInfo>,
    buckets: Vec<i32>,
    files: Vec<crate::FileTreeRow>,
    peers: Vec<crate::PeerRow>,
    trackers: Vec<crate::TrackerRow>,
    http_sources: Vec<crate::WebSeedRow>,
    speed: SpeedProps,
) -> DetailProps {
    let info_hash = stats.info_hashes.best_v1().to_hex();
    let down_rate = if stats.download_rate > 0 {
        crate::format::format_rate(stats.download_rate)
    } else {
        "\u{2014}".to_owned()
    };
    let up_rate = if stats.upload_rate > 0 {
        crate::format::format_rate(stats.upload_rate)
    } else {
        "\u{2014}".to_owned()
    };
    let remaining = stats.total_wanted.saturating_sub(stats.total_wanted_done);
    let eta = crate::format::format_eta_from_rates(remaining, stats.download_rate);
    let ratio = crate::format::format_ratio(stats.all_time_upload, stats.all_time_download);
    let num_peers = format!("{} / {}", stats.peers_connected, stats.peers_available);
    let num_seeds = stats.num_seeds.to_string();
    let share_fraction = if stats.distributed_copies > 0.0 {
        format!("{:.3}", stats.distributed_copies)
    } else {
        "\u{2014}".to_owned()
    };
    let total_size = crate::format::format_size(stats.total);
    let piece_count_text = if stats.pieces_total == 0 {
        "0".to_owned()
    } else {
        format!("{}", stats.pieces_total)
    };
    let piece_size = if stats.piece_size == 0 {
        "\u{2014}".to_owned()
    } else {
        crate::format::format_size(stats.piece_size)
    };
    let added_time = crate::format::format_relative_time(stats.added_time);
    let last_seen_complete = crate::format::format_relative_time(stats.last_seen_complete);
    let active_duration = crate::format::format_duration_secs(stats.active_duration);
    let pieces_pct = if stats.pieces_total > 0 {
        f64::from(stats.pieces_have) / f64::from(stats.pieces_total) * 100.0
    } else {
        0.0
    };
    let pieces_text = format!(
        "{} / {} pieces ({:.1}%)",
        stats.pieces_have, stats.pieces_total, pieces_pct
    );
    let state_label =
        crate::format::format_state_full(stats.state, stats.user_seed_mode, stats.super_seeding)
            .to_owned();
    let save_path = info.map_or_else(|| stats.save_path.clone(), |_| stats.save_path.clone());

    DetailProps {
        info_hash,
        name: stats.name.clone(),
        error: stats.error.clone(),
        state_label,
        down_rate,
        up_rate,
        all_time_down: crate::format::format_size(stats.all_time_download),
        all_time_up: crate::format::format_size(stats.all_time_upload),
        ratio,
        eta,
        num_peers,
        num_seeds,
        share_fraction,
        save_path,
        total_size,
        piece_count_text,
        piece_size,
        added_time,
        last_seen_complete,
        active_duration,
        piece_buckets: buckets,
        pieces_text,
        sequential: stats.sequential_download,
        files,
        peers,
        trackers,
        http_sources,
        speed_dl_path: speed.dl_path,
        speed_ul_path: speed.ul_path,
        speed_dl_limit: speed.dl_limit,
        speed_ul_limit: speed.ul_limit,
        speed_max_rate: speed.max_rate,
        speed_current_dl: speed.current_dl,
        speed_current_ul: speed.current_ul,
        speed_elapsed: speed.elapsed,
    }
}

fn apply_detail_props(win: &crate::MainWindow, p: DetailProps) {
    win.set_detail_info_hash(SharedString::from(p.info_hash));
    win.set_detail_name(SharedString::from(p.name));
    win.set_detail_error(SharedString::from(p.error));
    win.set_detail_state_label(SharedString::from(p.state_label));
    win.set_detail_down_rate(SharedString::from(p.down_rate));
    win.set_detail_up_rate(SharedString::from(p.up_rate));
    win.set_detail_all_time_down(SharedString::from(p.all_time_down));
    win.set_detail_all_time_up(SharedString::from(p.all_time_up));
    win.set_detail_ratio(SharedString::from(p.ratio));
    win.set_detail_eta(SharedString::from(p.eta));
    win.set_detail_num_peers(SharedString::from(p.num_peers));
    win.set_detail_num_seeds(SharedString::from(p.num_seeds));
    win.set_detail_share_fraction(SharedString::from(p.share_fraction));
    win.set_detail_save_path(SharedString::from(p.save_path));
    win.set_detail_total_size(SharedString::from(p.total_size));
    win.set_detail_piece_count_text(SharedString::from(p.piece_count_text));
    win.set_detail_piece_size(SharedString::from(p.piece_size));
    win.set_detail_added_time(SharedString::from(p.added_time));
    win.set_detail_last_seen_complete(SharedString::from(p.last_seen_complete));
    win.set_detail_active_duration(SharedString::from(p.active_duration));
    win.set_detail_piece_buckets(ModelRc::new(VecModel::from(p.piece_buckets)));
    win.set_detail_pieces_text(SharedString::from(p.pieces_text));
    win.set_detail_sequential(p.sequential);
    win.set_detail_files(ModelRc::new(VecModel::from(p.files)));
    win.set_detail_peers(ModelRc::new(VecModel::from(p.peers)));
    win.set_detail_trackers(ModelRc::new(VecModel::from(p.trackers)));
    win.set_detail_http_sources(ModelRc::new(VecModel::from(p.http_sources)));
    win.set_detail_speed_dl_path(SharedString::from(p.speed_dl_path));
    win.set_detail_speed_ul_path(SharedString::from(p.speed_ul_path));
    win.set_detail_speed_dl_limit(SharedString::from(p.speed_dl_limit));
    win.set_detail_speed_ul_limit(SharedString::from(p.speed_ul_limit));
    win.set_detail_speed_max_rate(SharedString::from(p.speed_max_rate));
    win.set_detail_speed_current_dl(SharedString::from(p.speed_current_dl));
    win.set_detail_speed_current_ul(SharedString::from(p.speed_current_ul));
    win.set_detail_speed_elapsed(SharedString::from(p.speed_elapsed));
}

fn clear_detail_props(win: &crate::MainWindow) {
    win.set_detail_info_hash(SharedString::default());
    win.set_detail_name(SharedString::default());
    win.set_detail_error(SharedString::default());
    win.set_detail_state_label(SharedString::default());
    win.set_detail_down_rate(SharedString::from("\u{2014}"));
    win.set_detail_up_rate(SharedString::from("\u{2014}"));
    win.set_detail_all_time_down(SharedString::from("0 B"));
    win.set_detail_all_time_up(SharedString::from("0 B"));
    win.set_detail_ratio(SharedString::from("0.00"));
    win.set_detail_eta(SharedString::from("\u{2014}"));
    win.set_detail_num_peers(SharedString::from("0 / 0"));
    win.set_detail_num_seeds(SharedString::from("0"));
    win.set_detail_share_fraction(SharedString::from("\u{2014}"));
    win.set_detail_save_path(SharedString::default());
    win.set_detail_total_size(SharedString::default());
    win.set_detail_piece_count_text(SharedString::default());
    win.set_detail_piece_size(SharedString::default());
    win.set_detail_added_time(SharedString::default());
    win.set_detail_last_seen_complete(SharedString::default());
    win.set_detail_active_duration(SharedString::default());
    win.set_detail_piece_buckets(ModelRc::new(VecModel::from(Vec::<i32>::new())));
    win.set_detail_pieces_text(SharedString::from("0 / 0 pieces (0%)"));
    win.set_detail_sequential(false);
    win.set_detail_files(ModelRc::new(VecModel::from(
        Vec::<crate::FileTreeRow>::new(),
    )));
    win.set_detail_peers(ModelRc::new(VecModel::from(Vec::<crate::PeerRow>::new())));
    win.set_detail_trackers(ModelRc::new(
        VecModel::from(Vec::<crate::TrackerRow>::new()),
    ));
    win.set_detail_http_sources(ModelRc::new(
        VecModel::from(Vec::<crate::WebSeedRow>::new()),
    ));
    win.set_detail_speed_dl_path(SharedString::default());
    win.set_detail_speed_ul_path(SharedString::default());
    win.set_detail_speed_dl_limit(SharedString::from("0"));
    win.set_detail_speed_ul_limit(SharedString::from("0"));
    win.set_detail_speed_max_rate(SharedString::from("\u{2014}"));
    win.set_detail_speed_current_dl(SharedString::from("\u{2014}"));
    win.set_detail_speed_current_ul(SharedString::from("\u{2014}"));
    win.set_detail_speed_elapsed(SharedString::default());
}

// ── Filtering (M173 Lane A) ─────────────────────────────────────────────────

/// Apply a sidebar predicate to a list of summaries, returning a new vec
/// containing only the rows that match.
///
/// `enrich` is the GUI-side hook that augments each summary with the
/// per-tick fields the sidebar needs (`error`, `category`, `tags`,
/// tracker buckets) — see `crate::sidebar::RowView`. The poll loop owns
/// the enrichment because it has access to the session handle (M173 Lane A
/// task A4 plugs in the real implementation; until then the default hook
/// returns the bare-summary `RowView`, which lets `Library::All` /
/// `Library::Active` / `Library::Inactive` / `Library::Paused` /
/// `Library::Seeding` / `Library::Downloading` / `Library::Completed`
/// behave correctly out of the box).
///
/// **Sort-after-filter semantics**: this function only filters. The caller
/// must run [`sort_summaries`] on the returned vec before pushing to the
/// model. A predicate of [`SidebarPredicate::All`] — the default — passes
/// through every row, so the M163 behaviour is preserved unchanged.
pub fn apply_predicate<F>(
    summaries: &[TorrentSummary],
    predicate: &SidebarPredicate,
    enrich: F,
) -> Vec<TorrentSummary>
where
    F: Fn(&TorrentSummary) -> RowView,
{
    if matches!(predicate, SidebarPredicate::All) {
        return summaries.to_vec();
    }
    summaries
        .iter()
        .filter(|s| predicate.matches(&enrich(s)))
        .cloned()
        .collect()
}

/// The minimal `enrich` hook used by `apply_predicate` when no richer
/// data is available — the A3 baseline. Production poll loop (A8) uses
/// a richer closure that consults `TorrentStats` / `tracker_list`, so
/// the helper is now used only by tests; kept public so future
/// callers (e.g. lightweight unit tests against a synthetic summary
/// stream) have a stable entry point.
#[must_use]
#[allow(dead_code)]
pub fn enrich_summary_only(s: &TorrentSummary) -> RowView {
    RowView::from_summary(s)
}

// ── Sorting ─────────────────────────────────────────────────────────────────

/// Sort torrent summaries in-place according to the current sort state.
///
/// Uses a secondary key (`info_hash`) for stable tie-breaking.
pub fn sort_summaries(summaries: &mut [TorrentSummary], sort: crate::columns::SortState) {
    summaries.sort_by(|a, b| {
        let cmp = match sort.column {
            ColumnId::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ColumnId::Progress => a
                .progress
                .partial_cmp(&b.progress)
                .unwrap_or(std::cmp::Ordering::Equal),
            ColumnId::State => crate::format::format_state(a.state, a.user_seed_mode)
                .cmp(crate::format::format_state(b.state, b.user_seed_mode)),
            ColumnId::DownRate => a.download_rate.cmp(&b.download_rate),
            ColumnId::UpRate => a.upload_rate.cmp(&b.upload_rate),
            ColumnId::Seeds => a.num_seeds.cmp(&b.num_seeds),
            ColumnId::Peers => a.num_peers.cmp(&b.num_peers),
            ColumnId::Eta => {
                // Sort by effective ETA: rate=0 sorts last.
                let eta_a = remaining_bytes(a)
                    .checked_div(a.download_rate)
                    .unwrap_or(u64::MAX);
                let eta_b = remaining_bytes(b)
                    .checked_div(b.download_rate)
                    .unwrap_or(u64::MAX);
                eta_a.cmp(&eta_b)
            }
            ColumnId::Size => a.total_size.cmp(&b.total_size),
            ColumnId::Ratio => {
                let ratio_a = ratio_value(a);
                let ratio_b = ratio_value(b);
                ratio_a
                    .partial_cmp(&ratio_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        };
        // Secondary sort key: info_hash (stable tiebreaker).
        let cmp = cmp.then_with(|| a.info_hash.cmp(&b.info_hash));
        if sort.ascending { cmp } else { cmp.reverse() }
    });
}

fn remaining_bytes(s: &TorrentSummary) -> u64 {
    let done = (s.progress * s.total_size as f64) as u64;
    s.total_size.saturating_sub(done)
}

fn ratio_value(s: &TorrentSummary) -> f64 {
    if s.all_time_download == 0 {
        if s.all_time_upload > 0 {
            f64::INFINITY
        } else {
            0.0
        }
    } else {
        s.all_time_upload as f64 / s.all_time_download as f64
    }
}

// ── Row conversion ──────────────────────────────────────────────────────────

fn to_slint_row(s: &TorrentSummary, selected: &HashSet<String>) -> crate::TorrentRow {
    let remaining = remaining_bytes(s);
    // During recheck, show checking progress instead of download progress.
    let (progress, progress_text) = if s.state == TorrentState::Checking {
        let pct = s.checking_progress * 100.0;
        (s.checking_progress, format!("{pct:.1}%"))
    } else {
        (s.progress as f32, format!("{:.1}%", s.progress * 100.0))
    };
    crate::TorrentRow {
        info_hash: SharedString::from(&s.info_hash),
        name: SharedString::from(&s.name),
        total_size: SharedString::from(crate::format::format_size(s.total_size)),
        progress,
        progress_text: SharedString::from(progress_text),
        down_rate: if s.download_rate > 0 {
            SharedString::from(crate::format::format_rate(s.download_rate))
        } else {
            SharedString::from("\u{2014}")
        },
        up_rate: if s.upload_rate > 0 {
            SharedString::from(crate::format::format_rate(s.upload_rate))
        } else {
            SharedString::from("\u{2014}")
        },
        seeds: SharedString::from(s.num_seeds.to_string()),
        peers: SharedString::from(s.num_peers.to_string()),
        eta: SharedString::from(crate::format::format_eta(remaining, s.download_rate)),
        state: if s.state == TorrentState::Checking {
            SharedString::from(format!("checking ({:.1}%)", s.checking_progress * 100.0))
        } else {
            // v0.187.3 / Bug 17: surface "Super Seeding" in the state column
            // when the BEP 16 super_seeding flag is set on the engine. Use
            // `format_state_full` (it accepts both seed_mode + super_seeding)
            // so the column matches what the detail-pane header shows.
            SharedString::from(crate::format::format_state_full(
                s.state,
                s.user_seed_mode,
                s.super_seeding,
            ))
        },
        state_color: state_color(s.state, s.user_seed_mode),
        ratio: SharedString::from(crate::format::format_ratio(
            s.all_time_upload,
            s.all_time_download,
        )),
        selected: selected.contains(&s.info_hash),
    }
}

// ── State color mapping ─────────────────────────────────────────────────────

/// Map a `TorrentState` to a display color.
///
/// When `user_seed_mode` is true and the torrent is `Downloading`, returns
/// purple (`#ab47bc`) — the same colour as `Sharing` — to visually indicate
/// the seed-only constraint.
pub fn state_color(state: TorrentState, user_seed_mode: bool) -> slint::Color {
    if user_seed_mode && matches!(state, TorrentState::Downloading) {
        return slint::Color::from_rgb_u8(0xab, 0x47, 0xbc); // #ab47bc (purple)
    }
    match state {
        TorrentState::Downloading => slint::Color::from_rgb_u8(0x4c, 0xaf, 0x50), // #4caf50
        TorrentState::Seeding => slint::Color::from_rgb_u8(0x21, 0x96, 0xf3),     // #2196f3
        TorrentState::Complete => slint::Color::from_rgb_u8(0x66, 0xbb, 0x6a),    // #66bb6a
        TorrentState::Paused => slint::Color::from_rgb_u8(0x9e, 0x9e, 0x9e),      // #9e9e9e
        TorrentState::Queued => slint::Color::from_rgb_u8(0xff, 0xb3, 0x00),      // #ffb300 amber
        TorrentState::Checking | TorrentState::FetchingMetadata => {
            slint::Color::from_rgb_u8(0xff, 0x98, 0x00) // #ff9800
        }
        TorrentState::Stopped => slint::Color::from_rgb_u8(0xf4, 0x43, 0x36), // #f44336
        TorrentState::Sharing => slint::Color::from_rgb_u8(0xab, 0x47, 0xbc), // #ab47bc
    }
}

// ── Diff helper ─────────────────────────────────────────────────────────────

/// Returns `true` if any visible field differs between two rows.
///
/// Used for incremental model updates — only calls `set_row_data` when
/// the row has actually changed.
#[allow(
    clippy::float_cmp,
    reason = "exact bitwise comparison for UI change detection"
)]
pub fn rows_differ(a: &crate::TorrentRow, b: &crate::TorrentRow) -> bool {
    a.progress != b.progress
        || a.down_rate != b.down_rate
        || a.up_rate != b.up_rate
        || a.state != b.state
        || a.selected != b.selected
        || a.peers != b.peers
        || a.seeds != b.seeds
        || a.eta != b.eta
        || a.ratio != b.ratio
        || a.name != b.name
}

// ── Aggregate rate helper ───────────────────────────────────────────────────

/// Sum download and upload rates across all summaries.
pub fn aggregate_rates(summaries: &[TorrentSummary]) -> (u64, u64) {
    let down: u64 = summaries.iter().map(|s| s.download_rate).sum();
    let up: u64 = summaries.iter().map(|s| s.upload_rate).sum();
    (down, up)
}

#[must_use]
pub fn count_states(summaries: &[TorrentSummary]) -> (i32, i32, i32) {
    let mut downloading = 0i32;
    let mut seeding = 0i32;
    let mut error = 0i32;
    for s in summaries {
        match s.state {
            TorrentState::Downloading | TorrentState::FetchingMetadata | TorrentState::Checking => {
                downloading += 1;
            }
            TorrentState::Seeding | TorrentState::Complete | TorrentState::Sharing => {
                seeding += 1;
            }
            TorrentState::Stopped => {
                error += 1;
            }
            TorrentState::Paused | TorrentState::Queued => {}
        }
    }
    (downloading, seeding, error)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::SortState;

    fn test_summary(name: &str, hash: &str) -> TorrentSummary {
        TorrentSummary {
            info_hash: hash.to_owned(),
            name: name.to_owned(),
            state: TorrentState::Downloading,
            progress: 0.5,
            download_rate: 0,
            upload_rate: 0,
            total_size: 1_000_000,
            num_peers: 5,
            added_time: 0,
            num_seeds: 2,
            all_time_upload: 0,
            all_time_download: 0,
            user_seed_mode: false,
            super_seeding: false,
            user_forced: false,
            checking_progress: 0.0,
        }
    }

    #[test]
    fn test_sort_by_name() {
        let mut summaries = vec![
            test_summary("Charlie", "ccc"),
            test_summary("Alpha", "aaa"),
            test_summary("Bravo", "bbb"),
        ];

        // Ascending by name.
        let sort = SortState {
            column: ColumnId::Name,
            ascending: true,
        };
        sort_summaries(&mut summaries, sort);
        assert_eq!(summaries[0].name, "Alpha");
        assert_eq!(summaries[1].name, "Bravo");
        assert_eq!(summaries[2].name, "Charlie");

        // Descending by name.
        let sort = SortState {
            column: ColumnId::Name,
            ascending: false,
        };
        sort_summaries(&mut summaries, sort);
        assert_eq!(summaries[0].name, "Charlie");
        assert_eq!(summaries[1].name, "Bravo");
        assert_eq!(summaries[2].name, "Alpha");
    }

    #[test]
    fn test_sort_by_rate_numeric() {
        let mut summaries = vec![
            {
                let mut s = test_summary("A", "aaa");
                s.download_rate = 100;
                s
            },
            {
                let mut s = test_summary("B", "bbb");
                s.download_rate = 50;
                s
            },
            {
                let mut s = test_summary("C", "ccc");
                s.download_rate = 200;
                s
            },
        ];

        let sort = SortState {
            column: ColumnId::DownRate,
            ascending: true,
        };
        sort_summaries(&mut summaries, sort);
        assert_eq!(summaries[0].download_rate, 50);
        assert_eq!(summaries[1].download_rate, 100);
        assert_eq!(summaries[2].download_rate, 200);
    }

    #[test]
    fn test_sort_stability() {
        let mut summaries = vec![
            {
                let mut s = test_summary("A", "bbb");
                s.download_rate = 100;
                s
            },
            {
                let mut s = test_summary("B", "aaa");
                s.download_rate = 100;
                s
            },
        ];

        let sort = SortState {
            column: ColumnId::DownRate,
            ascending: true,
        };
        sort_summaries(&mut summaries, sort);
        // Same rate — tiebroken by info_hash ascending.
        assert_eq!(summaries[0].info_hash, "aaa");
        assert_eq!(summaries[1].info_hash, "bbb");
    }

    #[test]
    fn test_to_slint_rows_zero_values() {
        let s = test_summary("Test", "abc");
        let selected = HashSet::new();
        let row = to_slint_row(&s, &selected);

        // Rate 0 shows em dash.
        assert_eq!(row.down_rate, SharedString::from("\u{2014}"));
        // Ratio 0/0 shows "0.00".
        assert_eq!(row.ratio, SharedString::from("0.00"));
        // ETA with rate 0 shows em dash.
        assert_eq!(row.eta, SharedString::from("\u{2014}"));
    }

    #[test]
    fn test_state_color_mapping() {
        let cases = [
            (TorrentState::Downloading, (0x4c, 0xaf, 0x50)),
            (TorrentState::Seeding, (0x21, 0x96, 0xf3)),
            (TorrentState::Complete, (0x66, 0xbb, 0x6a)),
            (TorrentState::Paused, (0x9e, 0x9e, 0x9e)),
            (TorrentState::Queued, (0xff, 0xb3, 0x00)),
            (TorrentState::Checking, (0xff, 0x98, 0x00)),
            (TorrentState::FetchingMetadata, (0xff, 0x98, 0x00)),
            (TorrentState::Stopped, (0xf4, 0x43, 0x36)),
            (TorrentState::Sharing, (0xab, 0x47, 0xbc)),
        ];

        for (state, (r, g, b)) in cases {
            let color = state_color(state, false);
            assert_eq!(color.red(), r, "red mismatch for {state:?}");
            assert_eq!(color.green(), g, "green mismatch for {state:?}");
            assert_eq!(color.blue(), b, "blue mismatch for {state:?}");
        }
    }

    #[test]
    fn test_state_color_seed_mode() {
        // Downloading + seed mode → purple (#ab47bc), same as Sharing.
        let color = state_color(TorrentState::Downloading, true);
        assert_eq!(color.red(), 0xab);
        assert_eq!(color.green(), 0x47);
        assert_eq!(color.blue(), 0xbc);

        // Seeding + seed mode → normal seeding colour (seed mode only
        // overrides Downloading).
        let color = state_color(TorrentState::Seeding, true);
        assert_eq!(color.red(), 0x21);
        assert_eq!(color.green(), 0x96);
        assert_eq!(color.blue(), 0xf3);
    }

    #[test]
    fn test_aggregate_rates() {
        let summaries = vec![
            {
                let mut s = test_summary("A", "aaa");
                s.download_rate = 100;
                s.upload_rate = 10;
                s
            },
            {
                let mut s = test_summary("B", "bbb");
                s.download_rate = 200;
                s.upload_rate = 20;
                s
            },
            {
                let mut s = test_summary("C", "ccc");
                s.download_rate = 300;
                s.upload_rate = 30;
                s
            },
        ];

        let (down, up) = aggregate_rates(&summaries);
        assert_eq!(down, 600);
        assert_eq!(up, 60);
    }

    // ── M173 Lane A: apply_predicate (sort-after-filter rebuild) ───────

    use crate::sidebar::{LibraryFilter, SidebarPredicate};

    #[test]
    fn apply_predicate_all_is_passthrough() {
        let summaries = vec![
            test_summary("A", "aaa"),
            test_summary("B", "bbb"),
            test_summary("C", "ccc"),
        ];
        let out = apply_predicate(&summaries, &SidebarPredicate::All, enrich_summary_only);
        assert_eq!(out.len(), 3);
        // Rebuild preserves input order — sorting happens AFTER filter.
        assert_eq!(out[0].info_hash, "aaa");
        assert_eq!(out[1].info_hash, "bbb");
        assert_eq!(out[2].info_hash, "ccc");
    }

    #[test]
    fn apply_predicate_paused_filters_out_downloading() {
        let mut a = test_summary("A", "aaa");
        a.state = TorrentState::Downloading;
        let mut b = test_summary("B", "bbb");
        b.state = TorrentState::Paused;
        let mut c = test_summary("C", "ccc");
        c.state = TorrentState::Paused;
        let summaries = vec![a, b, c];
        let pred = SidebarPredicate::Library(LibraryFilter::Paused);
        let out = apply_predicate(&summaries, &pred, enrich_summary_only);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|s| s.state == TorrentState::Paused));
    }

    #[test]
    fn apply_predicate_active_uses_rates() {
        let mut a = test_summary("A", "aaa");
        a.download_rate = 1024;
        let b = test_summary("B", "bbb"); // both rates 0
        let mut c = test_summary("C", "ccc");
        c.upload_rate = 512;
        let summaries = vec![a, b, c];
        let pred = SidebarPredicate::Library(LibraryFilter::Active);
        let out = apply_predicate(&summaries, &pred, enrich_summary_only);
        assert_eq!(out.len(), 2);
        let hashes: Vec<&str> = out.iter().map(|s| s.info_hash.as_str()).collect();
        assert!(hashes.contains(&"aaa"));
        assert!(hashes.contains(&"ccc"));
    }

    #[test]
    fn apply_predicate_custom_enricher_drives_category_filter() {
        let summaries = vec![
            test_summary("A", "aaa"),
            test_summary("B", "bbb"),
            test_summary("C", "ccc"),
        ];
        let pred = SidebarPredicate::Category("Linux".into());
        // Enricher fakes the category via a simple lookup — proves task A4
        // can plug richer data through without changing apply_predicate.
        let out = apply_predicate(&summaries, &pred, |s| {
            let cat = if s.info_hash == "bbb" {
                Some("Linux".to_owned())
            } else {
                None
            };
            crate::sidebar::RowView::from_summary(s).with_category(cat)
        });
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].info_hash, "bbb");
    }

    #[test]
    fn apply_predicate_filter_then_sort_preserves_only_match() {
        let mut a = test_summary("Charlie", "ccc");
        a.state = TorrentState::Paused;
        let mut b = test_summary("Alpha", "aaa");
        b.state = TorrentState::Downloading;
        let mut c = test_summary("Bravo", "bbb");
        c.state = TorrentState::Paused;
        let summaries = vec![a, b, c];
        let pred = SidebarPredicate::Library(LibraryFilter::Paused);
        let mut out = apply_predicate(&summaries, &pred, enrich_summary_only);
        // Verify that callers can sort the filter output correctly.
        let sort = SortState {
            column: ColumnId::Name,
            ascending: true,
        };
        sort_summaries(&mut out, sort);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "Bravo");
        assert_eq!(out[1].name, "Charlie");
    }

    #[test]
    fn test_rows_differ() {
        let row_a = crate::TorrentRow {
            info_hash: SharedString::from("abc"),
            name: SharedString::from("Test"),
            total_size: SharedString::from("1.0 MiB"),
            progress: 0.5,
            progress_text: SharedString::from("50.0%"),
            down_rate: SharedString::from("\u{2014}"),
            up_rate: SharedString::from("\u{2014}"),
            seeds: SharedString::from("2"),
            peers: SharedString::from("5"),
            eta: SharedString::from("\u{2014}"),
            state: SharedString::from("downloading"),
            state_color: slint::Color::from_rgb_u8(0x4c, 0xaf, 0x50),
            ratio: SharedString::from("0.00"),
            selected: false,
        };

        // Identical rows should not differ.
        let row_b = row_a.clone();
        assert!(!rows_differ(&row_a, &row_b));

        // Modify progress — should differ.
        let mut row_c = row_a.clone();
        row_c.progress = 0.75;
        assert!(rows_differ(&row_a, &row_c));
    }
}
