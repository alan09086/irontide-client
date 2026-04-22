//! Sidebar information architecture (M173 Lane A).
//!
//! Implements the four sidebar sections defined in the IronTide GUI design
//! spec §3:
//!
//! - **Library** — eight built-in filters (`All`, `Downloading`, `Seeding`,
//!   `Completed`, `Paused`, `Active`, `Inactive`, `Errored`).
//! - **Categories** — read from M170 `CategoryRegistry` (single-valued per
//!   torrent).
//! - **Tags** — read from M171 `TagRegistry` (multi-valued per torrent).
//! - **Trackers** — auto-aggregated from each torrent's live `TrackerInfo`
//!   list, grouped by `TrackerStatus` (Working / Unreachable / Error). The
//!   `Unreachable` bucket maps onto `TrackerStatus::NotContacted` since the
//!   M171 `TrackerStatus` enum has only three variants.
//!
//! ## Predicate model
//!
//! Every selectable sidebar row maps to a [`SidebarPredicate`]. A predicate
//! is a pure function over a [`RowView`] — a GUI-side richer view of a
//! torrent that bundles the [`TorrentSummary`] fields the model already
//! ships with the additional fields the predicate needs (`error`, `category`,
//! `tags`, current tracker hostnames). Building `RowView` GUI-side keeps the
//! `irontide-session` crate untouched (Lane A purity), and lets the predicate
//! layer be unit-tested without spinning up a session.
//!
//! Predicates compose via [`SidebarPredicate::and`] so the user can stack
//! filters (e.g. `Library::Downloading AND Category::Linux`). The torrent
//! list rebuilds on every predicate change — sort-after-filter is owned by
//! the caller (the existing `poll::sort_summaries`), not by Slint's
//! `FilterModel` which would force the sort through the runtime.
//!
//! ## Lane purity
//!
//! This module lives entirely inside `irontide-gui`. It never mutates session
//! state and reaches into `irontide-session` only through the existing public
//! API (`SessionHandle::list_categories`, `SessionHandle::list_tags`,
//! `SessionHandle::tracker_list`, `SessionHandle::list_torrent_summaries`).
//!
//! The module-level `#![allow(dead_code)]` lifts in task A8 once the
//! MainWindow wires the sidebar event channel through to the model. Tests
//! exercise every public item from the start.

#![allow(dead_code)]

use std::collections::HashMap;

use irontide::session::{TorrentState, TorrentSummary, TrackerInfo, TrackerStatus};

// ── Library filters ─────────────────────────────────────────────────────────

/// One of the eight built-in Library filters.
///
/// Each variant maps onto a single boolean predicate over a [`RowView`].
/// Variants are ordered to match the design spec §3 sidebar listing so
/// that `LibraryFilter::iter()` produces the document order used by
/// `Ctrl+1..9` keybinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibraryFilter {
    /// All torrents — predicate is constant `true`.
    All,
    /// Currently downloading (state == `Downloading` or `FetchingMetadata`).
    Downloading,
    /// Actively uploading: state == `Seeding` or `Sharing`.
    Seeding,
    /// All wanted pieces verified (`Complete` or `Seeding`).
    Completed,
    /// Manually paused by the user.
    Paused,
    /// Has any in-flight transfer (download or upload rate > 0).
    Active,
    /// No transfer activity (download and upload rate both zero) AND
    /// not currently paused. Paused torrents are reported under `Paused`,
    /// not double-counted as `Inactive`.
    Inactive,
    /// Carries a non-empty `error` string from the engine.
    Errored,
}

impl LibraryFilter {
    /// Document order — used by the sidebar layout and by `Ctrl+1..N`.
    pub const ORDER: [Self; 8] = [
        Self::All,
        Self::Downloading,
        Self::Seeding,
        Self::Completed,
        Self::Paused,
        Self::Active,
        Self::Inactive,
        Self::Errored,
    ];

