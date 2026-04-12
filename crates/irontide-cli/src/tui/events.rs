//! Keyboard input handling for the `irontide tui` dashboard.
//!
//! `handle_key` is a pure function: it mutates `AppState` for
//! display-only changes (selection cursor, modal open/close, input
//! buffer) and returns an [`Action`] enum for anything that needs
//! network I/O. The event loop in `tui::mod` executes the action
//! against the `ApiClient`.
//!
//! Splitting pure state mutation from async I/O keeps this module
//! 100% unit-testable — no tokio runtime, no HTTP mocking.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::state::{AppState, Modal};

/// An I/O action the event loop should execute on the `ApiClient`.
///
/// Every variant carries an owned `String` hash so the action can
/// outlive the borrow of `AppState` in `handle_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    /// Nothing to do — the key was handled locally (selection move,
    /// modal open, etc.) or was unrecognised.
    None,
    /// Exit the main loop cleanly.
    Quit,
    /// `POST /torrents/{hash}/pause`.
    Pause(String),
    /// `POST /torrents/{hash}/resume`.
    Resume(String),
    /// `POST /torrents/{hash}/seed_mode` with the requested flag.
    Seed(String, bool),
    /// `DELETE /torrents/{hash}` — only issued after user confirmation.
    RemoveConfirmed(String),
    /// `POST /torrents` with a magnet URI body.
    AddMagnet(String),
    /// Kick an immediate refresh tick (F5).
    RefreshNow,
}

/// Dispatch a single key event against the current app state.
///
/// Branches on whether a modal is active so the keybind table is
/// different in each context:
///
/// | context        | handled keys |
/// |----------------|--------------|
/// | no modal       | nav, action keybinds (`s`/`p`/`r`/`d`/`a`/`q`/`?`/F5) |
/// | AddMagnet      | character input, Enter (submit), Esc (cancel), Backspace |
/// | ConfirmDelete  | `y`/`n`/Esc |
/// | Help           | any key → close |
#[must_use]
pub(crate) fn handle_key(ev: KeyEvent, state: &mut AppState) -> Action {
    // Global: Ctrl-C always quits regardless of modal state.
    if ev.code == KeyCode::Char('c') && ev.modifiers.contains(KeyModifiers::CONTROL) {
        return Action::Quit;
    }

    // Dispatch modal-specific handling first. The handlers return a
    // `ModalOutcome` describing what to do after the key is applied;
    // this keeps the per-arm borrows of `state.modal` short-lived
    // (they drop before we re-enter `state.modal` to close it).
    if state.modal.is_some() {
        let outcome = match state.modal.as_mut() {
            Some(Modal::AddMagnet { input }) => handle_add_modal_input(ev, input),
            Some(Modal::ConfirmDelete { hash, .. }) => {
                let hash = hash.clone();
                handle_confirm_modal_input(ev, &hash)
            }
            Some(Modal::Help) => handle_help_modal_input(ev),
            None => ModalOutcome::noop(),
        };
        if outcome.close {
            state.modal = None;
        }
        return outcome.action;
    }

    handle_no_modal(ev, state)
}

/// Return value from a modal-specific key handler.
struct ModalOutcome {
    /// I/O action to execute (possibly `Action::None`).
    action: Action,
    /// Whether the modal should be closed after handling.
    close: bool,
}

impl ModalOutcome {
    const fn noop() -> Self {
        Self {
            action: Action::None,
            close: false,
        }
    }
}

/// Keybinds active when no modal is open — dashboard navigation plus
/// per-torrent actions.
fn handle_no_modal(ev: KeyEvent, state: &mut AppState) -> Action {
    match ev.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('?') => {
            state.modal = Some(Modal::Help);
            Action::None
        }
        KeyCode::Char('a') => {
            state.modal = Some(Modal::AddMagnet {
                input: String::new(),
            });
            Action::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.move_selection(-1);
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.move_selection(1);
            Action::None
        }
        KeyCode::Enter => {
            state.toggle_expand();
            Action::None
        }
        KeyCode::F(5) => Action::RefreshNow,
        // Actions requiring a selection — no-op on an empty list.
        KeyCode::Char('p') => state
            .selected_hash()
            .map_or(Action::None, |h| Action::Pause(h.to_owned())),
        KeyCode::Char('r') => state
            .selected_hash()
            .map_or(Action::None, |h| Action::Resume(h.to_owned())),
        KeyCode::Char('s') => {
            let Some(hash) = state.selected_hash().map(ToOwned::to_owned) else {
                return Action::None;
            };
            // Toggle based on the cached stats.user_seed_mode flag.
            // If we don't have cached detail, assume "currently off"
            // and enable — the first press after selection flips it
            // on, second press flips it off on a subsequent tick once
            // the cache repopulates.
            let currently_on = state
                .detail_cache
                .get(&hash)
                .is_some_and(|d| d.stats.user_seed_mode);
            Action::Seed(hash, !currently_on)
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            let Some(summary) = state.torrents.get(state.selected) else {
                return Action::None;
            };
            state.modal = Some(Modal::ConfirmDelete {
                hash: summary.info_hash.clone(),
                name: summary.name.clone(),
            });
            Action::None
        }
        _ => Action::None,
    }
}

