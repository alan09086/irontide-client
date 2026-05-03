//! Cross-platform accelerator (keyboard shortcut) helper.
//!
//! Slint `.slint` files can't cfg-gate — they see one value for the current
//! platform. This module resolves the accelerator modifier (Meta on macOS,
//! Control elsewhere) at compile time and provides utilities for rendering
//! shortcut strings and matching key events.
//!
//! M173 Lane A — task A7 — consumed the previously-unused
//! [`format_shortcut`], [`AccelModifier`], [`CURRENT_ACCEL`], and
//! [`matches_accel`] helpers when wiring the sidebar `Ctrl+1..9`
//! keybinds. The sidebar tooltips render via [`format_shortcut`], the
//! Slint focus-scope handler matches the platform accelerator via
//! [`matches_accel`], and the cross-platform shortcut label
//! ([`sidebar_shortcut_label`]) reads [`CURRENT_ACCEL`] under the
//! hood. The module-level `#![allow(dead_code)]` from M172b is now
//! gone.

use slint::SharedString;

/// Keyboard modifier used as the primary accelerator on this platform.
///
/// One variant per supported platform. `CURRENT_ACCEL` resolves to
/// exactly one variant per build target, so the other variant is
/// platform-dead by design — the `dead_code` allow on the variants is
/// intentional, not a stale annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelModifier {
    /// macOS command key (⌘). Only constructed on `target_os = "macos"`.
    #[allow(dead_code)]
    Meta,
    /// Control key (Ctrl) — Linux, Windows, BSD. Only constructed on
    /// non-macOS targets.
    #[allow(dead_code)]
    Control,
}

#[cfg(target_os = "macos")]
pub const CURRENT_ACCEL: AccelModifier = AccelModifier::Meta;
#[cfg(not(target_os = "macos"))]
pub const CURRENT_ACCEL: AccelModifier = AccelModifier::Control;

/// Render a shortcut string for display (menu labels, tooltips).
///
/// macOS example: `format_shortcut(&["Shift", "T"])` → `⌘⇧T`.
/// Other OS example: `format_shortcut(&["Shift", "T"])` → `Ctrl+Shift+T`.
#[must_use]
pub fn format_shortcut(parts: &[&str]) -> SharedString {
    match CURRENT_ACCEL {
        AccelModifier::Meta => {
            let mut s = String::from("⌘");
            for part in parts {
                s.push_str(match *part {
                    "Shift" => "⇧",
                    "Ctrl" | "Control" => "⌃",
                    "Alt" | "Option" => "⌥",
                    other => other,
                });
            }
            s.into()
        }
        AccelModifier::Control => {
            let mut s = String::from("Ctrl");
            for part in parts {
                s.push('+');
                s.push_str(part);
            }
            s.into()
        }
    }
}

/// Match a key event against the platform accelerator.
///
/// `event_text` is the character produced by the event (e.g. `"T"`).
/// `held_ctrl` and `held_meta` mirror the `FocusScope` `meta-held` /
/// `ctrl-held` properties.
#[must_use]
pub fn matches_accel(event_text: &str, held_ctrl: bool, held_meta: bool) -> bool {
    match CURRENT_ACCEL {
        AccelModifier::Meta => held_meta && !event_text.is_empty(),
        AccelModifier::Control => held_ctrl && !event_text.is_empty(),
    }
}

/// Render the cross-platform sidebar-shortcut label for slot `n`.
///
/// `n` is one-indexed (1..=9). Returns `Ctrl+N` on Linux/Windows/BSD and
/// `⌘N` on macOS. `n` outside `1..=9` returns the empty string so a
/// caller iterating over the sidebar's display rows can blank the
/// hint past the ninth row.
#[must_use]
pub fn sidebar_shortcut_label(n: u8) -> SharedString {
    if !(1..=9).contains(&n) {
        return SharedString::new();
    }
    let digit = char::from_digit(u32::from(n), 10).unwrap_or('0');
    let parts = [digit.to_string()];
    let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
    format_shortcut(&refs)
}

/// Returns the platform-appropriate shortcut label for the command palette.
#[must_use]
#[allow(dead_code)]
pub fn palette_shortcut_label() -> SharedString {
    format_shortcut(&["K"])
}

