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
}

impl AppState {
    pub fn new(shutdown_tx: tokio::sync::oneshot::Sender<()>) -> Self {
        Self {
            phase: AppPhase::Loading,
            shutdown_tx: Some(shutdown_tx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_phase_default_is_loading() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let state = AppState::new(tx);
        assert_eq!(state.phase, AppPhase::Loading);
    }

    #[test]
    fn app_phase_transitions() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let mut state = AppState::new(tx);
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
}