/// Keybinds active in the `AddMagnet` modal.
fn handle_add_modal_input(ev: KeyEvent, input: &mut String) -> ModalOutcome {
    match ev.code {
        KeyCode::Esc => ModalOutcome {
            action: Action::None,
            close: true,
        },
        KeyCode::Enter => {
            let uri = input.trim().to_owned();
            let action = if uri.is_empty() {
                Action::None
            } else {
                Action::AddMagnet(uri)
            };
            ModalOutcome {
                action,
                close: true,
            }
        }
        KeyCode::Backspace => {
            input.pop();
            ModalOutcome::noop()
        }
        KeyCode::Char(c) => {
            // Ignore Ctrl-modified keys so an accidental Ctrl-A
            // doesn't insert a garbage glyph.
            if ev.modifiers.contains(KeyModifiers::CONTROL) {
                return ModalOutcome::noop();
            }
            input.push(c);
            ModalOutcome::noop()
        }
        _ => ModalOutcome::noop(),
    }
}

/// Keybinds active in the `ConfirmDelete` modal.
fn handle_confirm_modal_input(ev: KeyEvent, hash: &str) -> ModalOutcome {
    match ev.code {
        KeyCode::Char('y' | 'Y') => ModalOutcome {
            action: Action::RemoveConfirmed(hash.to_owned()),
            close: true,
        },
        KeyCode::Char('n' | 'N') | KeyCode::Esc => ModalOutcome {
            action: Action::None,
            close: true,
        },
        _ => ModalOutcome::noop(),
    }
}

