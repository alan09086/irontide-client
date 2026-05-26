#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M219 window geometry: pixel coordinates fit comfortably in f32's 23-bit mantissa and i32; truncation/precision-loss is intentional"
)]

//! Window-geometry capture, restore, and off-screen clamping (M219).
//!
//! Used by `main.rs` at two points:
//! - On launch (post first-run wizard): [`apply_window_config`] restores
//!   size + position from the loaded `WindowConfig`, clamping the position
//!   so the title bar remains grabbable.
//! - On shutdown: [`to_window_config`] captures the current window
//!   geometry into a fresh `WindowConfig` ready to be persisted via
//!   `irontide_config::save_gui_config`.
//!
//! Slint 1.16 exposes `Window::size`, `Window::position`,
//! `Window::set_size`, and `Window::set_position` on the stable public
//! API; `set_maximized` is NOT available, so [`apply_window_config`]
//! logs and skips when `WindowConfig::maximized` is `Some(true)`.
//!
//! ## Wayland note
//!
//! On Wayland compositors (including KDE kwin), `set_position` is
//! advisory per protocol and is typically ignored. Size restore works
//! reliably; position restore is best-effort. X11 and macOS apply both
//! axes faithfully.

use irontide_config::WindowConfig;
use slint::ComponentHandle;

/// Minimum visible title-bar width (logical px) we guarantee on
/// horizontal clamps. If the user's last window was 800 px wide and is
/// fully off the right of a 1920 px screen, we clamp `x` so that at
/// least 200 px of the title bar (the draggable region with the close
/// / maximise / minimise buttons) remains inside screen bounds.
const MIN_TITLE_BAR_VISIBLE: i32 = 200;

/// Minimum visible title-bar height (logical px) we guarantee on
/// vertical clamps. 64 px is a generous estimate of a typical
/// `Frame + Titlebar` combined height across desktop chromes.
const MIN_TITLE_BAR_HEIGHT: i32 = 64;

/// Fallback screen rect used when we cannot query the live screen
/// dimensions through Slint. A 1920x1080 single-monitor assumption is
/// conservative — the clamp will reject true multi-monitor `x = -1800`
/// positions, which the user can correct manually by dragging the
/// window after launch.
const FALLBACK_SCREEN: (i32, i32, i32, i32) = (0, 0, 1920, 1080);

/// Pure off-screen clamp helper.
///
/// Given a window's logical `pos` (top-left), `size` (width, height),
/// and the available `screen` rect `(x, y, width, height)`, return a
/// clamped `(x, y)` that keeps at least [`MIN_TITLE_BAR_VISIBLE`]
/// horizontal pixels and [`MIN_TITLE_BAR_HEIGHT`] vertical pixels of
/// the title bar inside the screen.
#[must_use]
pub fn clamp_position(
    pos: (i32, i32),
    size: (f32, f32),
    screen: (i32, i32, i32, i32),
) -> (i32, i32) {
    let (px, py) = pos;
    let w = size.0.round() as i32;
    let (sx, sy, sw, sh) = screen;
    // `size.1` (height) is intentionally unused: the title-bar visible-
    // height invariant is bounded by [`MIN_TITLE_BAR_HEIGHT`], not the
    // whole window height — the user can hang the body off-screen and
    // still grab the title bar to drag the window back on-screen.
    let _ = size.1;

    // X axis: the window's right edge must be at least
    // MIN_TITLE_BAR_VISIBLE pixels right of the screen's left edge
    // (so px >= sx - (w - MIN_TITLE_BAR_VISIBLE)), AND the window's
    // left edge must be at most MIN_TITLE_BAR_VISIBLE pixels left of
    // the screen's right edge (so px <= sx + sw - MIN_TITLE_BAR_VISIBLE).
    let x_min = sx - (w - MIN_TITLE_BAR_VISIBLE);
    let x_max = sx + sw - MIN_TITLE_BAR_VISIBLE;
    let clamped_x = px.clamp(x_min, x_max);

    // Y axis: same rule, with MIN_TITLE_BAR_HEIGHT as the visible
    // budget (title-bar is always at the top of the frame, so we
    // also disallow dragging the title bar above screen.y).
    let y_min = sy;
    let y_max = sy + sh - MIN_TITLE_BAR_HEIGHT;
    let clamped_y = py.clamp(y_min, y_max);

    (clamped_x, clamped_y)
}