    /// Stable string identifier (used for config persistence + Slint props).
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Downloading => "downloading",
            Self::Seeding => "seeding",
            Self::Completed => "completed",
            Self::Paused => "paused",
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Errored => "errored",
        }
    }

    /// Inverse of [`Self::slug`].
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ORDER.iter().copied().find(|f| f.slug() == slug)
    }

    /// Apply this filter to a [`RowView`].
    #[must_use]
    pub fn matches(self, row: &RowView) -> bool {
        match self {
            Self::All => true,
            Self::Downloading => matches!(
                row.state,
                TorrentState::Downloading | TorrentState::FetchingMetadata
            ),
            Self::Seeding => matches!(row.state, TorrentState::Seeding | TorrentState::Sharing),
            Self::Completed => {
                matches!(
                    row.state,
                    TorrentState::Complete | TorrentState::Seeding | TorrentState::Sharing
                ) || row.progress >= 1.0
            }
            Self::Paused => matches!(row.state, TorrentState::Paused),
            Self::Active => row.download_rate > 0 || row.upload_rate > 0,
            Self::Inactive => {
                !matches!(row.state, TorrentState::Paused)
                    && row.download_rate == 0
                    && row.upload_rate == 0
            }
            Self::Errored => !row.error.is_empty(),
        }
    }
}

// ── Sidebar section identity ────────────────────────────────────────────────

/// A single addressable sidebar row.
///
/// [`SidebarSection`] doubles as the persistence key for the user's last
/// selection: serialising a `SidebarSection` round-trips through
/// `SidebarSection::from_token` so config files survive across upgrades.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SidebarSection {
    /// A built-in Library filter.
    Library(LibraryFilter),
    /// A user-defined category (M170 `CategoryRegistry`).
    Category(String),
    /// A user-defined tag (M171 `TagRegistry`).
    Tag(String),
    /// Auto-aggregated tracker bucket — see [`TrackerBucket`].
    Tracker(TrackerBucket),
}

/// Auto-aggregated tracker section bucket.
///
/// Maps onto [`TrackerStatus`] as follows: `Working` → `Working`,
/// `Unreachable` → `NotContacted` (no successful announce yet),
/// `Error` → `Error`. `NotContacted` is intentionally surfaced as
/// `Unreachable` in the UI because users think of "haven't heard back" as
/// unreachable, not "we haven't tried."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackerBucket {
    /// Last announce succeeded.
    Working,
    /// Tracker has not been successfully contacted yet.
    Unreachable,
    /// Last announce returned an error.
    Error,
}

impl TrackerBucket {
    /// All buckets in display order.
    pub const ORDER: [Self; 3] = [Self::Working, Self::Unreachable, Self::Error];

    /// Map a [`TrackerStatus`] into the matching bucket.
    #[must_use]
    pub fn from_status(status: TrackerStatus) -> Self {
        match status {
            TrackerStatus::Working => Self::Working,
            TrackerStatus::NotContacted => Self::Unreachable,
            TrackerStatus::Error => Self::Error,
        }
    }

    /// Stable slug for persistence + Slint props.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Unreachable => "unreachable",
            Self::Error => "error",
        }
    }

    /// Inverse of [`Self::slug`].
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ORDER.iter().copied().find(|b| b.slug() == slug)
    }
}

impl SidebarSection {
    /// Flat string token for round-tripping the user's selection through
    /// `config.toml`.
    ///
    /// Format: `library:<slug>` / `category:<name>` / `tag:<name>` /
    /// `tracker:<bucket-slug>`.
    #[must_use]
    pub fn to_token(&self) -> String {
        match self {
            Self::Library(f) => format!("library:{}", f.slug()),
            Self::Category(name) => format!("category:{name}"),
            Self::Tag(name) => format!("tag:{name}"),
            Self::Tracker(b) => format!("tracker:{}", b.slug()),
        }
    }