/// Keybinds active in the `Help` modal — any key closes it.
fn handle_help_modal_input(_ev: KeyEvent) -> ModalOutcome {
    ModalOutcome {
        action: Action::None,
        close: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::TorrentSummaryDto;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn make_torrent(hash: &str, name: &str) -> TorrentSummaryDto {
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

    fn state_with_one_torrent(hash: &str) -> AppState {
        let mut s = AppState::new();
        s.torrents = vec![make_torrent(hash, "test")];
        s
    }

    #[test]
    fn test_q_key_quits() {
        let mut s = AppState::new();
        assert_eq!(handle_key(key(KeyCode::Char('q')), &mut s), Action::Quit);
    }

    #[test]
    fn test_esc_quits_without_modal() {
        let mut s = AppState::new();
        assert_eq!(handle_key(key(KeyCode::Esc), &mut s), Action::Quit);
    }

    #[test]
    fn test_ctrl_c_quits_regardless_of_modal() {
        let mut s = AppState::new();
        s.modal = Some(Modal::AddMagnet {
            input: "partial".to_owned(),
        });
        assert_eq!(handle_key(ctrl(KeyCode::Char('c')), &mut s), Action::Quit);
    }

    #[test]
    fn test_arrow_down_moves_selection() {
        let mut s = AppState::new();
        s.torrents = vec![make_torrent("a", "A"), make_torrent("b", "B")];
        assert_eq!(handle_key(key(KeyCode::Down), &mut s), Action::None);
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn test_vim_j_moves_selection_down() {
        let mut s = AppState::new();
        s.torrents = vec![make_torrent("a", "A"), make_torrent("b", "B")];
        assert_eq!(handle_key(key(KeyCode::Char('j')), &mut s), Action::None);
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn test_p_pauses_selected() {
        let mut s = state_with_one_torrent("deadbeef");
        assert_eq!(
            handle_key(key(KeyCode::Char('p')), &mut s),
            Action::Pause("deadbeef".to_owned())
        );
    }

    #[test]
    fn test_r_resumes_selected() {
        let mut s = state_with_one_torrent("deadbeef");
        assert_eq!(
            handle_key(key(KeyCode::Char('r')), &mut s),
            Action::Resume("deadbeef".to_owned())
        );
    }

    #[test]
    fn test_s_toggles_seed_mode_enables_when_off() {
        let mut s = state_with_one_torrent("deadbeef");
        // No cached detail → assume off → enable.
        assert_eq!(
            handle_key(key(KeyCode::Char('s')), &mut s),
            Action::Seed("deadbeef".to_owned(), true)
        );
    }

    #[test]
    fn test_a_opens_add_modal() {
        let mut s = AppState::new();
        assert_eq!(handle_key(key(KeyCode::Char('a')), &mut s), Action::None);
        assert!(matches!(s.modal, Some(Modal::AddMagnet { .. })));
    }

    #[test]
    fn test_question_opens_help_modal() {
        let mut s = AppState::new();
        assert_eq!(handle_key(key(KeyCode::Char('?')), &mut s), Action::None);
        assert!(matches!(s.modal, Some(Modal::Help)));
    }

    #[test]
    fn test_d_opens_confirm_delete() {
        let mut s = state_with_one_torrent("deadbeef");
        assert_eq!(handle_key(key(KeyCode::Char('d')), &mut s), Action::None);
        match s.modal {
            Some(Modal::ConfirmDelete { hash, name }) => {
                assert_eq!(hash, "deadbeef");
                assert_eq!(name, "test");
            }
            other => panic!("expected ConfirmDelete, got {other:?}"),
        }
    }

    #[test]
    fn test_enter_submits_add_modal() {
        let mut s = AppState::new();
        s.modal = Some(Modal::AddMagnet {
            input: "magnet:?xt=urn:btih:abcd".to_owned(),
        });
        assert_eq!(
            handle_key(key(KeyCode::Enter), &mut s),
            Action::AddMagnet("magnet:?xt=urn:btih:abcd".to_owned())
        );
        assert!(s.modal.is_none(), "modal should close after submit");
    }

    #[test]
    fn test_add_modal_types_characters_into_buffer() {
        let mut s = AppState::new();
        s.modal = Some(Modal::AddMagnet {
            input: String::new(),
        });
        let _ = handle_key(key(KeyCode::Char('m')), &mut s);
        let _ = handle_key(key(KeyCode::Char('a')), &mut s);
        let _ = handle_key(key(KeyCode::Char('g')), &mut s);
        match s.modal {
            Some(Modal::AddMagnet { ref input }) => assert_eq!(input, "mag"),
            _ => panic!("modal should still be open with typed chars"),
        }
    }

    #[test]
    fn test_add_modal_backspace_erases() {
        let mut s = AppState::new();
        s.modal = Some(Modal::AddMagnet {
            input: "mag".to_owned(),
        });
        let _ = handle_key(key(KeyCode::Backspace), &mut s);
        match s.modal {
            Some(Modal::AddMagnet { ref input }) => assert_eq!(input, "ma"),
            _ => panic!("modal should still be open after backspace"),
        }
    }

    #[test]
    fn test_add_modal_empty_enter_is_noop() {
        let mut s = AppState::new();
        s.modal = Some(Modal::AddMagnet {
            input: "   ".to_owned(),
        });
        assert_eq!(handle_key(key(KeyCode::Enter), &mut s), Action::None);
        assert!(s.modal.is_none(), "modal should still close on empty enter");
    }

    #[test]
    fn test_esc_cancels_add_modal() {
        let mut s = AppState::new();
        s.modal = Some(Modal::AddMagnet {
            input: "mag".to_owned(),
        });
        assert_eq!(handle_key(key(KeyCode::Esc), &mut s), Action::None);
        assert!(s.modal.is_none());
    }

    #[test]
    fn test_confirm_delete_y_removes() {
        let mut s = AppState::new();
        s.modal = Some(Modal::ConfirmDelete {
            hash: "deadbeef".to_owned(),
            name: "test".to_owned(),
        });
        assert_eq!(
            handle_key(key(KeyCode::Char('y')), &mut s),
            Action::RemoveConfirmed("deadbeef".to_owned())
        );
        assert!(s.modal.is_none());
    }

    #[test]
    fn test_confirm_delete_n_cancels() {
        let mut s = AppState::new();
        s.modal = Some(Modal::ConfirmDelete {
            hash: "deadbeef".to_owned(),
            name: "test".to_owned(),
        });
        assert_eq!(handle_key(key(KeyCode::Char('n')), &mut s), Action::None);
        assert!(s.modal.is_none());
    }

    #[test]
    fn test_help_modal_any_key_closes() {
        let mut s = AppState::new();
        s.modal = Some(Modal::Help);
        assert_eq!(handle_key(key(KeyCode::Char('x')), &mut s), Action::None);
        assert!(s.modal.is_none());
    }

    #[test]
    fn test_no_key_action_without_selection() {
        let mut s = AppState::new();
        // Empty torrent list: p/r/s/d should all be no-ops.
        assert_eq!(handle_key(key(KeyCode::Char('p')), &mut s), Action::None);
        assert_eq!(handle_key(key(KeyCode::Char('r')), &mut s), Action::None);
        assert_eq!(handle_key(key(KeyCode::Char('s')), &mut s), Action::None);
        assert_eq!(handle_key(key(KeyCode::Char('d')), &mut s), Action::None);
        assert!(s.modal.is_none(), "d should not open confirm on empty list");
    }
}
