//! Application state for the `irontide tui` dashboard.
//!
//! The event loop owns exactly one `AppState` and mutates it in-place
//! between draws. Every field is plain data — no interior mutability,
//! no locks — because the loop is single-threaded. Separate async
//! tasks (the WebSocket subscriber, the tick refresher) funnel their
//! results back through `tokio::select!` branches, which mutate state
//! directly when they wake.
//!
//! Expanded-row selection is keyed by info-hash (not list index)
//! because list refreshes shuffle indices when torrents are added or
//! removed. Keying by hash means "the row the user picked" survives
//! every incremental update until the torrent is actually gone.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::client::{PeerInfoDto, TorrentInfoDto, TorrentStatsDto, TorrentSummaryDto};

/// Dashboard state — mutated in place by the event loop.
pub(crate) struct AppState {
    /// Torrents from the most recent `list_torrents` response.
    pub(crate) torrents: Vec<TorrentSummaryDto>,
    /// Currently-selected row index into [`Self::torrents`].
    ///
    /// Clamped to `0..torrents.len()` by every mutation helper.
    pub(crate) selected: usize,
    /// Set of info-hashes whose detail view is expanded. Keying by
    /// hash (not index) keeps expansion stable across list refreshes.
    pub(crate) expanded: HashSet<String>,
    /// Per-torrent cached detail for the currently-expanded row(s).
    /// Re-fetched on selection change or after a 500ms staleness window.
    pub(crate) detail_cache: HashMap<String, CachedDetail>,
    /// Active modal dialog, if any.
    pub(crate) modal: Option<Modal>,
    /// Last error message to render at the bottom of the screen.
    pub(crate) last_error: Option<String>,
    /// Aggregated session download rate (sum of per-torrent rates).
    pub(crate) agg_down: u64,
    /// Aggregated session upload rate (sum of per-torrent rates).
    pub(crate) agg_up: u64,
    /// Exit-on-next-tick flag.
    pub(crate) should_quit: bool,
}

/// A cached snapshot of the details for one torrent.
///
/// `fetched_at` is consulted by the refresh timer to decide whether
/// the cache is stale (>500ms) and needs a re-fetch.
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields are read by ui::draw; clippy can't see through trait objects
pub(crate) struct CachedDetail {
    /// File list + piece geometry.
    pub(crate) info: TorrentInfoDto,
    /// Live stats (rates, progress, seed-mode flag).
    pub(crate) stats: TorrentStatsDto,
    /// Peer list (empty on 404).
    pub(crate) peers: Vec<PeerInfoDto>,
    /// Monotonic timestamp of the fetch — used to gate re-fetches.
    pub(crate) fetched_at: Instant,
}

/// Modal dialog currently on screen.
///
/// The event loop dispatches key events to modal-specific handlers
/// when `AppState::modal` is `Some(_)`, so the main keybind table is
/// only consulted in the "no-modal" state.
#[derive(Debug, Clone)]
pub(crate) enum Modal {
    /// "Add magnet" prompt — the user is typing a magnet URI.
    AddMagnet {
        /// In-progress input buffer.
        input: String,
    },
    /// "Delete torrent?" confirmation.
    ConfirmDelete {
        /// Full v1 info-hash (hex) of the torrent targeted for removal.
        hash: String,
        /// Display name (for the modal body text).
        name: String,
    },
    /// Help overlay listing every keybind.
    Help,
}

impl AppState {
    /// Build a fresh, empty `AppState`.
    pub(crate) fn new() -> Self {
        Self {
            torrents: Vec::new(),
            selected: 0,
            expanded: HashSet::new(),
            detail_cache: HashMap::new(),
            modal: None,
            last_error: None,
            agg_down: 0,
            agg_up: 0,
            should_quit: false,
        }
    }

    /// Move the selection cursor by `delta` rows.
    ///
    /// Clamps to `[0, torrents.len())`. Clamping (rather than
    /// wrapping) is a deliberate choice: vim-style navigation in a
    /// dashboard feels more natural when the cursor sticks to the
    /// extremes, and it mirrors the selection behaviour of `htop`,
    /// `btop`, and rqbit's own TUI.
    ///
    /// If the list is empty, `selected` is reset to `0`.
    pub(crate) fn move_selection(&mut self, delta: isize) {
        if self.torrents.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.torrents.len();
        let current = isize::try_from(self.selected).unwrap_or(0);
        let next = current.saturating_add(delta);
        let clamped = next.clamp(0, isize::try_from(len - 1).unwrap_or(0));
        self.selected = usize::try_from(clamped).unwrap_or(0);
    }

    /// Hash of the currently-selected torrent, if any.
    pub(crate) fn selected_hash(&self) -> Option<&str> {
        self.torrents
            .get(self.selected)
            .map(|t| t.info_hash.as_str())
    }

    /// Toggle the expanded/collapsed state of the selected torrent.
    ///
    /// No-op on an empty list. Idempotent in the sense that two
    /// consecutive calls leave the set in the same state.
    pub(crate) fn toggle_expand(&mut self) {
        let Some(hash) = self.selected_hash().map(ToOwned::to_owned) else {
            return;
        };
        if !self.expanded.remove(&hash) {
            self.expanded.insert(hash);
        }
    }

