#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: CLI progress rendering — display formatting and progress bar arithmetic with values bounded by realistic torrent sizes"
)]

//! Pure rendering of `TorrentStatsDto` / `TorrentInfoDto` pairs into
//! either a `Vec<String>` of human-readable lines or a
//! `serde_json::Value` for JSON output.
//!
//! This module intentionally has no I/O and no daemon coupling: the
//! caller passes in already-fetched DTOs and chooses the output shape.
//! That makes every branch trivially unit-testable via `insta`.
//!
//! ## Human format (mirrors the M159 spec)
//!
//! ```text
//! ubuntu-24.04.iso                                        98.3%
//!   ↓ 45.2 MB/s   ↑ 2.1 MB/s   72 peers   ETA 12s   [Downloading]
//!   ├── ubuntu-24.04.iso              [████████████████████] 100.0%  1.4 GiB
//!   └── README.txt                    [████████████████████] 100.0%  4.2 KiB
//! ```
//!
//! ## Smart defaults
//!
//! - Per-file table is emitted only when `info` is `Some` and the
//!   torrent has more than one file.
//! - When a torrent has `> 20` files and the caller did not request
//!   `all_files`, only the `top_n` files ranked by "most in progress"
//!   (closest to 50% incomplete) are shown, with a trailing
//!   `... and N more` line.
//! - ETA is computed as `(total - downloaded) / download_rate` and
//!   rendered as `Xs`, `Xm Ys`, or `Xh Ym`. A zero download rate maps
//!   to `ETA —`.

use serde_json::json;

use crate::client::{FileInfoDto, TorrentInfoDto, TorrentStatsDto};
use crate::format::{format_rate, format_size, progress_bar};

/// Options controlling the human-readable file table.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RenderOpts {
    /// Show all files instead of a top-N slice, regardless of count.
    pub(crate) all_files: bool,
    /// How many files to show when the full list is > 20 and
    /// `all_files` is false. Defaults to `10`.
    pub(crate) top_n: usize,
}

impl Default for RenderOpts {
    fn default() -> Self {
        Self {
            all_files: false,
            top_n: 10,
        }
    }
}

/// Total width of the per-file progress bar, in characters.
const PROGRESS_BAR_WIDTH: usize = 20;

/// Threshold above which `top_n` truncation kicks in.
const MANY_FILES_THRESHOLD: usize = 20;

/// Render the human-readable view as a `Vec<String>`.
///
/// The caller is free to `join` the lines with `\n`, print them one at
/// a time, or thread them through `ratatui`. All lines are trimmed of
/// trailing whitespace.
pub(crate) fn render_human(
    stats: &TorrentStatsDto,
    info: Option<&TorrentInfoDto>,
    file_progress: Option<&[u64]>,
    opts: RenderOpts,
) -> Vec<String> {
    let mut lines = Vec::with_capacity(16);

    // ── Header line: name + overall progress percent ────────────────
    let pct = (stats.progress * 100.0).clamp(0.0, 100.0);
    lines.push(format!("{}  {:.1}%", stats.name, pct));

    // ── Stats line ──────────────────────────────────────────────────
    let eta = render_eta(stats);
    let state_label = render_state_label(stats);
    lines.push(format!(
        "  ↓ {}   ↑ {}   {} peers   ETA {}   [{}]",
        format_rate(stats.download_rate),
        format_rate(stats.upload_rate),
        stats.peers_connected,
        eta,
        state_label,
    ));

    // ── Per-file table (optional) ───────────────────────────────────
    if let Some(info) = info
        && info.files.len() > 1
    {
        append_file_table(&mut lines, info, file_progress, opts);
    }

    lines
}

