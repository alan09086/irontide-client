//! Command palette registry, fuzzy matching, and dispatch (M183).

use slint::SharedString;

use crate::accel;
use crate::sidebar::LibraryFilter;

// ── Command identity ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PaletteCommandId {
    // Action
    AddMagnetLink,
    AddTorrentFile,
    PauseSelected,
    ResumeSelected,
    RemoveSelected,
    ForceRecheck,
    ForceReannounce,
    PauseAll,
    ResumeAll,
    // Navigation
    NavAll,
    NavDownloading,
    NavSeeding,
    NavCompleted,
    NavPaused,
    NavActive,
    NavInactive,
    NavErrored,
    // Tools
    OpenPreferences,
    SelectAll,
    // Settings
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteCategory {
    Action,
    Navigation,
    Tools,
    Settings,
}

impl PaletteCategory {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Action => "ACTION",
            Self::Navigation => "NAVIGATION",
            Self::Tools => "TOOLS",
            Self::Settings => "SETTINGS",
        }
    }
}

// ── Command definition ───────────────────────────────────────────────────────

pub struct PaletteCommand {
    pub id: PaletteCommandId,
    pub label: &'static str,
    pub category: PaletteCategory,
    pub hotkey_hint: &'static str,
}

pub static COMMANDS: &[PaletteCommand] = &[
    // Action
    PaletteCommand {
        id: PaletteCommandId::AddMagnetLink,
        label: "Add Magnet Link",
        category: PaletteCategory::Action,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::AddTorrentFile,
        label: "Add Torrent File",
        category: PaletteCategory::Action,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::PauseSelected,
        label: "Pause Selected",
        category: PaletteCategory::Action,
        hotkey_hint: "Space",
    },
    PaletteCommand {
        id: PaletteCommandId::ResumeSelected,
        label: "Resume Selected",
        category: PaletteCategory::Action,
        hotkey_hint: "Space",
    },
    PaletteCommand {
        id: PaletteCommandId::RemoveSelected,
        label: "Remove Selected",
        category: PaletteCategory::Action,
        hotkey_hint: "Del",
    },
    PaletteCommand {
        id: PaletteCommandId::ForceRecheck,
        label: "Force Recheck",
        category: PaletteCategory::Action,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::ForceReannounce,
        label: "Force Reannounce",
        category: PaletteCategory::Action,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::PauseAll,
        label: "Pause All",
        category: PaletteCategory::Action,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::ResumeAll,
        label: "Resume All",
        category: PaletteCategory::Action,
        hotkey_hint: "",
    },
    // Navigation
    PaletteCommand {
        id: PaletteCommandId::NavAll,
        label: "All Torrents",
        category: PaletteCategory::Navigation,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::NavDownloading,
        label: "Downloading",
        category: PaletteCategory::Navigation,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::NavSeeding,
        label: "Seeding",
        category: PaletteCategory::Navigation,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::NavCompleted,
        label: "Completed",
        category: PaletteCategory::Navigation,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::NavPaused,
        label: "Paused",
        category: PaletteCategory::Navigation,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::NavActive,
        label: "Active",
        category: PaletteCategory::Navigation,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::NavInactive,
        label: "Inactive",
        category: PaletteCategory::Navigation,
        hotkey_hint: "",
    },
    PaletteCommand {
        id: PaletteCommandId::NavErrored,
        label: "Errored",
        category: PaletteCategory::Navigation,
        hotkey_hint: "",
    },
    // Tools
    PaletteCommand {
        id: PaletteCommandId::SelectAll,
        label: "Select All",
        category: PaletteCategory::Tools,
        hotkey_hint: "",
    },
    // Settings
    PaletteCommand {
        id: PaletteCommandId::OpenPreferences,
        label: "Preferences (Tweaks)",
        category: PaletteCategory::Settings,
        hotkey_hint: ",",
    },
    PaletteCommand {
        id: PaletteCommandId::Quit,
        label: "Quit",
        category: PaletteCategory::Settings,
        hotkey_hint: "",
    },
];

// ── Fuzzy matching ───────────────────────────────────────────────────────────

