#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M177 detail-pane: piece-bucket counts and progress fractions are display-only; truncation/precision-loss is intentional"
)]

//! M177 detail-pane snapshot fetch + helpers.
//!
//! Two pure helpers feed the Slint detail pane every poll tick:
//!
//! * [`bucket_piece_states`] reduces a per-piece state byte slice into
//!   at most `target_cells` Slint rectangles (the General-tab heatmap).
//!   Worst-state-in-bucket dominates so a single missing piece shows
//!   through even at 100K-piece scale.
//!
//! * [`flatten_files`] turns the shared
//!   [`irontide_format::FlatFileEntry`] slice into a depth-tagged
//!   tree-flattened row vec the Slint Content tab can render with a
//!   plain `for row in detail-files`. Folders that the user has
//!   collapsed (their `"{info_hash}/{path}"` key present in
//!   [`crate::app::AppState::detail_expanded`]) hide their children;
//!   default behaviour is *expanded* (D-user-1).
//!
//! [`DetailSnapshot`] is the in-flight bag the poll loop assembles per
//! tick before the `upgrade_in_event_loop` push to Slint.

use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};

use slint::SharedString;

use irontide::core::FilePriority;
use irontide::session::{PeerInfo, TorrentInfo, TorrentStats, TrackerInfo, TrackerStatus, WebSeedStats};
use irontide_format::{FlatFileEntry, is_pseudo_tracker};

use crate::{FileTreeRow, PeerRow, TrackerRow, WebSeedRow};

/// One poll-tick worth of detail-pane data, ready to be projected onto
/// the Slint window properties.
///
/// `info` is held in an [`Option`] because magnets without metadata
/// have no [`TorrentInfo`] yet — the General tab still renders header
/// and Transfer rows from the [`TorrentStats`], and the Content tab
/// renders an empty file list. Step 7 wires the projection.
#[allow(
    dead_code,
    reason = "Step 3 lands the type; Step 5 + 7 read every field"
)]
#[derive(Debug, Clone)]
pub struct DetailSnapshot {
    /// Per-tick stats — drives Transfer rates, peers, error card, etc.
    pub stats: TorrentStats,
    /// Static info dict (only changes on metadata resolve). The poll
    /// loop caches this across ticks to skip re-fetching.
    pub info: Option<TorrentInfo>,
    /// Bucketed piece-availability values for the heatmap. Length is
    /// 0 (pre-metadata torrent) or up to `target_cells` (currently
    /// 512). Each value is `0` (missing), `1` (downloading), `2` (have).
    pub piece_buckets: Vec<i32>,
    /// Flattened-tree file rows for the Content tab.
    pub files: Vec<FileTreeRow>,
}

/// Reduce a per-piece state slice (one byte per piece) into at most
/// `target_cells` `i32` cells suitable for a Slint `[int]` model.
///
/// Empty input → empty output (pre-metadata torrent — Slint renders
/// no cells, just the "X / Y pieces" label above). `target_cells == 0`
/// → empty output (D-eng-5 defensive — caller must clamp themselves
/// otherwise but this guard prevents a divide-by-zero panic).
///
/// When `piece_states.len() <= target_cells` the slice is widened
/// 1:1 (no bucketing). Otherwise each cell aggregates a contiguous
/// window of `bucket_size = ceil(len / target_cells)` pieces; the
/// **worst** state in the bucket wins (`0 > 1 > 2` — missing beats
/// downloading beats have).
#[must_use]
pub fn bucket_piece_states(piece_states: &[u8], target_cells: usize) -> Vec<i32> {
    if piece_states.is_empty() || target_cells == 0 {
        return Vec::new();
    }
    let n = piece_states.len();
    if n <= target_cells {
        return piece_states.iter().map(|&b| i32::from(b)).collect();
    }
    let bucket_size = n.div_ceil(target_cells);
    let mut out = Vec::with_capacity(target_cells);
    let mut start = 0;
    for cell in 0..target_cells {
        let end = ((cell + 1) * bucket_size).min(n);
        if start >= end {
            break;
        }
        let bucket = &piece_states[start..end];
        // Worst-state wins: 0 (missing) > 1 (downloading) > 2 (have).
        let value = if bucket.contains(&0u8) {
            0
        } else if bucket.contains(&1u8) {
            1
        } else {
            2
        };
        out.push(value);
        start = end;
    }
    out
}