/// Render the JSON view, mirroring `render_human` but returning a
/// `serde_json::Value`. No cleverness — a straightforward object with
/// `stats`, optional `info`, and optional `files` keys.
pub(crate) fn render_json(
    stats: &TorrentStatsDto,
    info: Option<&TorrentInfoDto>,
    file_progress: Option<&[u64]>,
) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    obj.insert(
        "name".to_owned(),
        serde_json::Value::String(stats.name.clone()),
    );
    obj.insert(
        "state".to_owned(),
        serde_json::Value::String(stats.state.clone()),
    );
    obj.insert("progress".to_owned(), json!(stats.progress));
    obj.insert("progress_ppm".to_owned(), json!(stats.progress_ppm));
    obj.insert("downloaded".to_owned(), json!(stats.downloaded));
    obj.insert("uploaded".to_owned(), json!(stats.uploaded));
    obj.insert("total".to_owned(), json!(stats.total));
    obj.insert("download_rate".to_owned(), json!(stats.download_rate));
    obj.insert("upload_rate".to_owned(), json!(stats.upload_rate));
    obj.insert("pieces_have".to_owned(), json!(stats.pieces_have));
    obj.insert("pieces_total".to_owned(), json!(stats.pieces_total));
    obj.insert("peers_connected".to_owned(), json!(stats.peers_connected));
    obj.insert("peers_available".to_owned(), json!(stats.peers_available));
    obj.insert("is_paused".to_owned(), json!(stats.is_paused));
    obj.insert("is_finished".to_owned(), json!(stats.is_finished));
    obj.insert("is_seeding".to_owned(), json!(stats.is_seeding));
    obj.insert("user_seed_mode".to_owned(), json!(stats.user_seed_mode));
    obj.insert(
        "eta".to_owned(),
        serde_json::Value::String(render_eta(stats)),
    );

    if let Some(info) = info {
        let files: Vec<serde_json::Value> = info
            .files
            .iter()
            .enumerate()
            .map(|(idx, f)| {
                let done = file_progress
                    .and_then(|fp| fp.get(idx).copied())
                    .unwrap_or(0);
                json!({
                    "path": f.path,
                    "length": f.length,
                    "downloaded": done,
                    "progress": file_progress_fraction(done, f.length),
                })
            })
            .collect();
        obj.insert("files".to_owned(), serde_json::Value::Array(files));
        obj.insert("total_length".to_owned(), json!(info.total_length));
        obj.insert("piece_length".to_owned(), json!(info.piece_length));
        obj.insert("num_pieces".to_owned(), json!(info.num_pieces));
        obj.insert("private".to_owned(), json!(info.private));
    }

    serde_json::Value::Object(obj)
}

// ─────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────

/// Compute ETA for a downloading torrent, formatted for display.
fn render_eta(stats: &TorrentStatsDto) -> String {
    // Already done? No ETA.
    if stats.is_finished || stats.is_seeding || stats.total <= stats.downloaded {
        return "—".to_owned();
    }
    if stats.download_rate == 0 {
        return "—".to_owned();
    }
    let remaining = stats.total.saturating_sub(stats.downloaded);
    // `download_rate` is already guaranteed non-zero above.
    let secs = remaining / stats.download_rate;
    format_duration_secs(secs)
}

/// Human-friendly duration string: `"42s"`, `"5m 12s"`, `"2h 7m"`.
fn format_duration_secs(secs: u64) -> String {
    if secs < 60 {
        return format!("{secs}s");
    }
    if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        return format!("{m}m {s}s");
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{h}h {m}m")
}

/// Produce the short state label that goes inside `[brackets]` on the
/// stats line. Prefers the explicit `user_seed_mode` / `is_paused`
/// flags over the raw state string so it matches the user's mental
/// model when they've just toggled a mode.
fn render_state_label(stats: &TorrentStatsDto) -> String {
    if stats.is_paused {
        return "Paused".to_owned();
    }
    if stats.user_seed_mode {
        return "Seed-only".to_owned();
    }
    if stats.is_seeding || stats.is_finished {
        return "Seeding".to_owned();
    }
    stats.state.clone()
}

/// Fraction in `[0.0, 1.0]` for a single file. Guards against
/// zero-length files.
fn file_progress_fraction(done: u64, total: u64) -> f64 {
    if total == 0 {
        return 1.0;
    }
    (done as f64 / total as f64).clamp(0.0, 1.0)
}