    /// Replace the torrent list, preserving the user's selection.
    ///
    /// If the previously-selected hash is still present in the new
    /// list, the selection follows it to its new index. If it's gone,
    /// selection resets to `0`. The expanded set is pruned to match
    /// the new list (stale entries are dropped).
    pub(crate) fn replace_torrents(&mut self, list: Vec<TorrentSummaryDto>) {
        let prior_hash = self.selected_hash().map(ToOwned::to_owned);
        self.torrents = list;

        // Remap selection.
        self.selected = prior_hash
            .as_deref()
            .and_then(|h| self.torrents.iter().position(|t| t.info_hash == h))
            .unwrap_or(0);
        if self.selected >= self.torrents.len() {
            self.selected = 0;
        }

        // Prune expanded + cache for hashes no longer present. Collect
        // into an intermediate set first so we're not mutating and
        // reading the map in the same pass.
        let live: HashSet<&str> = self.torrents.iter().map(|t| t.info_hash.as_str()).collect();
        self.expanded.retain(|h| live.contains(h.as_str()));
        self.detail_cache.retain(|h, _| live.contains(h.as_str()));
    }

    /// Record an error message to display in the status line.
    pub(crate) fn set_error(&mut self, msg: impl Into<String>) {
        self.last_error = Some(msg.into());
    }

    /// Clear the current error message (usually on successful tick).
    pub(crate) fn clear_error(&mut self) {
        self.last_error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_torrent(hash: &str, name: &str) -> TorrentSummaryDto {
        // Build DTOs through JSON so we don't have to expose a public
        // constructor on TorrentSummaryDto just for tests.
        let raw = serde_json::json!({
            "info_hash": hash,
            "name": name,
            "state": "Downloading",
            "progress": 0.0,
            "download_rate": 0,
            "upload_rate": 0,
            "total_size": 0,
            "num_peers": 0,
            "added_time": 0,
        });
        serde_json::from_value(raw).expect("test DTO")
    }

    #[test]
    fn test_new_state_is_empty() {
        let s = AppState::new();
        assert!(s.torrents.is_empty());
        assert_eq!(s.selected, 0);
        assert!(s.expanded.is_empty());
        assert!(s.detail_cache.is_empty());
        assert!(s.modal.is_none());
        assert!(s.last_error.is_none());
        assert_eq!(s.agg_down, 0);
        assert_eq!(s.agg_up, 0);
        assert!(!s.should_quit);
    }

    #[test]
    fn test_move_selection_wraps_or_clamps() {
        // Clamping semantics (not wrapping).
        let mut s = AppState::new();
        s.torrents = vec![make_torrent("a", "A"), make_torrent("b", "B")];

        // Down from 0 → 1.
        s.move_selection(1);
        assert_eq!(s.selected, 1);
        // Down from 1 → stays at 1 (clamp).
        s.move_selection(1);
        assert_eq!(s.selected, 1);
        // Up from 1 → 0.
        s.move_selection(-1);
        assert_eq!(s.selected, 0);
        // Up from 0 → stays at 0 (clamp).
        s.move_selection(-1);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn test_move_selection_on_empty_list() {
        let mut s = AppState::new();
        s.move_selection(5);
        assert_eq!(s.selected, 0);
        s.move_selection(-5);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn test_selected_hash_returns_none_when_empty() {
        let s = AppState::new();
        assert!(s.selected_hash().is_none());
    }

    #[test]
    fn test_selected_hash_returns_active_hash() {
        let mut s = AppState::new();
        s.torrents = vec![make_torrent("a", "A"), make_torrent("b", "B")];
        s.selected = 1;
        assert_eq!(s.selected_hash(), Some("b"));
    }

    #[test]
    fn test_replace_torrents_preserves_selection_by_hash() {
        let mut s = AppState::new();
        s.torrents = vec![make_torrent("a", "A"), make_torrent("b", "B")];
        s.selected = 1; // pointing at "b"

        // New list reorders "b" to index 0 and keeps "a".
        s.replace_torrents(vec![make_torrent("b", "B"), make_torrent("a", "A")]);
        assert_eq!(
            s.selected, 0,
            "selection should follow hash 'b' to its new index"
        );

        // Now remove "b" entirely — selection should reset to 0.
        s.replace_torrents(vec![make_torrent("a", "A")]);
        assert_eq!(s.selected, 0, "selection should reset when target removed");
    }

    #[test]
    fn test_replace_torrents_prunes_expanded_set() {
        let mut s = AppState::new();
        s.torrents = vec![make_torrent("a", "A"), make_torrent("b", "B")];
        s.expanded.insert("a".to_owned());
        s.expanded.insert("b".to_owned());

        s.replace_torrents(vec![make_torrent("a", "A")]);
        assert!(s.expanded.contains("a"));
        assert!(
            !s.expanded.contains("b"),
            "stale expansion should be pruned"
        );
    }

    #[test]
    fn test_toggle_expand_idempotent() {
        let mut s = AppState::new();
        s.torrents = vec![make_torrent("a", "A")];
        assert!(!s.expanded.contains("a"));
        s.toggle_expand();
        assert!(s.expanded.contains("a"));
        s.toggle_expand();
        assert!(!s.expanded.contains("a"));
        // Toggling twice gets us back to the original state.
        s.toggle_expand();
        s.toggle_expand();
        assert!(!s.expanded.contains("a"));
    }

    #[test]
    fn test_toggle_expand_noop_on_empty() {
        let mut s = AppState::new();
        s.toggle_expand(); // should not panic
        assert!(s.expanded.is_empty());
    }

    #[test]
    fn test_set_and_clear_error() {
        let mut s = AppState::new();
        s.set_error("daemon unreachable");
        assert_eq!(s.last_error.as_deref(), Some("daemon unreachable"));
        s.clear_error();
        assert!(s.last_error.is_none());
    }
}