#[must_use]
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let query_lower: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let cand_lower: Vec<char> = candidate.chars().flat_map(char::to_lowercase).collect();

    let mut qi = 0;
    let mut score: i32 = 0;
    let mut prev_match_idx: Option<usize> = None;

    for (ci, &ch) in cand_lower.iter().enumerate() {
        if qi < query_lower.len() && ch == query_lower[qi] {
            score += 1;
            // Prefix bonus
            if ci == qi {
                score += 3;
            }
            // Consecutive bonus
            if ci > 0 && prev_match_idx == Some(ci - 1) {
                score += 5;
            }
            // Word-boundary bonus
            if ci == 0 || matches!(cand_lower.get(ci - 1), Some(' ' | '_' | '-')) {
                score += 4;
            }
            prev_match_idx = Some(ci);
            qi += 1;
        }
    }

    if qi == query_lower.len() {
        Some(score)
    } else {
        None
    }
}

/// Filter commands by query. Returns `(flat_index, &PaletteCommand)` pairs
/// sorted by score descending.
#[must_use]
pub fn filter_commands(query: &str) -> Vec<(usize, &'static PaletteCommand)> {
    if query.is_empty() {
        return COMMANDS.iter().enumerate().collect();
    }
    let mut scored: Vec<(i32, usize, &PaletteCommand)> = COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(idx, cmd)| fuzzy_score(query, cmd.label).map(|s| (s, idx, cmd)))
        .collect();
    scored.sort_by_key(|&(s, _, _)| std::cmp::Reverse(s));
    scored.into_iter().map(|(_, idx, cmd)| (idx, cmd)).collect()
}

// ── Enable/disable ───────────────────────────────────────────────────────────

#[must_use]
pub fn is_enabled(id: PaletteCommandId, has_selection: bool) -> bool {
    match id {
        PaletteCommandId::PauseSelected
        | PaletteCommandId::ResumeSelected
        | PaletteCommandId::RemoveSelected
        | PaletteCommandId::ForceRecheck
        | PaletteCommandId::ForceReannounce => has_selection,
        _ => true,
    }
}

// ── Hotkey hint resolution ───────────────────────────────────────────────────

