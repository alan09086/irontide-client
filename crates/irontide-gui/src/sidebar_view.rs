//! Bridge between [`crate::sidebar`] domain types and the Slint
//! `SidebarRow` wire format (M173 Lane A task A8).
//!
//! All four sidebar sections fan out to the same `SidebarRow` Slint
//! struct (see `ui/organisms/sidebar.slint`). The only per-section
//! shape difference is the dot indicator: Library `Errored` shows a red
//! dot, Tracker buckets show a status-coloured dot, Categories and
//! Tags carry no dot. This module owns those mappings.

use slint::{Color, SharedString};

use crate::SidebarRow;
use crate::accel::sidebar_shortcut_label;
use crate::sidebar::{
    LibraryFilter, RowView, SectionCounts, SidebarPredicate, SidebarSection, TrackerBucket,
};

/// Hex literal kept here so the colour palette stays close to the dot
/// usage. Mirrors the M163 `state_color` table for `Stopped`
/// (`#f44336`) — the same hue users already associate with engine
/// errors in the torrent list.
const ERROR_DOT_HEX: (u8, u8, u8) = (0xf4, 0x43, 0x36);
const WORKING_DOT_HEX: (u8, u8, u8) = (0x4c, 0xaf, 0x50); // #4caf50 (green)
const UNREACHABLE_DOT_HEX: (u8, u8, u8) = (0xff, 0x98, 0x00); // #ff9800 (amber)

/// Build the four `SidebarRow` lists for one poll tick.
///
/// `counts` comes from `SectionCounts::from_rows(rows)` (or equivalently
/// from the latest `TrackerIndex::snapshot()`). `categories` and `tags`
/// are the registry snapshots — the sidebar shows every registered
/// category / tag even when no torrent currently uses it (zero count).
/// `active` is the currently-applied predicate; rows whose predicate
/// matches are flagged `selected: true` so the sidebar paints the
/// highlight.
#[must_use]
pub fn build_sidebar_rows(
    counts: &SectionCounts,
    categories: &[String],
    tags: &[String],
    active: &SidebarPredicate,
) -> SidebarRowSet {
    SidebarRowSet {
        library: build_library_rows(counts, active),
        categories: build_category_rows(counts, categories, active),
        tags: build_tag_rows(counts, tags, active),
        trackers: build_tracker_rows(counts, active),
    }
}

/// Bundle of the four section lists.
#[derive(Debug, Clone)]
pub struct SidebarRowSet {
    /// Library rows in [`LibraryFilter::ORDER`].
    pub library: Vec<SidebarRow>,
    /// Category rows in registry order (case-sensitive alphabetical when
    /// the registry returns them sorted).
    pub categories: Vec<SidebarRow>,
    /// Tag rows.
    pub tags: Vec<SidebarRow>,
    /// Tracker bucket rows in [`TrackerBucket::ORDER`].
    pub trackers: Vec<SidebarRow>,
}

fn build_library_rows(counts: &SectionCounts, active: &SidebarPredicate) -> Vec<SidebarRow> {
    let mut out = Vec::with_capacity(LibraryFilter::ORDER.len());
    for (slot, &filter) in LibraryFilter::ORDER.iter().enumerate() {
        let count = counts.library.get(&filter).copied().unwrap_or(0);
        let section = SidebarSection::Library(filter);
        let is_active = matches_predicate(active, &section);
        let dot_color = if matches!(filter, LibraryFilter::Errored) && count > 0 {
            color_from_rgb(ERROR_DOT_HEX)
        } else {
            Color::default()
        };
        let show_dot = matches!(filter, LibraryFilter::Errored) && count > 0;
        // Slot 1..=8 maps onto the 8 Library filters.
        let shortcut = u8::try_from(slot.saturating_add(1))
            .ok()
            .map(sidebar_shortcut_label)
            .unwrap_or_else(SharedString::new);
        out.push(SidebarRow {
            token: section.to_token().into(),
            label: library_label(filter).into(),
            count: clamped_count(count),
            shortcut,
            show_dot,
            dot_color,
            selected: is_active,
        });
    }
    out
}

fn build_category_rows(
    counts: &SectionCounts,
    categories: &[String],
    active: &SidebarPredicate,
) -> Vec<SidebarRow> {
    let mut out = Vec::with_capacity(categories.len());
    for name in categories {
        let count = counts.categories.get(name).copied().unwrap_or(0);
        let section = SidebarSection::Category(name.clone());
        let is_active = matches_predicate(active, &section);
        out.push(SidebarRow {
            token: section.to_token().into(),
            label: name.clone().into(),
            count: clamped_count(count),
            shortcut: SharedString::new(),
            show_dot: false,
            dot_color: Color::default(),
            selected: is_active,
        });
    }
    out
}