    /// Inverse of [`Self::to_token`]. Returns `None` for an unrecognised
    /// kind, an unknown `library` slug, or an unknown `tracker` bucket
    /// slug. `category:` and `tag:` accept any non-empty name.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        let (kind, body) = token.split_once(':')?;
        match kind {
            "library" => LibraryFilter::from_slug(body).map(Self::Library),
            "category" => {
                if body.is_empty() {
                    None
                } else {
                    Some(Self::Category(body.to_owned()))
                }
            }
            "tag" => {
                if body.is_empty() {
                    None
                } else {
                    Some(Self::Tag(body.to_owned()))
                }
            }
            "tracker" => TrackerBucket::from_slug(body).map(Self::Tracker),
            _ => None,
        }
    }
}

// ── RowView ────────────────────────────────────────────────────────────────

/// GUI-side richer view of one torrent.
///
/// Built per poll tick from a [`TorrentSummary`] plus the extra fields the
/// session crate already exposes via `TorrentStats`/`tracker_list`. We keep
/// `RowView` separate from `TorrentSummary` so the predicate layer can be
/// unit-tested without constructing a full `TorrentSummary` (cheaper test
/// fixtures, no churn in the session crate).
#[derive(Debug, Clone)]
pub struct RowView {
    /// Hex-encoded info hash — same string the model uses as the row id.
    pub info_hash: String,
    /// Engine state. Mirrors `TorrentSummary::state`.
    pub state: TorrentState,
    /// Download progress (0.0–1.0). Mirrors `TorrentSummary::progress`.
    pub progress: f64,
    /// Current download rate in bytes/sec.
    pub download_rate: u64,
    /// Current upload rate in bytes/sec.
    pub upload_rate: u64,
    /// Engine error string. Empty when no error.
    pub error: String,
    /// User-assigned category, if any.
    pub category: Option<String>,
    /// User-assigned tags. Empty when none.
    pub tags: Vec<String>,
    /// Tracker hostnames (lowercased, deduplicated) currently configured
    /// for this torrent. Used by `SidebarPredicate::Tracker` to decide
    /// whether a torrent shows up under any tracker bucket.
    pub tracker_hosts: Vec<String>,
    /// Per-tracker buckets observed on this torrent. A torrent that has at
    /// least one tracker in `TrackerBucket::Working` shows up under the
    /// Working section, etc. Multi-set membership is allowed (qBt-parity).
    pub tracker_buckets: Vec<TrackerBucket>,
}

impl RowView {
    /// Build a `RowView` from a [`TorrentSummary`] plus the extra fields
    /// the GUI fetches per tick. `error` defaults to empty / `category`
    /// to `None` / `tags` and tracker fields to empty when those data
    /// sources have not yet resolved (e.g. magnet metadata in flight).
    #[must_use]
    pub fn from_summary(summary: &TorrentSummary) -> Self {
        Self {
            info_hash: summary.info_hash.clone(),
            state: summary.state,
            progress: summary.progress,
            download_rate: summary.download_rate,
            upload_rate: summary.upload_rate,
            error: String::new(),
            category: None,
            tags: Vec::new(),
            tracker_hosts: Vec::new(),
            tracker_buckets: Vec::new(),
        }
    }

    /// Set the engine error string and return self (builder helper).
    #[must_use]
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = error.into();
        self
    }

    /// Set the category and return self.
    #[must_use]
    pub fn with_category(mut self, category: Option<String>) -> Self {
        self.category = category;
        self
    }

    /// Set the tags and return self.
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Replace the tracker derivative state with the live `TrackerInfo` list
    /// for this torrent. Hostnames are extracted via [`tracker_host`] (lower-
    /// cased), deduplicated in input order, and the unique [`TrackerBucket`]
    /// memberships are recorded.
    pub fn with_trackers(mut self, trackers: &[TrackerInfo]) -> Self {
        let mut hosts: Vec<String> = Vec::new();
        let mut buckets: Vec<TrackerBucket> = Vec::new();
        for t in trackers {
            if let Some(host) = tracker_host(&t.url)
                && !hosts.contains(&host)
            {
                hosts.push(host);
            }
            let bucket = TrackerBucket::from_status(t.status);
            if !buckets.contains(&bucket) {
                buckets.push(bucket);
            }
        }
        self.tracker_hosts = hosts;
        self.tracker_buckets = buckets;
        self
    }
}

