//! Formatting helpers for the torrent list display.
//!
//! Thin `pub(crate)` wrappers around [`irontide_format`] so that existing
//! call sites in the GUI don't need to change. All logic lives in the shared
//! crate and is tested there.

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

pub(crate) fn format_state(state: &TorrentState, user_seed_mode: bool) -> &'static str {
    irontide_format::format_state(state, user_seed_mode)
}
