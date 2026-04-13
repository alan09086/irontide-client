//! Shared byte-count and rate formatting helpers for CLI output.
//!
//! `format_size` and `format_rate` are thin wrappers around [`irontide_format`]
//! so existing call sites don't need to change. `progress_bar` is CLI-specific
//! and lives here.

/// Format a raw byte count as a human-readable size string.
///
/// Delegates to [`irontide_format::format_size`].
pub(crate) fn format_size(bytes: u64) -> String {
    irontide_format::format_size(bytes)
}

/// Format a byte-per-second rate as a human-readable string.
///
/// Delegates to [`irontide_format::format_rate`].
pub(crate) fn format_rate(bytes_per_sec: u64) -> String {
    irontide_format::format_rate(bytes_per_sec)
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