/// Extract the hostname from a tracker URL, lowercased.
///
/// Falls back to `None` for ill-formed URLs or DHT/PeX/LSD pseudo-trackers.
/// Accepts the `udp://`, `http://`, `https://`, `ws://`, `wss://` schemes
/// the session emits. Strips the optional userinfo (`user:pass@`), the
/// optional port (`:6969`), and the path/query/fragment.
#[must_use]
pub fn tracker_host(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    if after_scheme.is_empty() {
        return None;
    }
    let after_userinfo = after_scheme.rsplit_once('@').map_or(after_scheme, |x| x.1);
    let host_with_port = after_userinfo
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_userinfo);
    if host_with_port.is_empty() {
        return None;
    }
    // IPv6 literals: `[::1]:6969` — keep brackets in the host segment.
    let host = if let Some(stripped) = host_with_port.strip_prefix('[') {
        let close = stripped.find(']')?;
        &host_with_port[..=(close + 1)]
    } else {
        host_with_port.split(':').next().unwrap_or(host_with_port)
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

// ── SidebarPredicate ───────────────────────────────────────────────────────

/// Predicate over a [`RowView`].
///
/// `SidebarPredicate` is a closed sum of the supported predicate kinds
/// (one per sidebar row plus a recursive `And` for stacking). Avoiding
/// `Box<dyn Fn>` keeps the predicate `Clone` + `PartialEq` so the GUI
/// can compare predicates cheaply (no rebuild when nothing changed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SidebarPredicate {
    /// Match every row (the default — same as `Library(All)`).
    #[default]
    All,
    /// One of the eight Library filters.
    Library(LibraryFilter),
    /// Match when `RowView::category == Some(name)`.
    Category(String),
    /// Match when the row's tag list contains `name`.
    Tag(String),
    /// Match when the row reports membership in this tracker bucket.
    Tracker(TrackerBucket),
    /// Logical AND of two predicates (right-associative; stack via repeated
    /// [`SidebarPredicate::and`]).
    And(Box<SidebarPredicate>, Box<SidebarPredicate>),
}

impl SidebarPredicate {
    /// Convenience constructor that maps a [`SidebarSection`] to the
    /// matching predicate. The empty-tag/empty-category case is permitted
    /// for parity with qBt's "uncategorised" / "untagged" rows in a future
    /// milestone — at that point the predicate becomes `Category(name)`
    /// for `name.is_empty()` already correctly testing uncategorised.
    #[must_use]
    pub fn from_section(section: &SidebarSection) -> Self {
        match section {
            SidebarSection::Library(f) => Self::Library(*f),
            SidebarSection::Category(name) => Self::Category(name.clone()),
            SidebarSection::Tag(name) => Self::Tag(name.clone()),
            SidebarSection::Tracker(b) => Self::Tracker(*b),
        }
    }

    /// Compose with another predicate via logical AND.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        Self::And(Box::new(self), Box::new(other))
    }

    /// Evaluate against a [`RowView`].
    #[must_use]
    pub fn matches(&self, row: &RowView) -> bool {
        match self {
            Self::All => true,
            Self::Library(f) => f.matches(row),
            Self::Category(name) => row.category.as_deref() == Some(name.as_str()),
            Self::Tag(name) => row.tags.iter().any(|t| t == name),
            Self::Tracker(bucket) => row.tracker_buckets.contains(bucket),
            Self::And(a, b) => a.matches(row) && b.matches(row),
        }
    }
}

