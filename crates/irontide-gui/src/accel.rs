//! Cross-platform accelerator (keyboard shortcut) helper.
//!
//! Slint `.slint` files can't cfg-gate — they see one value for the current
//! platform. This module resolves the accelerator modifier (Meta on macOS,
//! Control elsewhere) at compile time and provides utilities for rendering
//! shortcut strings and matching key events.
//!
//! Lane B wires the Ctrl+Shift+T / Cmd+Shift+T toggle directly into
//! `main.slint`'s FocusScope (via tracked `ctrl-held` / `meta-held`
//! properties), so the Rust helpers are currently only consumed by
//! tests and future menu-label strings. The module-level
//! `#![allow(dead_code)]` covers that gap.

#![allow(dead_code)]

use slint::SharedString;

/// Keyboard modifier used as the primary accelerator on this platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelModifier {
    /// macOS command key (⌘).
    Meta,
    /// Control key (Ctrl) — Linux, Windows, BSD.
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
/// `held_ctrl` and `held_meta` mirror the FocusScope `meta-held` /
/// `ctrl-held` properties.
#[must_use]
pub fn matches_accel(event_text: &str, held_ctrl: bool, held_meta: bool) -> bool {
    match CURRENT_ACCEL {
        AccelModifier::Meta => held_meta && !event_text.is_empty(),
        AccelModifier::Control => held_ctrl && !event_text.is_empty(),
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
}
