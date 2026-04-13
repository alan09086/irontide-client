//! Formatting helpers for the torrent list display.
//!
//! Provides human-readable strings for sizes, transfer rates, ETAs,
//! share ratios, and torrent state labels used throughout the GUI.

use irontide::session::TorrentState;

/// Format a raw byte count as a human-readable size string.
///
/// Uses binary (KiB/MiB/GiB) units with decimal precision matching
/// libtorrent / rqbit conventions.
pub(crate) fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Format a byte-per-second rate as a human-readable string.
///
/// Note: rates use the `KB/s` / `MB/s` suffix (without the `i`) to match
/// libtorrent progress output.
pub(crate) fn format_rate(bytes_per_sec: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    if bytes_per_sec >= MIB {
        format!("{:.1} MB/s", bytes_per_sec as f64 / MIB as f64)
    } else if bytes_per_sec >= KIB {
        format!("{:.1} KB/s", bytes_per_sec as f64 / KIB as f64)
    } else {
        format!("{bytes_per_sec} B/s")
    }
}

/// Estimate remaining download time given outstanding bytes and current rate.
///
/// Returns `"—"` (em dash) when the rate is zero. Otherwise formats the
/// duration as `"Xd Yh"`, `"Xh Ym"`, `"Xm Ys"`, or `"Xs"` depending on
/// the magnitude.
pub(crate) fn format_eta(remaining_bytes: u64, rate_bps: u64) -> String {
    if rate_bps == 0 {
        return "\u{2014}".to_string(); // em dash
    }
    let secs = remaining_bytes / rate_bps;
    if secs >= 86400 {
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        format!("{days}d {hours}h")
    } else if secs >= 3600 {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        format!("{hours}h {mins}m")
    } else if secs >= 60 {
        let mins = secs / 60;
        let s = secs % 60;
        format!("{mins}m {s}s")
    } else {
        format!("{secs}s")
    }
}

/// Format the upload/download share ratio.
///
/// Returns `"∞"` when bytes were uploaded but nothing was downloaded,
/// `"0.00"` when both counters are zero, and a two-decimal ratio otherwise.
pub(crate) fn format_ratio(uploaded: u64, downloaded: u64) -> String {
    if downloaded == 0 && uploaded > 0 {
        return "\u{221e}".to_string(); // ∞
    }
    if downloaded == 0 {
        return "0.00".to_string();
    }
    format!("{:.2}", uploaded as f64 / downloaded as f64)
}

/// Map a `TorrentState` variant to its lowercase display label.
pub(crate) fn format_state(state: &TorrentState) -> &'static str {
    match state {
        TorrentState::FetchingMetadata => "fetching metadata",
        TorrentState::Checking => "checking",
        TorrentState::Downloading => "downloading",
        TorrentState::Complete => "complete",
        TorrentState::Seeding => "seeding",
        TorrentState::Paused => "paused",
        TorrentState::Stopped => "stopped",
        TorrentState::Sharing => "sharing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_eta_ranges() {
        // Zero rate → em dash
        assert_eq!(format_eta(1_000_000, 0), "\u{2014}");

        // Seconds only
        assert_eq!(format_eta(30, 1), "30s");

        // Minutes and seconds
        assert_eq!(format_eta(45 * 60 + 12, 1), "45m 12s");

        // Hours and minutes
        assert_eq!(format_eta(2 * 3600 + 15 * 60, 1), "2h 15m");

        // Days and hours
        assert_eq!(format_eta(2 * 86400 + 15 * 3600, 1), "2d 15h");
    }

    #[test]
    fn test_format_ratio() {
        // Normal ratio: 150 / 100 = 1.50
        assert_eq!(format_ratio(150, 100), "1.50");

        // Downloaded zero with upload → infinity
        assert_eq!(format_ratio(500, 0), "\u{221e}");

        // Both zero → 0.00
        assert_eq!(format_ratio(0, 0), "0.00");
    }

    #[test]
    fn test_format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1_048_576), "1.0 MiB");
        assert_eq!(format_size(1_073_741_824), "1.00 GiB");
    }

    #[test]
    fn test_format_state() {
        assert_eq!(
            format_state(&TorrentState::FetchingMetadata),
            "fetching metadata"
        );
        assert_eq!(format_state(&TorrentState::Checking), "checking");
        assert_eq!(format_state(&TorrentState::Downloading), "downloading");
        assert_eq!(format_state(&TorrentState::Complete), "complete");
        assert_eq!(format_state(&TorrentState::Seeding), "seeding");
        assert_eq!(format_state(&TorrentState::Paused), "paused");
        assert_eq!(format_state(&TorrentState::Stopped), "stopped");
        assert_eq!(format_state(&TorrentState::Sharing), "sharing");
    }
}