fn build_tag_rows(
    counts: &SectionCounts,
    tags: &[String],
    active: &SidebarPredicate,
) -> Vec<SidebarRow> {
    let mut out = Vec::with_capacity(tags.len());
    for name in tags {
        let count = counts.tags.get(name).copied().unwrap_or(0);
        let section = SidebarSection::Tag(name.clone());
        let is_active = matches_predicate(active, &section);
        out.push(SidebarRow {
            token: section.to_token().into(),
            label: name.clone().into(),
            count: clamped_count(count),
            shortcut: SharedString::new(),
            show_dot: false,
            dot_color: Color::default(),
            selected: is_active,
        });
    }
    out
}

fn build_tracker_rows(counts: &SectionCounts, active: &SidebarPredicate) -> Vec<SidebarRow> {
    let mut out = Vec::with_capacity(TrackerBucket::ORDER.len());
    for &bucket in &TrackerBucket::ORDER {
        let count = counts.trackers.get(&bucket).copied().unwrap_or(0);
        let section = SidebarSection::Tracker(bucket);
        let is_active = matches_predicate(active, &section);
        let (label, dot_rgb) = match bucket {
            TrackerBucket::Working => ("Working", WORKING_DOT_HEX),
            TrackerBucket::Unreachable => ("Unreachable", UNREACHABLE_DOT_HEX),
            TrackerBucket::Error => ("Error", ERROR_DOT_HEX),
        };
        out.push(SidebarRow {
            token: section.to_token().into(),
            label: label.into(),
            count: clamped_count(count),
            shortcut: SharedString::new(),
            show_dot: true,
            dot_color: color_from_rgb(dot_rgb),
            selected: is_active,
        });
    }
    out
}

/// Map a Library filter onto its display label.
fn library_label(filter: LibraryFilter) -> &'static str {
    match filter {
        LibraryFilter::All => "All",
        LibraryFilter::Downloading => "Downloading",
        LibraryFilter::Seeding => "Seeding",
        LibraryFilter::Completed => "Completed",
        LibraryFilter::Paused => "Paused",
        LibraryFilter::Active => "Active",
        LibraryFilter::Inactive => "Inactive",
        LibraryFilter::Errored => "Errored",
    }
}

/// True when `active` is the predicate produced by `section`.
///
/// Special case: `SidebarPredicate::All` and
/// `SidebarPredicate::Library(LibraryFilter::All)` are semantically
/// identical — both pass every row through. The default predicate is
/// the bare `All`, but the user can pick "Library / All" from the
/// sidebar (which produces the `Library(All)` form). Treat them as one
/// row for selection-highlight purposes.
fn matches_predicate(active: &SidebarPredicate, section: &SidebarSection) -> bool {
    let target = SidebarPredicate::from_section(section);
    if *active == target {
        return true;
    }
    matches!(
        (active, &target),
        (
            SidebarPredicate::All,
            SidebarPredicate::Library(LibraryFilter::All)
        ) | (
            SidebarPredicate::Library(LibraryFilter::All),
            SidebarPredicate::All
        )
    )
}

/// Convert a `usize` count to the `i32` Slint expects, saturating at
/// `i32::MAX` to avoid panic on truly absurd inputs.
#[allow(clippy::cast_possible_wrap)]
fn clamped_count(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn color_from_rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::from_rgb_u8(r, g, b)
}

/// Resolve a `Ctrl+1..9` slot index to the matching [`SidebarPredicate`].
///
/// Slots 1..=8 map onto [`LibraryFilter::ORDER`]; slot 9 is reserved for
/// a future binding (currently returns `None`). Callers receive the
/// predicate, run [`crate::app::AppState::set_predicate`] and let the
/// next poll tick rebuild the model.
#[must_use]
pub fn predicate_for_shortcut_slot(slot: u8) -> Option<SidebarPredicate> {
    if !(1..=8).contains(&slot) {
        return None;
    }
    let idx = usize::from(slot.saturating_sub(1));
    LibraryFilter::ORDER
        .get(idx)
        .copied()
        .map(SidebarPredicate::Library)
}