/// Append the multi-file progress table to `lines`. Applies top-N
/// truncation and tree-character selection.
fn append_file_table(
    lines: &mut Vec<String>,
    info: &TorrentInfoDto,
    file_progress: Option<&[u64]>,
    opts: RenderOpts,
) {
    let files = &info.files;
    let count = files.len();

    // Select which file indices to display and in what order.
    // - all_files OR count <= threshold → original order, all files
    // - otherwise → top_n files ranked by "in-progress activity"
    let (display_indices, trailer): (Vec<usize>, Option<usize>) =
        if opts.all_files || count <= MANY_FILES_THRESHOLD {
            ((0..count).collect(), None)
        } else {
            let top_n = opts.top_n.min(count);
            let mut ranked = build_ranking(files, file_progress);
            ranked.truncate(top_n);
            // Preserve original file order in the output for stability.
            ranked.sort_unstable();
            (ranked, Some(count - top_n))
        };

    // Compute padded name width so the bars line up. Longest path in
    // the *displayed* set — not the full torrent — because truncation
    // already filtered to the interesting subset.
    let name_width = display_indices
        .iter()
        .filter_map(|&i| files.get(i))
        .map(|f| f.path.chars().count())
        .max()
        .unwrap_or(0)
        .min(50); // cap to stop a single huge path from blowing out the line

    for (pos, &idx) in display_indices.iter().enumerate() {
        let Some(file) = files.get(idx) else {
            continue;
        };
        let is_last = pos + 1 == display_indices.len() && trailer.is_none();
        let prefix = if is_last { "└──" } else { "├──" };

        let done = file_progress
            .and_then(|fp| fp.get(idx).copied())
            .unwrap_or(0);
        let frac = file_progress_fraction(done, file.length);
        let bar = progress_bar(frac, PROGRESS_BAR_WIDTH);
        let pct = frac * 100.0;

        // Truncate or pad the path so the bar column lines up.
        let path_col = pad_or_truncate(&file.path, name_width);

        lines.push(format!(
            "  {prefix} {path_col}  [{bar}] {pct:5.1}%  {}",
            format_size(file.length),
        ));
    }

    if let Some(remaining) = trailer {
        lines.push(format!("  └── ... and {remaining} more"));
    }
}

/// Rank files by "amount of work remaining", filtering out files that
/// are either fully complete or have not started at all (unless the
/// set is small enough that we need to pad it out). Returns a sorted
/// vector of indices where the most "in-progress" files come first.
fn build_ranking(files: &[FileInfoDto], file_progress: Option<&[u64]>) -> Vec<usize> {
    let mut in_progress: Vec<(usize, f64)> = files
        .iter()
        .enumerate()
        .filter_map(|(idx, f)| {
            let done = file_progress
                .and_then(|fp| fp.get(idx).copied())
                .unwrap_or(0);
            if f.length == 0 {
                return None;
            }
            if done == 0 || done >= f.length {
                return None;
            }
            let frac = done as f64 / f.length as f64;
            // Rank by "distance from 50%" — files near halfway are
            // the most interesting because they're actively moving.
            // Smaller distance = higher rank.
            let distance = (0.5 - frac).abs();
            Some((idx, distance))
        })
        .collect();

    // Primary sort: smallest distance first (most active).
    in_progress.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // If nothing is in progress, fall back to listing files in order —
    // the caller still wants to see *something*.
    if in_progress.is_empty() {
        return (0..files.len()).collect();
    }

    in_progress.into_iter().map(|(idx, _)| idx).collect()
}

