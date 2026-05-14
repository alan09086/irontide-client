#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M177 detail-pane formatters: UNIX timestamps and cumulative-seconds durations stay well within i64/u64 range; sign casts are intentional"
)]

//! Formatting helpers for the torrent list display.
//!
//! Thin `pub(crate)` wrappers around [`irontide_format`] so that existing
//! call sites in the GUI don't need to change. All logic lives in the shared
//! crate and is tested there.
//!
//! M177 adds three GUI-only formatters for the detail pane:
//! [`format_relative_time`], [`format_eta_from_rates`], and a duration
//! helper for the General tab's "Active duration" row. The shared
//! [`irontide_format::format_ratio`] / `format_eta` already cover the
//! list-view shape.

use std::time::{SystemTime, UNIX_EPOCH};

use irontide::session::TorrentState;

pub(crate) fn format_size(bytes: u64) -> String {
    irontide_format::format_size(bytes)
}

pub(crate) fn format_rate(bytes_per_sec: u64) -> String {
    irontide_format::format_rate(bytes_per_sec)
}

pub(crate) fn format_eta(remaining_bytes: u64, rate_bps: u64) -> String {
    irontide_format::format_eta(remaining_bytes, rate_bps)
}

pub(crate) fn format_ratio(uploaded: u64, downloaded: u64) -> String {
    irontide_format::format_ratio(uploaded, downloaded)
}

pub(crate) fn format_state(state: TorrentState, user_seed_mode: bool) -> &'static str {
    irontide_format::format_state(&state, user_seed_mode)
}

pub(crate) fn format_state_full(
    state: TorrentState,
    user_seed_mode: bool,
    super_seeding: bool,
) -> &'static str {
    irontide_format::format_state_with_super_seeding(&state, user_seed_mode, super_seeding)
}

/// Format a UNIX timestamp (seconds) as a relative time string.
///
/// `0` (sentinel for "Never") returns `"Never"`. Future timestamps
/// return `"in <duration>"`. Past timestamps return `"<duration> ago"`.
/// Used by the M177 General tab's "Added" / "Last seen complete" rows.
#[must_use]
pub(crate) fn format_relative_time(unix_secs: i64) -> String {
    if unix_secs == 0 {
        return "Never".to_owned();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let delta = now - unix_secs;
    let abs = delta.unsigned_abs();
    let label = if abs < 60 {
        format!("{abs}s")
    } else if abs < 3600 {
        format!("{}m", abs / 60)
    } else if abs < 86_400 {
        format!("{}h", abs / 3600)
    } else if abs < 86_400 * 30 {
        format!("{}d", abs / 86_400)
    } else if abs < 86_400 * 365 {
        format!("{}mo", abs / (86_400 * 30))
    } else {
        format!("{}y", abs / (86_400 * 365))
    };
    if delta >= 0 {
        format!("{label} ago")
    } else {
        format!("in {label}")
    }
}

/// Format an ETA from `(remaining_bytes, rate_bps)` for the General
/// tab. Wrapper over [`irontide_format::format_eta`]; kept here so the
/// detail-pane snapshot push doesn't need to import both helpers.
#[must_use]
pub(crate) fn format_eta_from_rates(remaining_bytes: u64, rate_bps: u64) -> String {
    format_eta(remaining_bytes, rate_bps)
}

/// Format a cumulative-seconds duration (e.g. `TorrentStats::active_duration`)
/// as a compact `Xd Yh` / `Yh Zm` / `Zm Ws` string. Used by the M177
/// General tab's "Active duration" row.
#[must_use]
pub(crate) fn format_duration_secs(secs: i64) -> String {
    if secs <= 0 {
        return "0s".to_owned();
    }
    let secs = secs as u64;
    if secs >= 86_400 {
        let days = secs / 86_400;
        let hours = (secs % 86_400) / 3600;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_relative_time_zero_is_never() {
        assert_eq!(format_relative_time(0), "Never");
    }

    #[test]
    fn format_relative_time_past_uses_ago() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let three_hours_ago = now - 3 * 3600;
        let s = format_relative_time(three_hours_ago);
        assert!(
            s.ends_with(" ago") && s.contains('h'),
            "expected '<n>h ago', got {s}"
        );
    }

    #[test]
    fn format_relative_time_future_uses_in() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let two_days_from_now = now + 2 * 86_400;
        let s = format_relative_time(two_days_from_now);
        assert!(
            s.starts_with("in ") && s.contains('d'),
            "expected 'in <n>d', got {s}"
        );
    }

    #[test]
    fn format_eta_from_rates_zero_rate_returns_em_dash() {
        // The shared helper returns "—" on a zero rate; this wrapper
        // must preserve that.
        assert_eq!(format_eta_from_rates(1_000_000, 0), "\u{2014}");
    }

    #[test]
    fn format_duration_secs_buckets() {
        assert_eq!(format_duration_secs(0), "0s");
        assert_eq!(format_duration_secs(45), "45s");
        assert_eq!(format_duration_secs(125), "2m 5s");
        assert_eq!(format_duration_secs(2 * 3600 + 15 * 60), "2h 15m");
        assert_eq!(format_duration_secs(3 * 86_400 + 5 * 3600), "3d 5h");
    }
}