/// Build a [`RowView`] from a [`irontide::session::TorrentStats`] plus
/// the matching [`irontide::session::TrackerInfo`] list. Convenience
/// shim for the poll loop.
#[must_use]
pub fn rich_row_view(
    stats: &irontide::session::TorrentStats,
    trackers: &[irontide::session::TrackerInfo],
) -> RowView {
    RowView::from_stats(stats).with_trackers(trackers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn empty_counts() -> SectionCounts {
        SectionCounts {
            library: LibraryFilter::ORDER.iter().map(|f| (*f, 0)).collect(),
            categories: BTreeMap::new(),
            tags: BTreeMap::new(),
            trackers: TrackerBucket::ORDER.iter().map(|b| (*b, 0)).collect(),
        }
    }

    #[test]
    fn build_library_rows_emits_eight_in_order() {
        let counts = empty_counts();
        let rows = build_library_rows(&counts, &SidebarPredicate::All);
        assert_eq!(rows.len(), 8);
        // First row must be "All" + selected (default predicate).
        assert_eq!(rows[0].label, SharedString::from("All"));
        assert!(rows[0].selected);
        // Subsequent rows: not selected.
        for row in &rows[1..] {
            assert!(!row.selected);
        }
    }

    #[test]
    fn library_errored_dot_only_when_count_positive() {
        let mut counts = empty_counts();
        counts.library.insert(LibraryFilter::Errored, 0);
        let rows = build_library_rows(&counts, &SidebarPredicate::All);
        let errored_row = rows.iter().find(|r| r.label == "Errored").expect("found");
        assert!(!errored_row.show_dot, "no dot when zero errors");

        counts.library.insert(LibraryFilter::Errored, 3);
        let rows = build_library_rows(&counts, &SidebarPredicate::All);
        let errored_row = rows.iter().find(|r| r.label == "Errored").expect("found");
        assert!(errored_row.show_dot, "dot when errors present");
        assert_eq!(errored_row.count, 3);
    }

    #[test]
    fn library_shortcut_slots_match_order() {
        let counts = empty_counts();
        let rows = build_library_rows(&counts, &SidebarPredicate::All);
        for (i, row) in rows.iter().enumerate() {
            // Slot is 1-indexed.
            let expected = sidebar_shortcut_label(u8::try_from(i + 1).unwrap_or(0));
            assert_eq!(row.shortcut, expected, "row {i} shortcut");
        }
    }

    #[test]
    fn build_category_rows_uses_registry_order_with_zero_count() {
        let counts = empty_counts();
        let cats = vec!["Linux".to_owned(), "Music".to_owned()];
        let rows = build_category_rows(&counts, &cats, &SidebarPredicate::All);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "Linux");
        assert_eq!(rows[0].count, 0);
        assert_eq!(rows[1].label, "Music");
    }

    #[test]
    fn build_tag_rows_marks_active_tag() {
        let counts = empty_counts();
        let tags = vec!["hd".to_owned(), "1080p".to_owned()];
        let active = SidebarPredicate::Tag("1080p".into());
        let rows = build_tag_rows(&counts, &tags, &active);
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].selected);
        assert!(rows[1].selected, "1080p tag should be selected");
    }

    #[test]
    fn build_tracker_rows_three_buckets_with_dots() {
        let counts = empty_counts();
        let rows = build_tracker_rows(&counts, &SidebarPredicate::All);
        assert_eq!(rows.len(), 3);
        for row in &rows {
            assert!(row.show_dot, "tracker rows always show a dot");
        }
        assert_eq!(rows[0].label, "Working");
        assert_eq!(rows[1].label, "Unreachable");
        assert_eq!(rows[2].label, "Error");
    }

    #[test]
    fn predicate_for_shortcut_slot_in_range() {
        assert_eq!(
            predicate_for_shortcut_slot(1),
            Some(SidebarPredicate::Library(LibraryFilter::All))
        );
        assert_eq!(
            predicate_for_shortcut_slot(8),
            Some(SidebarPredicate::Library(LibraryFilter::Errored))
        );
    }

    #[test]
    fn predicate_for_shortcut_slot_out_of_range_returns_none() {
        assert_eq!(predicate_for_shortcut_slot(0), None);
        assert_eq!(predicate_for_shortcut_slot(9), None);
        assert_eq!(predicate_for_shortcut_slot(255), None);
    }

    #[test]
    fn build_sidebar_rows_assembles_all_four_sections() {
        let counts = empty_counts();
        let cats = vec!["Linux".to_owned()];
        let tags = vec!["hd".to_owned()];
        let set = build_sidebar_rows(&counts, &cats, &tags, &SidebarPredicate::All);
        assert_eq!(set.library.len(), 8);
        assert_eq!(set.categories.len(), 1);
        assert_eq!(set.tags.len(), 1);
        assert_eq!(set.trackers.len(), 3);
    }
}
