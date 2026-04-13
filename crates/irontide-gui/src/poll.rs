//! 500ms polling loop that fetches torrent data from the session,
//! formats it into Slint `TorrentRow` structs, and pushes updates
//! to the main window.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use slint::{Model, ModelRc, SharedString, VecModel};

use irontide::session::{SessionHandle, TorrentState, TorrentSummary};

use crate::columns::ColumnId;

// Thread-local storage for the VecModel so the upgrade_in_event_loop
// closure can access it without moving it into each closure.
thread_local! {
    static TORRENT_MODEL: RefCell<Option<Rc<VecModel<crate::TorrentRow>>>> = const { RefCell::new(None) };
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

/// Initialise the thread-local torrent model and bind it to the window.
///
/// Must be called on the Slint main thread (inside `upgrade_in_event_loop`)
/// exactly once during startup.
pub fn init_model(win: &crate::MainWindow) {
    let model = Rc::new(VecModel::<crate::TorrentRow>::default());
    TORRENT_MODEL.with(|m| *m.borrow_mut() = Some(model.clone()));
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
        .map(|s| s.listen_port as i32)
        .unwrap_or(0);

    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Fetch data — on failure, keep last UI, log warning.
        let summaries = match session.list_torrent_summaries().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("poll: list_torrent_summaries failed: {e}");
                continue;
            }
        };
        let sess_stats = match session.session_stats().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("poll: session_stats failed: {e}");
                continue;
            }
        };

        // Read sort + selection state.
        let (sort, selected) = {
            let st = state.lock();
            (st.sort, st.selected.clone())
        };

        // Sort raw summaries.
        let mut sorted = summaries;
        sort_summaries(&mut sorted, &sort);

        // Convert to Slint rows.
        let new_rows: Vec<crate::TorrentRow> =
            sorted.iter().map(|s| to_slint_row(s, &selected)).collect();

        // Update current_order in state (for shift-click range).
        {
            let mut st = state.lock();
            st.current_order = sorted.iter().map(|s| s.info_hash.clone()).collect();
        }

        // Compute status bar values.
        let (agg_down, agg_up) = aggregate_rates(&sorted);
        let total_torrents = sorted.len() as i32;
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
            win.set_sort_column(sort_col);
            win.set_sort_ascending(sort_asc);
        });
    }
}

// ── Sorting ─────────────────────────────────────────────────────────────────

/// Sort torrent summaries in-place according to the current sort state.
///
/// Uses a secondary key (info_hash) for stable tie-breaking.
pub fn sort_summaries(summaries: &mut [TorrentSummary], sort: &crate::columns::SortState) {
    summaries.sort_by(|a, b| {
        let cmp = match sort.column {
            ColumnId::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ColumnId::Progress => a
                .progress
                .partial_cmp(&b.progress)
                .unwrap_or(std::cmp::Ordering::Equal),
            ColumnId::State => {
                crate::format::format_state(&a.state).cmp(crate::format::format_state(&b.state))
            }
            ColumnId::DownRate => a.download_rate.cmp(&b.download_rate),
            ColumnId::UpRate => a.upload_rate.cmp(&b.upload_rate),
            ColumnId::Seeds => a.num_seeds.cmp(&b.num_seeds),
            ColumnId::Peers => a.num_peers.cmp(&b.num_peers),
            ColumnId::Eta => {
                // Sort by effective ETA: rate=0 sorts last.
                let eta_a = if a.download_rate > 0 {
                    remaining_bytes(a) / a.download_rate
                } else {
                    u64::MAX
                };
                let eta_b = if b.download_rate > 0 {
                    remaining_bytes(b) / b.download_rate
                } else {
                    u64::MAX
                };
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
    crate::TorrentRow {
        info_hash: SharedString::from(&s.info_hash),
        name: SharedString::from(&s.name),
        total_size: SharedString::from(crate::format::format_size(s.total_size)),
        progress: s.progress as f32,
        progress_text: SharedString::from(format!("{:.1}%", s.progress * 100.0)),
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
        state: SharedString::from(crate::format::format_state(&s.state)),
        state_color: state_color(&s.state),
        ratio: SharedString::from(crate::format::format_ratio(
            s.all_time_upload,
            s.all_time_download,
        )),
        selected: selected.contains(&s.info_hash),
    }
}

// ── State color mapping ─────────────────────────────────────────────────────

/// Map a `TorrentState` to a display color.
pub fn state_color(state: &TorrentState) -> slint::Color {
    match state {
        TorrentState::Downloading => slint::Color::from_rgb_u8(0x4c, 0xaf, 0x50), // #4caf50
        TorrentState::Seeding => slint::Color::from_rgb_u8(0x21, 0x96, 0xf3),     // #2196f3
        TorrentState::Complete => slint::Color::from_rgb_u8(0x66, 0xbb, 0x6a),    // #66bb6a
        TorrentState::Paused => slint::Color::from_rgb_u8(0x9e, 0x9e, 0x9e),      // #9e9e9e
        TorrentState::Checking => slint::Color::from_rgb_u8(0xff, 0x98, 0x00),    // #ff9800
        TorrentState::FetchingMetadata => slint::Color::from_rgb_u8(0xff, 0x98, 0x00), // #ff9800
        TorrentState::Stopped => slint::Color::from_rgb_u8(0xf4, 0x43, 0x36),     // #f44336
        TorrentState::Sharing => slint::Color::from_rgb_u8(0xab, 0x47, 0xbc),     // #ab47bc
    }
}

// ── Diff helper ─────────────────────────────────────────────────────────────

/// Returns `true` if any visible field differs between two rows.
///
/// Used for incremental model updates — only calls `set_row_data` when
/// the row has actually changed.
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
        sort_summaries(&mut summaries, &sort);
        assert_eq!(summaries[0].name, "Alpha");
        assert_eq!(summaries[1].name, "Bravo");
        assert_eq!(summaries[2].name, "Charlie");

        // Descending by name.
        let sort = SortState {
            column: ColumnId::Name,
            ascending: false,
        };
        sort_summaries(&mut summaries, &sort);
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
        sort_summaries(&mut summaries, &sort);
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
        sort_summaries(&mut summaries, &sort);
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
            (TorrentState::Checking, (0xff, 0x98, 0x00)),
            (TorrentState::FetchingMetadata, (0xff, 0x98, 0x00)),
            (TorrentState::Stopped, (0xf4, 0x43, 0x36)),
            (TorrentState::Sharing, (0xab, 0x47, 0xbc)),
        ];

        for (state, (r, g, b)) in cases {
            let color = state_color(&state);
            assert_eq!(color.red(), r, "red mismatch for {state:?}");
            assert_eq!(color.green(), g, "green mismatch for {state:?}");
            assert_eq!(color.blue(), b, "blue mismatch for {state:?}");
        }
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