/// Aggregate of a folder's child files. Used by [`flatten_files`] to
/// derive folder progress + total size + child count.
#[derive(Debug, Default, Clone, Copy)]
struct FolderAgg {
    size: u64,
    progress: u64,
    file_count: u32,
}

/// Project a flat file list into a depth-tagged tree-flattened row vec.
///
/// Folder rows precede their children in the output Vec. `expanded`
/// holds folder keys (`"{info_hash}/{path}"`) the user has *explicitly
/// collapsed* — semantically the inverse of the field name, kept to
/// match the locked decision D-user-1 in the M177 plan. Default
/// behaviour (key absent) is **expanded**, so a fresh torrent shows
/// every folder and file unfolded; a user click toggles the key into
/// the set to hide that folder's children.
///
/// `selected` (M178 / TODO-1) lists file indices the user has highlighted
/// for the per-file priority popup. Each emitted file row carries its
/// index back to the caller via [`FileTreeRow::index`] and a precomputed
/// `selected` flag drives the row highlight.
///
/// Length-mismatch saturation lives in
/// [`irontide_format::build_flat`]; this helper trusts the input.
#[must_use]
pub fn flatten_files(
    flat: &[FlatFileEntry],
    expanded: &HashSet<String>,
    info_hash: &str,
    selected: &HashSet<usize>,
) -> Vec<FileTreeRow> {
    if flat.is_empty() {
        return Vec::new();
    }

    // Pre-pass: aggregate folder size/progress so folder rows can
    // show meaningful progress + size labels. BTreeMap key by folder
    // PathBuf — deterministic ordering for tests.
    let mut folder_aggs: BTreeMap<PathBuf, FolderAgg> = BTreeMap::new();
    for entry in flat {
        let mut p = entry.path.parent().map(Path::to_path_buf);
        while let Some(folder) = p {
            if folder.as_os_str().is_empty() {
                break;
            }
            let agg = folder_aggs.entry(folder.clone()).or_default();
            agg.size = agg.size.saturating_add(entry.size);
            agg.progress = agg.progress.saturating_add(entry.progress);
            agg.file_count = agg.file_count.saturating_add(1);
            p = folder.parent().map(Path::to_path_buf);
        }
    }

    let mut out: Vec<FileTreeRow> = Vec::with_capacity(flat.len() + folder_aggs.len());
    // Currently-open folder path stack. Each element is the cumulative
    // path from the root, e.g. `["video", "video/extras"]`.
    let mut open_stack: Vec<PathBuf> = Vec::new();
    // If `Some(depth)`, suppress emissions at depth `>= depth` — set
    // when we descend into a collapsed folder. Cleared when the open
    // stack pops back above that depth.
    let mut hidden_from_depth: Option<usize> = None;

    for (file_idx, entry) in flat.iter().enumerate() {
        let folder_components: Vec<&std::ffi::OsStr> = entry
            .path
            .parent()
            .map(|p| {
                p.components()
                    .filter_map(|c| match c {
                        Component::Normal(s) => Some(s),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Find the longest common prefix between open_stack and the
        // new folder ancestor list, so we know where to truncate +
        // where to push new folder rows.
        let mut common = 0;
        while common < open_stack.len() && common < folder_components.len() {
            let stack_part = open_stack[common].file_name().unwrap_or_default();
            if stack_part == folder_components[common] {
                common += 1;
            } else {
                break;
            }
        }
        // Pop folders that the new entry doesn't share. If we had
        // descended into a collapsed folder and have now popped back
        // above its depth, clear the suppression flag.
        open_stack.truncate(common);
        if let Some(d) = hidden_from_depth
            && open_stack.len() < d
        {
            hidden_from_depth = None;
        }

        // Push new folder ancestors, emitting a row for each new one.
        let mut accumulated = open_stack.last().cloned().unwrap_or_default();
        for part in &folder_components[common..] {
            accumulated.push(part);
            let depth = open_stack.len();
            let key = format!("{}/{}", info_hash, accumulated.to_string_lossy());
            let collapsed = expanded.contains(&key);
            let agg = folder_aggs.get(&accumulated).copied().unwrap_or_default();

            if hidden_from_depth.is_none() {
                let progress = if agg.size > 0 {
                    (agg.progress as f64 / agg.size as f64).clamp(0.0, 1.0) as f32
                } else {
                    1.0
                };
                out.push(FileTreeRow {
                    key: SharedString::from(&key),
                    depth: depth as i32,
                    is_folder: true,
                    expanded: !collapsed,
                    name: SharedString::from(&*part.to_string_lossy()),
                    progress,
                    progress_text: SharedString::from(format!(
                        "{:.1}%",
                        f64::from(progress) * 100.0
                    )),
                    size: SharedString::from(crate::format::format_size(agg.size)),
                    priority: SharedString::default(),
                    // Folder rows aren't selectable for priority editing.
                    index: -1,
                    selected: false,
                });
            }

            open_stack.push(accumulated.clone());
            if collapsed && hidden_from_depth.is_none() {
                // Suppress everything at this depth or deeper until
                // the stack pops back above us.
                hidden_from_depth = Some(open_stack.len());
            }
        }

        if hidden_from_depth.is_none() {
            let file_name = entry
                .path
                .file_name()
                .map(std::ffi::OsStr::to_string_lossy)
                .unwrap_or_default()
                .to_string();
            let progress = if entry.size > 0 {
                (entry.progress as f64 / entry.size as f64).clamp(0.0, 1.0) as f32
            } else {
                1.0
            };
            out.push(FileTreeRow {
                key: SharedString::default(),
                depth: open_stack.len() as i32,
                is_folder: false,
                expanded: false,
                name: SharedString::from(file_name),
                progress,
                progress_text: SharedString::from(format!("{:.1}%", f64::from(progress) * 100.0)),
                size: SharedString::from(crate::format::format_size(entry.size)),
                priority: SharedString::from(priority_label(entry.priority)),
                index: file_idx as i32,
                selected: selected.contains(&file_idx),
            });
        }
    }

    out
}

/// M178: project the live `PeerInfo` list into Slint-renderable rows
/// for the Peers tab. Rates and durations format here; the flag string
/// is composed from the shared `irontide_format::peer_flags` helper so
/// the GUI and Web UI render the same glyph set.
#[must_use]
pub fn flatten_peer_rows(peers: &[PeerInfo]) -> Vec<PeerRow> {
    peers
        .iter()
        .map(|p| {
            let flags = irontide_format::peer_flags(p);
            let flag_str: String = flags.iter().map(|(c, _)| *c).collect();
            let tooltip: String = flags
                .iter()
                .map(|(c, t)| format!("{c}: {t}"))
                .collect::<Vec<_>>()
                .join("\n");
            let source_label = format!("{:?}", p.source);
            PeerRow {
                addr: SharedString::from(p.addr.to_string()),
                client: SharedString::from(p.client.clone()),
                flags: SharedString::from(flag_str),
                flags_tooltip: SharedString::from(tooltip),
                down_rate: SharedString::from(crate::format::format_rate(p.download_rate)),
                up_rate: SharedString::from(crate::format::format_rate(p.upload_rate)),
                source: SharedString::from(source_label),
                snubbed: p.snubbed,
                connected: SharedString::from(crate::format::format_duration_secs(
                    p.connected_duration_secs as i64,
                )),
            }
        })
        .collect()
}

/// M178: project the merged tracker list (synthesized pseudo-trackers
/// + real trackers) into Slint-renderable rows.
///
/// Pseudo-trackers (DHT / PeX / LSD — detected via [`is_pseudo_tracker`])
/// get a friendly `tier_label` derived from the URL; real trackers show
/// their tier integer.
#[must_use]
pub fn flatten_tracker_rows(trackers: &[TrackerInfo]) -> Vec<TrackerRow> {
    trackers
        .iter()
        .map(|t| {
            let is_pseudo = is_pseudo_tracker(t);
            let tier_label = if is_pseudo {
                pseudo_label_from_url(&t.url).to_string()
            } else {
                t.tier.to_string()
            };
            let status = match t.status {
                TrackerStatus::NotContacted => "updating",
                TrackerStatus::Working => "working",
                TrackerStatus::Error => "error",
            };
            let peers = t
                .seeders
                .map(|s| s.saturating_add(t.leechers.unwrap_or(0)).to_string())
                .unwrap_or_else(|| "—".to_owned());
            let seeds = t
                .seeders
                .map(|s| s.to_string())
                .unwrap_or_else(|| "—".to_owned());
            let leeches = t
                .leechers
                .map(|s| s.to_string())
                .unwrap_or_else(|| "—".to_owned());
            let next_announce = if is_pseudo {
                "—".to_owned()
            } else {
                crate::format::format_duration_secs(t.next_announce_secs as i64)
            };
            TrackerRow {
                url: SharedString::from(t.url.clone()),
                tier_label: SharedString::from(tier_label),
                status: SharedString::from(status),
                peers: SharedString::from(peers),
                seeds: SharedString::from(seeds),
                leeches: SharedString::from(leeches),
                next_announce: SharedString::from(next_announce),
                is_pseudo,
            }
        })
        .collect()
}

fn pseudo_label_from_url(url: &str) -> &'static str {
    if url.contains("[DHT]") {
        "DHT"
    } else if url.contains("[PeX]") {
        "PeX"
    } else if url.contains("[LSD]") {
        "LSD"
    } else {
        "—"
    }
}

/// M178: project the per-URL [`WebSeedStats`] list into Slint-renderable
/// HTTP Sources rows. The Slint side renders a two-line layout per
/// D-eng-8 — `last_error` displays as a subtitle when non-empty.
#[must_use]
pub fn flatten_web_seed_rows(stats: &[WebSeedStats]) -> Vec<WebSeedRow> {
    stats
        .iter()
        .map(|s| {
            let state = match s.state {
                irontide::session::WebSeedState::Idle => "idle",
                irontide::session::WebSeedState::Active => "active",
                irontide::session::WebSeedState::Errored => "errored",
            };
            WebSeedRow {
                url: SharedString::from(s.url.clone()),
                state: SharedString::from(state),
                downloaded: SharedString::from(crate::format::format_size(s.downloaded_bytes)),
                last_rate: SharedString::from(crate::format::format_rate(s.last_rate_bps)),
                last_error: SharedString::from(s.last_error.clone().unwrap_or_default()),
                consecutive_failures: i32::try_from(s.consecutive_failures).unwrap_or(i32::MAX),
            }
        })
        .collect()
}

/// Map a [`FilePriority`] to its display label for the Content tab.
/// M177 is display-only; M178 plumbs the labels into the M164
/// right-click context menu for editing.
#[must_use]
pub fn priority_label(p: FilePriority) -> &'static str {
    match p {
        FilePriority::Skip => "Skip",
        FilePriority::Low => "Low",
        FilePriority::Normal => "Normal",
        FilePriority::High => "High",
    }
}

/// Resolve a folder path to the file indices it contains (recursive).
///
/// `flat` must be in file-index order (as produced by `build_flat`), so
/// positional index == file index. Uses `starts_with` for recursive
/// inclusion of all descendants.
#[must_use]
pub fn collect_folder_file_indices(
    flat: &[FlatFileEntry],
    folder_path: &Path,
) -> Vec<usize> {
    flat.iter()
        .enumerate()
        .filter(|(_, e)| e.path.starts_with(folder_path))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_entry(path: &str, size: u64, progress: u64, p: FilePriority) -> FlatFileEntry {
        FlatFileEntry {
            path: PathBuf::from(path),
            size,
            progress,
            priority: p,
        }
    }

    // ── bucket_piece_states ────────────────────────────────────────────

    #[test]
    fn bucket_empty_input_returns_empty() {
        assert!(bucket_piece_states(&[], 512).is_empty());
    }

    #[test]
    fn bucket_target_zero_returns_empty() {
        // D-eng-5 defensive — must NOT divide by zero.
        let states = vec![0u8, 1, 2, 0, 1];
        assert!(bucket_piece_states(&states, 0).is_empty());
    }

    #[test]
    fn bucket_below_target_widens_one_to_one() {
        let states = vec![0u8, 1, 2, 0, 1];
        let out = bucket_piece_states(&states, 512);
        assert_eq!(out, vec![0i32, 1, 2, 0, 1]);
    }

    #[test]
    fn bucket_above_target_compresses_to_target_cells() {
        // 1024 pieces compressed to 512 cells → bucket_size = 2.
        let mut states = vec![2u8; 1024];
        states[0] = 0; // first cell becomes "missing"
        states[1023] = 1; // last cell becomes "downloading"
        let out = bucket_piece_states(&states, 512);
        assert_eq!(out.len(), 512);
        assert_eq!(out[0], 0, "first cell must reflect the missing piece");
        assert_eq!(*out.last().unwrap(), 1, "last cell must show downloading");
        // Middle cells stay "have".
        assert_eq!(out[256], 2);
    }

    #[test]
    fn bucket_worst_state_in_bucket_wins() {
        // 8 pieces, 4 cells → bucket_size = 2.
        // [have, have] = 2; [have, downloading] = 1; [missing, have] = 0; [downloading, have] = 1
        let states = vec![2u8, 2, 2, 1, 0, 2, 1, 2];
        let out = bucket_piece_states(&states, 4);
        assert_eq!(out, vec![2i32, 1, 0, 1]);
    }

    #[test]
    fn bucket_all_have_collapses_to_two() {
        // 100_352 = 512 * 196 — divides evenly so we emit all 512 cells.
        let states = vec![2u8; 100_352];
        let out = bucket_piece_states(&states, 512);
        assert_eq!(out.len(), 512);
        assert!(
            out.iter().all(|&v| v == 2),
            "every cell must be have when input is uniform"
        );
    }

    // ── flatten_files ─────────────────────────────────────────────────

    #[test]
    fn flatten_empty_returns_empty() {
        let out = flatten_files(&[], &HashSet::new(), "abcd", &HashSet::new());
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_single_file_no_folder_emits_one_row() {
        let flat = vec![flat_entry("a.bin", 1000, 500, FilePriority::Normal)];
        let out = flatten_files(&flat, &HashSet::new(), "abcd", &HashSet::new());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].depth, 0);
        assert!(!out[0].is_folder);
        assert_eq!(out[0].name.as_str(), "a.bin");
        assert!(
            (out[0].progress - 0.5).abs() < 1e-6,
            "single file at 500/1000 must show 0.5 progress"
        );
        assert_eq!(out[0].priority.as_str(), "Normal");
    }

    #[test]
    fn flatten_three_level_nest_emits_folders_then_files_in_order() {
        let flat = vec![
            flat_entry("readme.txt", 100, 100, FilePriority::Normal),
            flat_entry("video/intro.mp4", 50_000, 25_000, FilePriority::High),
            flat_entry("video/extras/bts.mkv", 80_000, 0, FilePriority::Skip),
        ];
        let out = flatten_files(&flat, &HashSet::new(), "abcd", &HashSet::new());
        // Expected emission order:
        //   [0] file readme.txt        depth=0
        //   [1] folder video           depth=0  expanded
        //   [2] file intro.mp4         depth=1
        //   [3] folder extras          depth=1  expanded
        //   [4] file bts.mkv           depth=2
        assert_eq!(out.len(), 5);
        assert!(!out[0].is_folder);
        assert_eq!(out[0].name.as_str(), "readme.txt");
        assert_eq!(out[0].depth, 0);

        assert!(out[1].is_folder);
        assert_eq!(out[1].name.as_str(), "video");
        assert_eq!(out[1].depth, 0);
        assert!(out[1].expanded);

        assert!(!out[2].is_folder);
        assert_eq!(out[2].name.as_str(), "intro.mp4");
        assert_eq!(out[2].depth, 1);

        assert!(out[3].is_folder);
        assert_eq!(out[3].name.as_str(), "extras");
        assert_eq!(out[3].depth, 1);

        assert!(!out[4].is_folder);
        assert_eq!(out[4].name.as_str(), "bts.mkv");
        assert_eq!(out[4].depth, 2);

        // Folder keys for D-eng-4 cleanup test in Step 6.
        assert_eq!(out[1].key.as_str(), "abcd/video");
        assert_eq!(out[3].key.as_str(), "abcd/video/extras");
    }

    #[test]
    fn flatten_collapsed_folder_hides_children() {
        let flat = vec![
            flat_entry("readme.txt", 100, 100, FilePriority::Normal),
            flat_entry("video/intro.mp4", 50_000, 25_000, FilePriority::High),
            flat_entry("video/extras/bts.mkv", 80_000, 0, FilePriority::Skip),
        ];
        // User collapsed the "video" folder.
        let mut expanded = HashSet::new();
        expanded.insert("abcd/video".to_string());
        let out = flatten_files(&flat, &expanded, "abcd", &HashSet::new());
        // Expected: readme.txt + folder video (collapsed marker), no
        // intro.mp4 / extras / bts.mkv.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name.as_str(), "readme.txt");
        assert_eq!(out[1].name.as_str(), "video");
        assert!(out[1].is_folder);
        assert!(!out[1].expanded, "collapsed folder must report expanded=false");
    }

    #[test]
    fn flatten_aggregates_folder_progress() {
        // Folder "video" has two files: 50k/100k progress and 0/100k progress
        // → total 50k / 200k = 0.25 progress.
        let flat = vec![
            flat_entry("video/a.mp4", 100_000, 50_000, FilePriority::Normal),
            flat_entry("video/b.mp4", 100_000, 0, FilePriority::Normal),
        ];
        let out = flatten_files(&flat, &HashSet::new(), "h", &HashSet::new());
        assert_eq!(out[0].name.as_str(), "video");
        assert!(out[0].is_folder);
        assert!(
            (out[0].progress - 0.25).abs() < 1e-3,
            "folder progress must aggregate child progress: got {}",
            out[0].progress
        );
    }

    // ── collect_folder_file_indices (F9) ──────────────────────────────

    #[test]
    fn folder_indices_collects_all_descendants() {
        let flat = vec![
            flat_entry("readme.txt", 100, 100, FilePriority::Normal),
            flat_entry("video/intro.mp4", 50_000, 25_000, FilePriority::High),
            flat_entry("video/extras/bts.mkv", 80_000, 0, FilePriority::Skip),
            flat_entry("audio/track.flac", 10_000, 10_000, FilePriority::Normal),
        ];
        let indices = collect_folder_file_indices(&flat, Path::new("video"));
        assert_eq!(indices, vec![1, 2]);
    }

    #[test]
    fn folder_indices_no_match_returns_empty() {
        let flat = vec![
            flat_entry("readme.txt", 100, 100, FilePriority::Normal),
            flat_entry("video/intro.mp4", 50_000, 25_000, FilePriority::High),
        ];
        let indices = collect_folder_file_indices(&flat, Path::new("audio"));
        assert!(indices.is_empty());
    }

    #[test]
    fn folder_indices_empty_input_returns_empty() {
        let indices = collect_folder_file_indices(&[], Path::new("video"));
        assert!(indices.is_empty());
    }

    // ── priority_label ────────────────────────────────────────────────

    #[test]
    fn priority_label_maps_all_variants() {
        assert_eq!(priority_label(FilePriority::Skip), "Skip");
        assert_eq!(priority_label(FilePriority::Low), "Low");
        assert_eq!(priority_label(FilePriority::Normal), "Normal");
        assert_eq!(priority_label(FilePriority::High), "High");
    }
}