// ── Section counts ─────────────────────────────────────────────────────────

/// Aggregate Library counts across a slice of [`RowView`]s.
///
/// Each filter is evaluated over the same slice. The result is keyed by
/// [`LibraryFilter`] so callers can diff against a previous tick and emit
/// `row_changed` only for sections whose count moved.
#[must_use]
pub fn library_counts(rows: &[RowView]) -> HashMap<LibraryFilter, usize> {
    let mut counts: HashMap<LibraryFilter, usize> =
        LibraryFilter::ORDER.iter().map(|f| (*f, 0)).collect();
    for row in rows {
        for f in LibraryFilter::ORDER {
            if f.matches(row)
                && let Some(count) = counts.get_mut(&f)
            {
                *count = count.saturating_add(1);
            }
        }
    }
    counts
}

/// Aggregate per-category counts across a slice of [`RowView`]s.
///
/// Torrents whose `category` is `None` are excluded — the "uncategorised"
/// row is rendered separately by the sidebar UI and is not represented as
/// a key in this map.
#[must_use]
pub fn category_counts(rows: &[RowView]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for row in rows {
        if let Some(cat) = row.category.as_ref() {
            *counts.entry(cat.clone()).or_insert(0) =
                counts.get(cat).copied().unwrap_or(0).saturating_add(1);
        }
    }
    counts
}