/// Capture the live window's logical geometry into a `WindowConfig`
/// suitable for `irontide_config::save_gui_config`.
///
/// The `maximized` field is left at `None` for now (Slint 1.16's
/// public `Window` API does not expose a read-back for maximised
/// state). Whatever value was loaded from `config.toml` is preserved
/// by the caller, who reads the loaded value before overwriting.
#[must_use]
pub fn to_window_config(main_window: &crate::MainWindow) -> WindowConfig {
    let window = main_window.window();
    let scale = window.scale_factor();
    let physical_size = window.size();
    let physical_pos = window.position();

    // Convert physical → logical at the live scale factor so the
    // persisted values are display-density-independent.
    let width = (physical_size.width as f32) / scale;
    let height = (physical_size.height as f32) / scale;
    let x = ((physical_pos.x as f32) / scale).round() as i32;
    let y = ((physical_pos.y as f32) / scale).round() as i32;

    WindowConfig {
        width: Some(width),
        height: Some(height),
        x: Some(x),
        y: Some(y),
        maximized: None,
    }
}

/// Apply a loaded `WindowConfig` to the live window.
///
/// Position is clamped via [`clamp_position`] against the fallback
/// screen rect (Slint 1.16 does not expose a stable per-monitor query
/// from a `MainWindow`-derived `Window`). `maximized = Some(true)` is
/// logged and skipped — the schema reserves the field but the restore
/// is a no-op until Slint exposes `set_maximized`.
pub fn apply_window_config(main_window: &crate::MainWindow, cfg: &WindowConfig) {
    let window = main_window.window();

    if let (Some(w), Some(h)) = (cfg.width, cfg.height) {
        window.set_size(slint::WindowSize::Logical(slint::LogicalSize::new(w, h)));
        if let (Some(x), Some(y)) = (cfg.x, cfg.y) {
            let (cx, cy) = clamp_position((x, y), (w, h), FALLBACK_SCREEN);
            window.set_position(slint::WindowPosition::Logical(slint::LogicalPosition::new(
                cx as f32, cy as f32,
            )));
        }
    }

    if cfg.maximized == Some(true) {
        tracing::info!(
            "maximized restore is not yet wired in Slint 1.16 — ignoring `maximized = true` from config"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_passes_through_in_bounds() {
        let pos = (100, 100);
        let size = (800.0, 600.0);
        let screen = (0, 0, 1920, 1080);
        assert_eq!(clamp_position(pos, size, screen), (100, 100));
    }

    #[test]
    fn clamp_off_screen_left() {
        // pos.x = -2000, window 800 wide → must be >= -(800 - 200) = -600.
        let pos = (-2000, 100);
        let size = (800.0, 600.0);
        let screen = (0, 0, 1920, 1080);
        let (cx, _) = clamp_position(pos, size, screen);
        assert_eq!(cx, -600);
    }

    #[test]
    fn clamp_off_screen_right() {
        // pos.x = 1900 on 1920-wide screen → must be <= 1920 - 200 = 1720.
        let pos = (1900, 100);
        let size = (800.0, 600.0);
        let screen = (0, 0, 1920, 1080);
        let (cx, _) = clamp_position(pos, size, screen);
        assert_eq!(cx, 1720);
    }

    #[test]
    fn clamp_off_screen_top() {
        // Title bar cannot go above screen.y.
        let pos = (100, -500);
        let size = (800.0, 600.0);
        let screen = (0, 0, 1920, 1080);
        let (_, cy) = clamp_position(pos, size, screen);
        assert_eq!(cy, 0);
    }

    #[test]
    fn clamp_off_screen_bottom() {
        // pos.y = 1100 on 1080-tall screen → must be <= 1080 - 64 = 1016.
        let pos = (100, 1100);
        let size = (800.0, 600.0);
        let screen = (0, 0, 1920, 1080);
        let (_, cy) = clamp_position(pos, size, screen);
        assert_eq!(cy, 1016);
    }

    #[test]
    fn clamp_exact_boundary_passes_through() {
        // Window placed so its right edge is exactly MIN_TITLE_BAR_VISIBLE
        // from screen.right and top at screen.top.
        let pos = (1720, 0);
        let size = (800.0, 600.0);
        let screen = (0, 0, 1920, 1080);
        assert_eq!(clamp_position(pos, size, screen), (1720, 0));
    }

    #[test]
    fn clamp_multi_monitor_negative_x_within_extended_screen() {
        // If the caller injects a screen rect that includes a left-of-
        // primary monitor (-1920, 0, 3840, 1080), an x = -1800 window
        // with size (800, 600) is fully on-screen and passes through.
        let pos = (-1800, 100);
        let size = (800.0, 600.0);
        let screen = (-1920, 0, 3840, 1080);
        assert_eq!(clamp_position(pos, size, screen), (-1800, 100));
    }
}