#[must_use]
pub fn resolved_hotkey(cmd: &PaletteCommand) -> SharedString {
    match cmd.id {
        PaletteCommandId::OpenPreferences => accel::format_shortcut(&[","]),
        PaletteCommandId::SelectAll => accel::format_shortcut(&["A"]),
        _ => SharedString::from(cmd.hotkey_hint),
    }
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

pub enum DispatchAction {
    ShowAddMagnet,
    ShowAddTorrent,
    SendCommand(crate::app::GuiCommand),
    SetPredicate(crate::sidebar::SidebarPredicate),
    OpenPreferences,
    SelectAll,
    Quit,
}

#[must_use]
pub fn dispatch(id: PaletteCommandId, selected: &[String]) -> DispatchAction {
    use crate::app::GuiCommand;
    use crate::sidebar::SidebarPredicate;
    match id {
        PaletteCommandId::AddMagnetLink => DispatchAction::ShowAddMagnet,
        PaletteCommandId::AddTorrentFile => DispatchAction::ShowAddTorrent,
        PaletteCommandId::PauseSelected => DispatchAction::SendCommand(GuiCommand::PauseTorrents {
            hashes: selected.to_vec(),
        }),
        PaletteCommandId::ResumeSelected => {
            DispatchAction::SendCommand(GuiCommand::ResumeTorrents {
                hashes: selected.to_vec(),
            })
        }
        PaletteCommandId::RemoveSelected => {
            DispatchAction::SendCommand(GuiCommand::RemoveTorrents {
                hashes: selected.to_vec(),
                delete_files: false,
            })
        }
        PaletteCommandId::ForceRecheck => DispatchAction::SendCommand(GuiCommand::ForceRecheck {
            hashes: selected.to_vec(),
        }),
        PaletteCommandId::ForceReannounce => {
            DispatchAction::SendCommand(GuiCommand::ForceReannounce {
                hashes: selected.to_vec(),
            })
        }
        PaletteCommandId::PauseAll => {
            DispatchAction::SendCommand(GuiCommand::PauseTorrents { hashes: Vec::new() })
        }
        PaletteCommandId::ResumeAll => {
            DispatchAction::SendCommand(GuiCommand::ResumeTorrents { hashes: Vec::new() })
        }
        PaletteCommandId::NavAll => {
            DispatchAction::SetPredicate(SidebarPredicate::Library(LibraryFilter::All))
        }
        PaletteCommandId::NavDownloading => {
            DispatchAction::SetPredicate(SidebarPredicate::Library(LibraryFilter::Downloading))
        }
        PaletteCommandId::NavSeeding => {
            DispatchAction::SetPredicate(SidebarPredicate::Library(LibraryFilter::Seeding))
        }
        PaletteCommandId::NavCompleted => {
            DispatchAction::SetPredicate(SidebarPredicate::Library(LibraryFilter::Completed))
        }
        PaletteCommandId::NavPaused => {
            DispatchAction::SetPredicate(SidebarPredicate::Library(LibraryFilter::Paused))
        }
        PaletteCommandId::NavActive => {
            DispatchAction::SetPredicate(SidebarPredicate::Library(LibraryFilter::Active))
        }
        PaletteCommandId::NavInactive => {
            DispatchAction::SetPredicate(SidebarPredicate::Library(LibraryFilter::Inactive))
        }
        PaletteCommandId::NavErrored => {
            DispatchAction::SetPredicate(SidebarPredicate::Library(LibraryFilter::Errored))
        }
        PaletteCommandId::OpenPreferences => DispatchAction::OpenPreferences,
        PaletteCommandId::SelectAll => DispatchAction::SelectAll,
        PaletteCommandId::Quit => DispatchAction::Quit,
    }
}

// ── Recent commands ──────────────────────────────────────────────────────────

const MAX_RECENT: usize = 5;

pub fn record_recent(recent: &mut Vec<PaletteCommandId>, id: PaletteCommandId) {
    recent.retain(|&x| x != id);
    recent.insert(0, id);
    recent.truncate(MAX_RECENT);
}

// ── Slint model building ─────────────────────────────────────────────────────

use slint::Model as _;

use crate::{PaletteCategory as SlintPaletteCategory, PaletteItem as SlintPaletteItem};

pub fn build_slint_categories(
    results: &[(usize, &'static PaletteCommand)],
    has_selection: bool,
    recent: &[PaletteCommandId],
    query_empty: bool,
) -> slint::ModelRc<SlintPaletteCategory> {
    let mut categories: Vec<SlintPaletteCategory> = Vec::new();

    // Recent pseudo-category when query is empty and recents exist
    if query_empty && !recent.is_empty() {
        let items: Vec<SlintPaletteItem> = recent
            .iter()
            .filter_map(|&id| {
                COMMANDS
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.id == id)
                    .map(|(idx, cmd)| SlintPaletteItem {
                        label: SharedString::from(cmd.label),
                        hotkey: resolved_hotkey(cmd),
                        enabled: is_enabled(id, has_selection),
                        index: i32::try_from(idx).unwrap_or(0),
                    })
            })
            .collect();
        if !items.is_empty() {
            categories.push(SlintPaletteCategory {
                name: SharedString::from("RECENT"),
                items: slint::ModelRc::new(slint::VecModel::from(items)),
            });
        }
    }

    // Group results by category (preserving order within each group)
    let cat_order = [
        PaletteCategory::Action,
        PaletteCategory::Navigation,
        PaletteCategory::Tools,
        PaletteCategory::Settings,
    ];

    for cat in cat_order {
        let items: Vec<SlintPaletteItem> = results
            .iter()
            .filter(|(_, cmd)| cmd.category == cat)
            .map(|&(idx, cmd)| SlintPaletteItem {
                label: SharedString::from(cmd.label),
                hotkey: resolved_hotkey(cmd),
                enabled: is_enabled(cmd.id, has_selection),
                index: i32::try_from(idx).unwrap_or(0),
            })
            .collect();
        if !items.is_empty() {
            categories.push(SlintPaletteCategory {
                name: SharedString::from(cat.label()),
                items: slint::ModelRc::new(slint::VecModel::from(items)),
            });
        }
    }

    slint::ModelRc::new(slint::VecModel::from(categories))
}

/// Count total items across all categories (for footer display + active-index bounds).
#[must_use]
pub fn count_items(categories: &slint::ModelRc<SlintPaletteCategory>) -> i32 {
    let mut total: usize = 0;
    for i in 0..categories.row_count() {
        if let Some(cat) = categories.row_data(i) {
            total += cat.items.row_count();
        }
    }
    i32::try_from(total).unwrap_or(i32::MAX)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_score_none_for_non_subsequence() {
        assert_eq!(fuzzy_score("xyz", "Pause Selected"), None);
    }

    #[test]
    fn fuzzy_score_some_for_valid_match() {
        assert!(fuzzy_score("pause", "Pause Selected").is_some());
    }

    #[test]
    fn fuzzy_score_case_insensitive() {
        let upper = fuzzy_score("PAUSE", "Pause Selected");
        let lower = fuzzy_score("pause", "Pause Selected");
        assert_eq!(upper, lower);
    }

    #[test]
    fn fuzzy_score_consecutive_beats_scattered() {
        let consecutive = fuzzy_score("add", "Add Magnet Link").unwrap();
        let scattered = fuzzy_score("aml", "Add Magnet Link").unwrap();
        assert!(consecutive > scattered);
    }

    #[test]
    fn filter_commands_empty_returns_all() {
        let results = filter_commands("");
        assert_eq!(results.len(), COMMANDS.len());
    }

    #[test]
    fn filter_commands_pause_returns_pause_first() {
        let results = filter_commands("pause");
        assert!(!results.is_empty());
        let first_label = results[0].1.label;
        assert!(
            first_label.contains("Pause"),
            "expected Pause-related command first, got {first_label}"
        );
    }

    #[test]
    fn filter_commands_xyz_returns_empty() {
        let results = filter_commands("xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn is_enabled_selection_dependent_without_selection() {
        assert!(!is_enabled(PaletteCommandId::PauseSelected, false));
        assert!(!is_enabled(PaletteCommandId::ResumeSelected, false));
        assert!(!is_enabled(PaletteCommandId::RemoveSelected, false));
        assert!(!is_enabled(PaletteCommandId::ForceRecheck, false));
        assert!(!is_enabled(PaletteCommandId::ForceReannounce, false));
    }

    #[test]
    fn is_enabled_non_selection_dependent_always_true() {
        assert!(is_enabled(PaletteCommandId::AddMagnetLink, false));
        assert!(is_enabled(PaletteCommandId::NavAll, false));
        assert!(is_enabled(PaletteCommandId::Quit, false));
        assert!(is_enabled(PaletteCommandId::OpenPreferences, false));
    }

    #[test]
    fn record_recent_inserts_at_front() {
        let mut recent = Vec::new();
        record_recent(&mut recent, PaletteCommandId::Quit);
        record_recent(&mut recent, PaletteCommandId::NavAll);
        assert_eq!(recent[0], PaletteCommandId::NavAll);
        assert_eq!(recent[1], PaletteCommandId::Quit);
    }

    #[test]
    fn record_recent_deduplicates() {
        let mut recent = vec![PaletteCommandId::NavAll, PaletteCommandId::Quit];
        record_recent(&mut recent, PaletteCommandId::Quit);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0], PaletteCommandId::Quit);
        assert_eq!(recent[1], PaletteCommandId::NavAll);
    }

    #[test]
    fn record_recent_caps_at_5() {
        let mut recent = Vec::new();
        record_recent(&mut recent, PaletteCommandId::Quit);
        record_recent(&mut recent, PaletteCommandId::NavAll);
        record_recent(&mut recent, PaletteCommandId::NavSeeding);
        record_recent(&mut recent, PaletteCommandId::NavPaused);
        record_recent(&mut recent, PaletteCommandId::NavActive);
        record_recent(&mut recent, PaletteCommandId::SelectAll);
        assert_eq!(recent.len(), 5);
        assert_eq!(recent[0], PaletteCommandId::SelectAll);
        assert!(!recent.contains(&PaletteCommandId::Quit));
    }
}
