use std::collections::HashSet;

/// Application lifecycle phases.
#[derive(Debug, Clone, PartialEq)]
pub enum AppPhase {
    Loading,
    Ready,
    Error(String),
}

/// Menu actions from the File menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuAction {
    AddMagnet,
    AddTorrentFile,
    Quit,
}

impl MenuAction {
    /// Parse a menu callback index into a `MenuAction`.
    /// Returns `None` for out-of-bounds indices.
    pub fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::AddMagnet),
            1 => Some(Self::AddTorrentFile),
            2 => Some(Self::Quit),
            _ => None,
        }
    }
}

/// Top-level application state.
pub struct AppState {
    pub phase: AppPhase,
    pub shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    pub sort: crate::columns::SortState,
    pub selected: HashSet<String>,
    pub last_clicked: Option<String>,
    pub current_order: Vec<String>,
    pub columns: crate::columns::ColumnConfig,
    pub columns_dirty: bool,
}

impl AppState {
    pub fn new(shutdown_tx: tokio::sync::oneshot::Sender<()>, columns: crate::columns::ColumnConfig) -> Self {
        Self {
            phase: AppPhase::Loading,
            shutdown_tx: Some(shutdown_tx),
            sort: crate::columns::SortState::default(),
            selected: HashSet::new(),
            last_clicked: None,
            current_order: Vec::new(),
            columns,
            columns_dirty: false,
        }
    }

    /// Single-click: clear all selection, select only this hash.
    pub fn selection_click(&mut self, info_hash: &str) {
        self.selected.clear();
        self.selected.insert(info_hash.to_owned());
        self.last_clicked = Some(info_hash.to_owned());
    }

    /// Ctrl+click: toggle selection of this hash without clearing others.
    pub fn selection_ctrl_click(&mut self, info_hash: &str) {
        if self.selected.contains(info_hash) {
            self.selected.remove(info_hash);
        } else {
            self.selected.insert(info_hash.to_owned());
        }
        self.last_clicked = Some(info_hash.to_owned());
    }

    /// Shift+click: select range from last_clicked to this hash.
    /// Uses `current_order` to determine the range.
    pub fn selection_shift_click(&mut self, info_hash: &str) {
        let Some(anchor) = self.last_clicked.as_ref() else {
            // No anchor — treat as single click.
            self.selection_click(info_hash);
            return;
        };
        let anchor_pos = self.current_order.iter().position(|h| h == anchor);
        let target_pos = self.current_order.iter().position(|h| h == info_hash);
        match (anchor_pos, target_pos) {
            (Some(a), Some(t)) => {
                let (start, end) = if a <= t { (a, t) } else { (t, a) };
                self.selected.clear();
                for h in &self.current_order[start..=end] {
                    self.selected.insert(h.clone());
                }
            }
            _ => {
                // Can't find either in order — treat as single click.
                self.selection_click(info_hash);
            }
        }
        // Don't update last_clicked on shift-click (anchor stays)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_phase_default_is_loading() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let state = AppState::new(tx, crate::columns::ColumnConfig::default());
        assert_eq!(state.phase, AppPhase::Loading);
    }

    #[test]
    fn app_phase_transitions() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(tx, crate::columns::ColumnConfig::default());
        assert_eq!(state.phase, AppPhase::Loading);

        state.phase = AppPhase::Ready;
        assert_eq!(state.phase, AppPhase::Ready);

        state.phase = AppPhase::Error("test error".to_string());
        assert_eq!(state.phase, AppPhase::Error("test error".to_string()));
    }

    #[test]
    fn menu_action_from_index() {
        assert_eq!(MenuAction::from_index(0), Some(MenuAction::AddMagnet));
        assert_eq!(MenuAction::from_index(1), Some(MenuAction::AddTorrentFile));
        assert_eq!(MenuAction::from_index(2), Some(MenuAction::Quit));
    }

    #[test]
    fn menu_action_out_of_bounds() {
        assert_eq!(MenuAction::from_index(-1), None);
        assert_eq!(MenuAction::from_index(3), None);
        assert_eq!(MenuAction::from_index(100), None);
    }

    #[test]
    fn test_selection_single_click() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(tx, crate::columns::ColumnConfig::default());
        state.selection_click("abc123");
        assert!(state.selected.contains("abc123"));
        assert_eq!(state.selected.len(), 1);
        // Second click clears first
        state.selection_click("def456");
        assert!(!state.selected.contains("abc123"));
        assert!(state.selected.contains("def456"));
        assert_eq!(state.selected.len(), 1);
    }

    #[test]
    fn test_selection_ctrl_toggle() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(tx, crate::columns::ColumnConfig::default());
        state.selection_ctrl_click("abc123");
        assert!(state.selected.contains("abc123"));
        state.selection_ctrl_click("def456");
        assert_eq!(state.selected.len(), 2);
        // Toggle off
        state.selection_ctrl_click("abc123");
        assert!(!state.selected.contains("abc123"));
        assert_eq!(state.selected.len(), 1);
    }

    #[test]
    fn test_selection_shift_range() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(tx, crate::columns::ColumnConfig::default());
        state.current_order = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
        // Click "b" first (sets anchor)
        state.selection_click("b");
        // Shift-click "d" — should select b, c, d
        state.selection_shift_click("d");
        assert_eq!(state.selected.len(), 3);
        assert!(state.selected.contains("b"));
        assert!(state.selected.contains("c"));
        assert!(state.selected.contains("d"));
    }
}