/// Pad `s` with spaces to `width` (measured in `char`s), or truncate
/// with an ellipsis if it's longer.
fn pad_or_truncate(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len == width {
        return s.to_owned();
    }
    if len < width {
        let mut out = String::with_capacity(width);
        out.push_str(s);
        for _ in len..width {
            out.push(' ');
        }
        return out;
    }
    // len > width: truncate with ellipsis if possible.
    if width <= 3 {
        return s.chars().take(width).collect();
    }
    let take = width.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{InfoHashesDto, TorrentStatsDto};

    // ── helpers ─────────────────────────────────────────────────────

    /// Minimal `TorrentStatsDto` builder for tests.
    fn mk_stats() -> TorrentStatsDto {
        // Build via JSON so we don't need to touch the `pub(crate)`
        // DTO constructors — also exercises the real deserialize path.
        let raw = r#"{
            "name":"test.iso",
            "state":"Downloading",
            "progress":0.5,
            "progress_ppm":500000,
            "total_done":500,
            "total_upload":0,
            "total":1000,
            "download_rate":100,
            "upload_rate":0,
            "pieces_have":5,
            "pieces_total":10,
            "peers_connected":3,
            "peers_available":5,
            "is_paused":false,
            "is_finished":false,
            "is_seeding":false,
            "user_seed_mode":false,
            "info_hashes":{"v1":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20],"v2":null}
        }"#;
        serde_json::from_str(raw).expect("parse mk_stats template")
    }

    /// Build `TorrentInfoDto` with `n` files of varying lengths.
    fn mk_info_files(count: usize) -> TorrentInfoDto {
        let mut files = Vec::with_capacity(count);
        let mut total = 0u64;
        for i in 0..count {
            let length = 1024u64 * ((i as u64) + 1);
            total += length;
            files.push(FileInfoDto {
                path: format!("dir/file_{i:03}.bin"),
                length,
            });
        }
        TorrentInfoDto {
            name: "bundle".to_owned(),
            total_length: total,
            piece_length: 16384,
            num_pieces: ((total / 16384) + 1) as u32,
            files,
            private: false,
        }
    }

    fn mk_single_info(name: &str, length: u64) -> TorrentInfoDto {
        TorrentInfoDto {
            name: name.to_owned(),
            total_length: length,
            piece_length: 16384,
            num_pieces: ((length / 16384) + 1) as u32,
            files: vec![FileInfoDto {
                path: name.to_owned(),
                length,
            }],
            private: false,
        }
    }

    #[test]
    fn format_duration_covers_ranges() {
        assert_eq!(format_duration_secs(0), "0s");
        assert_eq!(format_duration_secs(59), "59s");
        assert_eq!(format_duration_secs(60), "1m 0s");
        assert_eq!(format_duration_secs(312), "5m 12s");
        assert_eq!(format_duration_secs(3600), "1h 0m");
        assert_eq!(format_duration_secs(7620), "2h 7m");
    }

    #[test]
    fn pad_or_truncate_shorter() {
        assert_eq!(pad_or_truncate("abc", 6), "abc   ");
    }

    #[test]
    fn pad_or_truncate_exact() {
        assert_eq!(pad_or_truncate("abcdef", 6), "abcdef");
    }

    #[test]
    fn pad_or_truncate_longer() {
        assert_eq!(pad_or_truncate("abcdefghij", 6), "abcde…");
    }

    // ── insta snapshot tests ────────────────────────────────────────

    #[test]
    fn snapshot_single_file_zero_progress() {
        let mut stats = mk_stats();
        stats.name = "ubuntu-24.04.iso".to_owned();
        stats.progress = 0.0;
        stats.progress_ppm = 0;
        stats.downloaded = 0;
        stats.total = 1_500_000_000;
        stats.download_rate = 0;
        stats.pieces_have = 0;
        stats.pieces_total = 5000;
        let info = mk_single_info("ubuntu-24.04.iso", 1_500_000_000);
        let lines = render_human(&stats, Some(&info), None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_single_file_50pct_with_eta() {
        let mut stats = mk_stats();
        stats.name = "ubuntu-24.04.iso".to_owned();
        stats.progress = 0.5;
        stats.progress_ppm = 500_000;
        stats.downloaded = 750_000_000;
        stats.total = 1_500_000_000;
        stats.download_rate = 45 * 1024 * 1024; // 45 MB/s
        stats.upload_rate = 2 * 1024 * 1024; // 2 MB/s
        stats.peers_connected = 72;
        stats.pieces_have = 2500;
        stats.pieces_total = 5000;
        let info = mk_single_info("ubuntu-24.04.iso", 1_500_000_000);
        let lines = render_human(&stats, Some(&info), None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_single_file_seeding() {
        let mut stats = mk_stats();
        stats.name = "ubuntu-24.04.iso".to_owned();
        stats.state = "Seeding".to_owned();
        stats.progress = 1.0;
        stats.progress_ppm = 1_000_000;
        stats.downloaded = 1_500_000_000;
        stats.total = 1_500_000_000;
        stats.uploaded = 300_000_000;
        stats.download_rate = 0;
        stats.upload_rate = 1024 * 1024;
        stats.peers_connected = 5;
        stats.is_seeding = true;
        stats.is_finished = true;
        stats.pieces_have = 5000;
        stats.pieces_total = 5000;
        let info = mk_single_info("ubuntu-24.04.iso", 1_500_000_000);
        let lines = render_human(&stats, Some(&info), None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_multi_file_all_zero() {
        let mut stats = mk_stats();
        stats.name = "collection".to_owned();
        stats.progress = 0.0;
        stats.progress_ppm = 0;
        stats.downloaded = 0;
        stats.total = 1000 * 1024;
        stats.download_rate = 0;
        stats.peers_connected = 0;
        let info = mk_info_files(3);
        let lines = render_human(&stats, Some(&info), None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_multi_file_partial_progress() {
        let mut stats = mk_stats();
        stats.name = "collection".to_owned();
        stats.progress = 0.3;
        stats.progress_ppm = 300_000;
        stats.downloaded = 3_000;
        stats.total = 10_000;
        stats.download_rate = 2048;
        stats.peers_connected = 8;
        let info = mk_info_files(3);
        let file_progress = vec![1024, 512, 0];
        let lines = render_human(
            &stats,
            Some(&info),
            Some(&file_progress),
            RenderOpts::default(),
        );
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_multi_file_some_complete() {
        let mut stats = mk_stats();
        stats.name = "collection".to_owned();
        stats.progress = 0.66;
        stats.progress_ppm = 666_000;
        stats.downloaded = 6144;
        stats.total = 9216;
        stats.download_rate = 1024;
        stats.peers_connected = 5;
        let info = mk_info_files(3);
        // file_0 (1024) complete, file_1 (2048) complete, file_2 (3072) partial
        let file_progress = vec![1024, 2048, 1536];
        let lines = render_human(
            &stats,
            Some(&info),
            Some(&file_progress),
            RenderOpts::default(),
        );
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_many_files_top_n_trailer() {
        let mut stats = mk_stats();
        stats.name = "many".to_owned();
        stats.progress = 0.4;
        stats.progress_ppm = 400_000;
        stats.downloaded = 50_000;
        stats.total = 125_000;
        stats.download_rate = 8192;
        stats.peers_connected = 30;
        let info = mk_info_files(25);
        // Make ~half of them in-progress so ranking has something to chew on.
        let mut file_progress = vec![0u64; 25];
        for (i, fp) in file_progress.iter_mut().enumerate().take(14) {
            // Partial download: 30% through file i.
            let len = 1024u64 * ((i as u64) + 1);
            *fp = len * 3 / 10;
        }
        let opts = RenderOpts {
            all_files: false,
            top_n: 10,
        };
        let lines = render_human(&stats, Some(&info), Some(&file_progress), opts);
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_many_files_all_files() {
        let mut stats = mk_stats();
        stats.name = "many".to_owned();
        stats.progress = 0.4;
        stats.progress_ppm = 400_000;
        stats.downloaded = 50_000;
        stats.total = 125_000;
        stats.download_rate = 8192;
        stats.peers_connected = 30;
        let info = mk_info_files(25);
        let file_progress = vec![0u64; 25];
        let opts = RenderOpts {
            all_files: true,
            top_n: 10,
        };
        let lines = render_human(&stats, Some(&info), Some(&file_progress), opts);
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_paused() {
        let mut stats = mk_stats();
        stats.name = "ubuntu-24.04.iso".to_owned();
        stats.state = "Paused".to_owned();
        stats.is_paused = true;
        stats.progress = 0.42;
        stats.progress_ppm = 420_000;
        stats.downloaded = 420_000;
        stats.total = 1_000_000;
        stats.download_rate = 0;
        stats.upload_rate = 0;
        stats.peers_connected = 0;
        let lines = render_human(&stats, None, None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_user_seed_mode() {
        let mut stats = mk_stats();
        stats.name = "seedy.iso".to_owned();
        stats.state = "Downloading".to_owned(); // engine state vs user toggle
        stats.user_seed_mode = true;
        stats.progress = 0.75;
        stats.progress_ppm = 750_000;
        stats.downloaded = 750;
        stats.uploaded = 12_345_678;
        stats.total = 1000;
        stats.download_rate = 0;
        stats.upload_rate = 512 * 1024;
        stats.peers_connected = 4;
        let lines = render_human(&stats, None, None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_finished_seeding_flags() {
        let mut stats = mk_stats();
        stats.name = "complete.iso".to_owned();
        stats.state = "Seeding".to_owned();
        stats.is_finished = true;
        stats.is_seeding = true;
        stats.progress = 1.0;
        stats.progress_ppm = 1_000_000;
        stats.downloaded = 1000;
        stats.total = 1000;
        stats.upload_rate = 3 * 1024 * 1024;
        stats.peers_connected = 2;
        let lines = render_human(&stats, None, None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_zero_peers_zero_rates() {
        let mut stats = mk_stats();
        stats.name = "stalled.iso".to_owned();
        stats.progress = 0.15;
        stats.progress_ppm = 150_000;
        stats.downloaded = 150;
        stats.total = 1000;
        stats.download_rate = 0;
        stats.upload_rate = 0;
        stats.peers_connected = 0;
        let lines = render_human(&stats, None, None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_very_large_bytes() {
        let mut stats = mk_stats();
        stats.name = "huge.iso".to_owned();
        stats.progress = 0.5;
        stats.progress_ppm = 500_000;
        stats.downloaded = 5_000_000_000; // 5 GB
        stats.total = 10_000_000_000; // 10 GB
        stats.download_rate = 50 * 1024 * 1024;
        stats.upload_rate = 10 * 1024 * 1024;
        stats.peers_connected = 100;
        let lines = render_human(&stats, None, None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_missing_info() {
        let mut stats = mk_stats();
        stats.name = "no-info.iso".to_owned();
        stats.progress = 0.66;
        stats.progress_ppm = 660_000;
        stats.downloaded = 660;
        stats.total = 1000;
        stats.download_rate = 1024;
        stats.peers_connected = 2;
        let lines = render_human(&stats, None, None, RenderOpts::default());
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn snapshot_json_multi_file() {
        let mut stats = mk_stats();
        stats.name = "collection".to_owned();
        stats.progress = 0.3;
        stats.progress_ppm = 300_000;
        stats.downloaded = 3_000;
        stats.total = 10_000;
        stats.download_rate = 2048;
        stats.peers_connected = 8;
        let info = mk_info_files(3);
        let file_progress = vec![1024, 512, 0];
        let value = render_json(&stats, Some(&info), Some(&file_progress));
        insta::assert_json_snapshot!(value);
    }

    /// Sanity: unused DTO type `InfoHashesDto` referenced once so
    /// the compiler doesn't flag the test-only import as dead.
    #[test]
    fn hash_dto_roundtrip() {
        let raw = r#"{"v1":[1,2,3],"v2":null}"#;
        let _dto: InfoHashesDto = serde_json::from_str(raw).expect("parse");
    }
}
