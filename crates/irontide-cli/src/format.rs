//! Shared byte-count and rate formatting helpers for CLI output.
//!
//! Extracted from `download.rs` so every CLI mode (batch, REPL, TUI, and
//! the progress renderer) can share a single implementation and keep the
//! user-visible formatting stable.

/// Format a raw byte count as a human-readable size string.
///
/// Uses binary (KiB/MiB/GiB) units with decimal precision matching
/// libtorrent / rqbit conventions. Sub-KiB values are reported as raw
/// bytes, and the largest unit supported is GiB so that very large
/// torrents stay on a single line.
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
/// libtorrent progress output. Sub-KB/s values are reported in raw `B/s`.
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

/// Render a progress fraction as a fixed-width bar string.
///
/// `progress` is clamped to the `[0.0, 1.0]` interval. `width` is the
/// total number of characters in the bar. Filled cells use `█` and empty
/// cells use `░` to match the M159 CLI mockup.
///
/// A zero-width bar returns an empty string (useful for tests).
pub(crate) fn progress_bar(progress: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let clamped = if progress.is_nan() {
        0.0
    } else {
        progress.clamp(0.0, 1.0)
    };
    // Use saturating cast: width is small (typically 20), never overflows.
    let filled = (clamped * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;
    let mut out = String::with_capacity(filled + empty);
    for _ in 0..filled {
        out.push('█');
    }
    for _ in 0..empty {
        out.push('░');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1_048_576), "1.0 MiB");
        assert_eq!(format_size(1_073_741_824), "1.00 GiB");
    }

    #[test]
    fn format_rate_units() {
        assert_eq!(format_rate(0), "0 B/s");
        assert_eq!(format_rate(1024), "1.0 KB/s");
        assert_eq!(format_rate(1_048_576), "1.0 MB/s");
    }

    #[test]
    fn progress_bar_endpoints() {
        assert_eq!(progress_bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(progress_bar(1.0, 10), "██████████");
        assert_eq!(progress_bar(0.5, 10), "█████░░░░░");
    }

    #[test]
    fn progress_bar_clamps_out_of_range() {
        assert_eq!(progress_bar(-0.5, 4), "░░░░");
        assert_eq!(progress_bar(1.5, 4), "████");
        assert_eq!(progress_bar(f64::NAN, 4), "░░░░");
    }

    #[test]
    fn progress_bar_zero_width() {
        assert_eq!(progress_bar(0.5, 0), "");
    }
}