/// Aggregate per-tag counts across a slice of [`RowView`]s.
///
/// A torrent with multiple tags contributes one count to each tag (multi-
/// set semantics, qBt-parity).
#[must_use]
pub fn tag_counts(rows: &[RowView]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for row in rows {
        for tag in &row.tags {
            *counts.entry(tag.clone()).or_insert(0) =
                counts.get(tag).copied().unwrap_or(0).saturating_add(1);
        }
    }
    counts
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn row(state: TorrentState, progress: f64) -> RowView {
        RowView {
            info_hash: "deadbeef".into(),
            state,
            progress,
            download_rate: 0,
            upload_rate: 0,
            error: String::new(),
            category: None,
            tags: Vec::new(),
            tracker_hosts: Vec::new(),
            tracker_buckets: Vec::new(),
        }
    }

    // ── LibraryFilter::matches ──

    #[test]
    fn library_all_matches_everything() {
        for state in [
            TorrentState::Downloading,
            TorrentState::Paused,
            TorrentState::Stopped,
            TorrentState::Seeding,
        ] {
            assert!(LibraryFilter::All.matches(&row(state, 0.0)));
        }
    }

    #[test]
    fn library_downloading_matches_downloading_and_metadata() {
        assert!(LibraryFilter::Downloading.matches(&row(TorrentState::Downloading, 0.5)));
        assert!(LibraryFilter::Downloading.matches(&row(TorrentState::FetchingMetadata, 0.0)));
        assert!(!LibraryFilter::Downloading.matches(&row(TorrentState::Seeding, 1.0)));
    }

    #[test]
    fn library_seeding_matches_seeding_and_sharing() {
        assert!(LibraryFilter::Seeding.matches(&row(TorrentState::Seeding, 1.0)));
        assert!(LibraryFilter::Seeding.matches(&row(TorrentState::Sharing, 0.7)));
        assert!(!LibraryFilter::Seeding.matches(&row(TorrentState::Downloading, 0.5)));
    }

    #[test]
    fn library_completed_matches_complete_seeding_or_progress_one() {
        assert!(LibraryFilter::Completed.matches(&row(TorrentState::Complete, 1.0)));
        assert!(LibraryFilter::Completed.matches(&row(TorrentState::Seeding, 1.0)));
        // Edge: progress is 1.0 but state hasn't transitioned yet.
        assert!(LibraryFilter::Completed.matches(&row(TorrentState::Downloading, 1.0)));
        assert!(!LibraryFilter::Completed.matches(&row(TorrentState::Downloading, 0.5)));
    }

    #[test]
    fn library_paused_matches_paused_only() {
        assert!(LibraryFilter::Paused.matches(&row(TorrentState::Paused, 0.5)));
        assert!(!LibraryFilter::Paused.matches(&row(TorrentState::Downloading, 0.5)));
    }

    #[test]
    fn library_active_matches_when_either_rate_positive() {
        let mut r = row(TorrentState::Downloading, 0.5);
        r.download_rate = 1024;
        assert!(LibraryFilter::Active.matches(&r));
        r.download_rate = 0;
        r.upload_rate = 512;
        assert!(LibraryFilter::Active.matches(&r));
        r.upload_rate = 0;
        assert!(!LibraryFilter::Active.matches(&r));
    }

    #[test]
    fn library_inactive_excludes_paused() {
        let r = row(TorrentState::Downloading, 0.5); // both rates 0
        assert!(LibraryFilter::Inactive.matches(&r));
        let r_paused = row(TorrentState::Paused, 0.5);
        assert!(
            !LibraryFilter::Inactive.matches(&r_paused),
            "Paused must not count as Inactive (otherwise counted twice)"
        );
        let mut r_active = row(TorrentState::Downloading, 0.5);
        r_active.download_rate = 1;
        assert!(!LibraryFilter::Inactive.matches(&r_active));
    }

    #[test]
    fn library_errored_matches_non_empty_error() {
        let r_clean = row(TorrentState::Downloading, 0.5);
        assert!(!LibraryFilter::Errored.matches(&r_clean));
        let r_disk = row(TorrentState::Downloading, 0.5).with_error("disk full");
        assert!(LibraryFilter::Errored.matches(&r_disk));
        let r_net = row(TorrentState::Downloading, 0.5).with_error("connection refused");
        assert!(LibraryFilter::Errored.matches(&r_net));
    }

    // ── slug round-trips ──

    #[test]
    fn library_filter_slug_round_trip() {
        for f in LibraryFilter::ORDER {
            assert_eq!(LibraryFilter::from_slug(f.slug()), Some(f));
        }
        assert_eq!(LibraryFilter::from_slug("nonsense"), None);
    }

    #[test]
    fn tracker_bucket_slug_round_trip() {
        for b in TrackerBucket::ORDER {
            assert_eq!(TrackerBucket::from_slug(b.slug()), Some(b));
        }
        assert_eq!(TrackerBucket::from_slug("invalid"), None);
    }

    #[test]
    fn sidebar_section_token_round_trip() {
        let cases = [
            SidebarSection::Library(LibraryFilter::All),
            SidebarSection::Library(LibraryFilter::Errored),
            SidebarSection::Category("Linux".into()),
            SidebarSection::Tag("hd".into()),
            SidebarSection::Tracker(TrackerBucket::Working),
        ];
        for s in cases {
            let token = s.to_token();
            assert_eq!(SidebarSection::from_token(&token), Some(s));
        }
    }

    #[test]
    fn sidebar_section_from_token_rejects_garbage() {
        assert_eq!(SidebarSection::from_token("nonsense"), None);
        assert_eq!(SidebarSection::from_token("library:huh"), None);
        assert_eq!(SidebarSection::from_token("tracker:huh"), None);
        assert_eq!(SidebarSection::from_token("category:"), None);
        assert_eq!(SidebarSection::from_token("tag:"), None);
        assert_eq!(SidebarSection::from_token("library"), None);
    }

    #[test]
    fn tracker_bucket_from_status() {
        assert_eq!(
            TrackerBucket::from_status(TrackerStatus::Working),
            TrackerBucket::Working
        );
        assert_eq!(
            TrackerBucket::from_status(TrackerStatus::NotContacted),
            TrackerBucket::Unreachable
        );
        assert_eq!(
            TrackerBucket::from_status(TrackerStatus::Error),
            TrackerBucket::Error
        );
    }

    // ── tracker_host ──

    #[test]
    fn tracker_host_strips_scheme_path_userinfo_port() {
        assert_eq!(
            tracker_host("https://tracker.example.com:6969/announce"),
            Some("tracker.example.com".into())
        );
        assert_eq!(
            tracker_host("udp://user:pass@TRACKER.example.org:80"),
            Some("tracker.example.org".into())
        );
        assert_eq!(
            tracker_host("http://[::1]:6969/announce"),
            Some("[::1]".into())
        );
    }

    #[test]
    fn tracker_host_returns_none_on_garbage() {
        assert_eq!(tracker_host(""), None);
        assert_eq!(tracker_host("://"), None);
    }

    // ── SidebarPredicate composition ──

    #[test]
    fn predicate_default_is_all() {
        assert_eq!(SidebarPredicate::default(), SidebarPredicate::All);
        assert!(SidebarPredicate::default().matches(&row(TorrentState::Paused, 0.0)));
    }

    #[test]
    fn predicate_and_combines_two_filters() {
        let mut r = row(TorrentState::Downloading, 0.5);
        r.download_rate = 100;
        r.category = Some("Linux".into());
        let pred =
            SidebarPredicate::Library(LibraryFilter::Downloading).and(SidebarPredicate::Category("Linux".into()));
        assert!(pred.matches(&r));

        // Wrong category breaks the AND.
        r.category = Some("Music".into());
        assert!(!pred.matches(&r));

        // Wrong state breaks the AND.
        r.category = Some("Linux".into());
        r.state = TorrentState::Seeding;
        assert!(!pred.matches(&r));
    }

    #[test]
    fn predicate_tag_matches_any_member() {
        let mut r = row(TorrentState::Downloading, 0.0);
        r.tags = vec!["hd".into(), "1080p".into()];
        assert!(SidebarPredicate::Tag("hd".into()).matches(&r));
        assert!(SidebarPredicate::Tag("1080p".into()).matches(&r));
        assert!(!SidebarPredicate::Tag("4k".into()).matches(&r));
    }

    #[test]
    fn predicate_category_matches_exact_string() {
        let mut r = row(TorrentState::Downloading, 0.0);
        r.category = Some("Linux".into());
        assert!(SidebarPredicate::Category("Linux".into()).matches(&r));
        // Case sensitive (qBt-parity).
        assert!(!SidebarPredicate::Category("linux".into()).matches(&r));
        // Uncategorised row never matches a named category.
        let r_uncat = row(TorrentState::Downloading, 0.0);
        assert!(!SidebarPredicate::Category("Linux".into()).matches(&r_uncat));
    }

    #[test]
    fn predicate_tracker_matches_bucket_membership() {
        let mut r = row(TorrentState::Downloading, 0.0);
        r.tracker_buckets = vec![TrackerBucket::Working, TrackerBucket::Error];
        assert!(SidebarPredicate::Tracker(TrackerBucket::Working).matches(&r));
        assert!(SidebarPredicate::Tracker(TrackerBucket::Error).matches(&r));
        assert!(!SidebarPredicate::Tracker(TrackerBucket::Unreachable).matches(&r));
    }

    #[test]
    fn predicate_from_section_round_trip() {
        let s = SidebarSection::Category("Linux".into());
        let p = SidebarPredicate::from_section(&s);
        assert_eq!(p, SidebarPredicate::Category("Linux".into()));
    }

    // ── Aggregate counts ──

    #[test]
    fn library_counts_zero_on_empty() {
        let c = library_counts(&[]);
        for f in LibraryFilter::ORDER {
            assert_eq!(c.get(&f).copied().unwrap_or(0), 0);
        }
    }

    #[test]
    fn library_counts_classifies_one_of_each() {
        let mut downloading = row(TorrentState::Downloading, 0.5);
        downloading.download_rate = 1024;
        let seeding = row(TorrentState::Seeding, 1.0);
        let paused = row(TorrentState::Paused, 0.4);
        let errored = row(TorrentState::Downloading, 0.5).with_error("oh no");

        let rows = vec![downloading.clone(), seeding.clone(), paused.clone(), errored.clone()];
        let c = library_counts(&rows);
        assert_eq!(c[&LibraryFilter::All], 4);
        // Downloading: the active downloader + the errored one.
        assert_eq!(c[&LibraryFilter::Downloading], 2);
        // Seeding + Completed both include the seeding row.
        assert_eq!(c[&LibraryFilter::Seeding], 1);
        assert_eq!(c[&LibraryFilter::Completed], 1);
        assert_eq!(c[&LibraryFilter::Paused], 1);
        // Active: only the downloader has a non-zero rate.
        assert_eq!(c[&LibraryFilter::Active], 1);
        // Inactive: seeding (rates 0) + errored (rates 0). Paused doesn't count.
        assert_eq!(c[&LibraryFilter::Inactive], 2);
        assert_eq!(c[&LibraryFilter::Errored], 1);
    }

    #[test]
    fn category_counts_excludes_uncategorised() {
        let mut r1 = row(TorrentState::Downloading, 0.0);
        r1.category = Some("Linux".into());
        let mut r2 = row(TorrentState::Downloading, 0.0);
        r2.category = Some("Linux".into());
        let mut r3 = row(TorrentState::Downloading, 0.0);
        r3.category = Some("Music".into());
        let r4 = row(TorrentState::Downloading, 0.0); // uncategorised
        let rows = vec![r1, r2, r3, r4];
        let c = category_counts(&rows);
        assert_eq!(c.get("Linux").copied(), Some(2));
        assert_eq!(c.get("Music").copied(), Some(1));
        assert_eq!(c.get(""), None);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn tag_counts_multi_set_semantics() {
        let mut r1 = row(TorrentState::Downloading, 0.0);
        r1.tags = vec!["hd".into(), "1080p".into()];
        let mut r2 = row(TorrentState::Downloading, 0.0);
        r2.tags = vec!["hd".into()];
        let r3 = row(TorrentState::Downloading, 0.0); // untagged
        let rows = vec![r1, r2, r3];
        let c = tag_counts(&rows);
        assert_eq!(c["hd"], 2);
        assert_eq!(c["1080p"], 1);
        assert_eq!(c.len(), 2);
    }

    // ── RowView::with_trackers ──

    fn dummy_tracker(url: &str, status: TrackerStatus) -> TrackerInfo {
        TrackerInfo {
            url: url.into(),
            tier: 0,
            status,
            seeders: None,
            leechers: None,
            downloaded: None,
            next_announce_secs: 0,
            consecutive_failures: 0,
        }
    }

    #[test]
    fn row_with_trackers_dedupes_hosts_and_buckets() {
        let trackers = vec![
            dummy_tracker("http://tracker.a.com:80/announce", TrackerStatus::Working),
            dummy_tracker(
                "udp://tracker.A.com:6969/announce",
                TrackerStatus::NotContacted,
            ),
            dummy_tracker("http://tracker.b.org/announce", TrackerStatus::Error),
        ];
        let r = row(TorrentState::Downloading, 0.0).with_trackers(&trackers);
        // Two unique hosts: tracker.a.com (case-folded) and tracker.b.org.
        assert_eq!(r.tracker_hosts, vec!["tracker.a.com", "tracker.b.org"]);
        // Three distinct buckets present.
        assert!(r.tracker_buckets.contains(&TrackerBucket::Working));
        assert!(r.tracker_buckets.contains(&TrackerBucket::Unreachable));
        assert!(r.tracker_buckets.contains(&TrackerBucket::Error));
    }
}