/// Try to interpret a Slint key event as a sidebar `Ctrl+N` / `⌘N`
/// shortcut. Returns the slot index `1..=9` on match, or `None` for
/// any other key.
///
/// Mirrors the same modifier semantics as [`matches_accel`]: on macOS
/// the `held_meta` flag must be true; on every other platform the
/// `held_ctrl` flag must be true. The `event_text` parameter is the
/// raw Slint `event.text` payload — we only inspect the first ASCII
/// digit so multi-byte sequences (the macOS Cmd-prefixed text payload,
/// e.g. some IMEs) are tolerated without panic.
#[must_use]
pub fn parse_sidebar_shortcut(event_text: &str, held_ctrl: bool, held_meta: bool) -> Option<u8> {
    if !matches_accel(event_text, held_ctrl, held_meta) {
        return None;
    }
    let first = event_text.chars().next()?;
    let digit = first.to_digit(10)?;
    if (1..=9).contains(&digit) {
        u8::try_from(digit).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn format_shortcut_macos_renders_cmd_shift_t() {
        let out = format_shortcut(&["Shift", "T"]);
        assert_eq!(out.as_str(), "⌘⇧T");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn format_shortcut_non_macos_renders_ctrl_shift_t() {
        let out = format_shortcut(&["Shift", "T"]);
        assert_eq!(out.as_str(), "Ctrl+Shift+T");
    }

    // ── M173 Lane A: sidebar shortcut helpers ──────────────────────────

    #[cfg(target_os = "macos")]
    #[test]
    fn sidebar_shortcut_label_macos() {
        assert_eq!(sidebar_shortcut_label(1).as_str(), "⌘1");
        assert_eq!(sidebar_shortcut_label(9).as_str(), "⌘9");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn sidebar_shortcut_label_non_macos() {
        assert_eq!(sidebar_shortcut_label(1).as_str(), "Ctrl+1");
        assert_eq!(sidebar_shortcut_label(9).as_str(), "Ctrl+9");
    }

    #[test]
    fn sidebar_shortcut_label_out_of_range_is_empty() {
        assert_eq!(sidebar_shortcut_label(0).as_str(), "");
        assert_eq!(sidebar_shortcut_label(10).as_str(), "");
        assert_eq!(sidebar_shortcut_label(255).as_str(), "");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn parse_sidebar_shortcut_non_macos_requires_ctrl() {
        assert_eq!(parse_sidebar_shortcut("3", true, false), Some(3));
        // Without ctrl, no match.
        assert_eq!(parse_sidebar_shortcut("3", false, false), None);
        // 0 is out of range.
        assert_eq!(parse_sidebar_shortcut("0", true, false), None);
        // Non-digit text never matches.
        assert_eq!(parse_sidebar_shortcut("a", true, false), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parse_sidebar_shortcut_macos_requires_meta() {
        assert_eq!(parse_sidebar_shortcut("3", false, true), Some(3));
        assert_eq!(parse_sidebar_shortcut("3", false, false), None);
    }

    #[test]
    fn parse_sidebar_shortcut_empty_text_returns_none() {
        assert_eq!(parse_sidebar_shortcut("", true, true), None);
    }

    // ── M173 Lane A task A10: focus-scope isolation contract ──────────
    //
    // The Slint FocusScope handler in main.slint gates Ctrl+1..9
    // behind a modal-open check (`!show-add-magnet-dialog && ...`).
    // We cannot drive Slint from a unit test, but we can pin the
    // contract on the Rust side by simulating the guard:
    //
    // 1. The shortcut handler MUST NOT fire when any modal flag is
    //    true. We model this with a closure `is_modal_open` mirroring
    //    the Slint expression and assert that the would-be slot is
    //    suppressed when any modal is open.
    //
    // 2. The shortcut handler MUST fire when no modal is open AND
    //    the platform accelerator matches.

    /// Mirrors the Slint guard: the handler fires only when every
    /// modal flag is `false`. Returns the resolved slot when the
    /// shortcut should fire, or `None` when the modal guard or the
    /// accelerator gate suppresses it.
    fn would_fire_shortcut(
        digit_text: &str,
        ctrl: bool,
        meta: bool,
        modals_open: ModalState,
    ) -> Option<u8> {
        if modals_open.any_open() {
            return None;
        }
        parse_sidebar_shortcut(digit_text, ctrl, meta)
    }

    #[derive(Default, Debug, Clone, Copy)]
    struct ModalState {
        add_magnet: bool,
        add_torrent: bool,
        delete_confirm: bool,
        context_menu: bool,
        tweaks: bool,
    }

    impl ModalState {
        fn any_open(self) -> bool {
            self.add_magnet
                || self.add_torrent
                || self.delete_confirm
                || self.context_menu
                || self.tweaks
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn focus_scope_isolation_no_modal_fires() {
        let modals = ModalState::default();
        assert_eq!(
            would_fire_shortcut("1", true, false, modals),
            Some(1),
            "no modal open + Ctrl+1 must fire"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn focus_scope_isolation_add_magnet_modal_blocks() {
        let modals = ModalState {
            add_magnet: true,
            ..Default::default()
        };
        assert_eq!(
            would_fire_shortcut("1", true, false, modals),
            None,
            "add-magnet modal open must suppress shortcut"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn focus_scope_isolation_add_torrent_modal_blocks() {
        let modals = ModalState {
            add_torrent: true,
            ..Default::default()
        };
        assert_eq!(would_fire_shortcut("3", true, false, modals), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn focus_scope_isolation_delete_confirm_modal_blocks() {
        let modals = ModalState {
            delete_confirm: true,
            ..Default::default()
        };
        assert_eq!(would_fire_shortcut("5", true, false, modals), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn focus_scope_isolation_context_menu_blocks() {
        // The right-click context menu should also suppress sidebar
        // shortcuts (a Ctrl+N press while the menu is up should not
        // also navigate the sidebar).
        let modals = ModalState {
            context_menu: true,
            ..Default::default()
        };
        assert_eq!(would_fire_shortcut("1", true, false, modals), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn focus_scope_isolation_tweaks_overlay_blocks() {
        // Mirrors the M173 Lane A change to extend the modal guard
        // with `!show-tweaks` — the Tweaks overlay absorbs its own
        // Ctrl+N shortcuts; sidebar must not also fire.
        let modals = ModalState {
            tweaks: true,
            ..Default::default()
        };
        assert_eq!(would_fire_shortcut("9", true, false, modals), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn focus_scope_isolation_dismiss_modal_re_enables() {
        // Open a modal, dismiss it, the next shortcut fires.
        let mut modals = ModalState {
            add_magnet: true,
            ..Default::default()
        };
        assert_eq!(would_fire_shortcut("1", true, false, modals), None);
        modals.add_magnet = false;
        assert_eq!(would_fire_shortcut("1", true, false, modals), Some(1));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn focus_scope_isolation_no_modal_but_no_modifier_does_not_fire() {
        // Even with all modals closed, the shortcut is gated on the
        // platform accelerator. Plain "1" without Ctrl does not fire.
        let modals = ModalState::default();
        assert_eq!(would_fire_shortcut("1", false, false, modals), None);
    }
}
