use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub fn try_init_tray() -> Result<TrayHandle, Box<dyn std::error::Error>> {
    gtk::init().map_err(|_| "GTK initialisation failed (no display?)")?;
    Ok(TrayHandle::new()?)
}

const ICON_SIZE: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Downloading,
    Seeding,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ShowHide,
    PauseAll,
    ResumeAll,
    Quit,
}

pub struct TrayHandle {
    icon: TrayIcon,
    show_hide_id: tray_icon::menu::MenuId,
    pause_all_id: tray_icon::menu::MenuId,
    resume_all_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
    current_state: TrayState,
}

#[must_use]
fn generate_icon_rgba(state: TrayState) -> Vec<u8> {
    let (r, g, b) = match state {
        TrayState::Idle => (128, 128, 128),
        TrayState::Downloading => (66, 133, 244),
        TrayState::Seeding => (52, 168, 83),
        TrayState::Error => (234, 67, 53),
    };
    let size = ICON_SIZE as usize;
    #[allow(
        clippy::cast_precision_loss,
        reason = "ICON_SIZE is 32, well within f64 precision"
    )]
    let centre = size as f64 / 2.0;
    let radius = centre - 1.0;
    let mut data = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            #[allow(clippy::cast_precision_loss, reason = "loop index ≤ 32")]
            let dx = x as f64 - centre;
            #[allow(clippy::cast_precision_loss, reason = "loop index ≤ 32")]
            let dy = y as f64 - centre;
            let dist = (dx * dx + dy * dy).sqrt();
            let offset = (y * size + x) * 4;
            if dist <= radius {
                let alpha = if dist > radius - 1.0 {
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "clamped 0..=255"
                    )]
                    let a = ((radius - dist) * 255.0) as u8;
                    a
                } else {
                    255
                };
                data[offset] = r;
                data[offset + 1] = g;
                data[offset + 2] = b;
                data[offset + 3] = alpha;
            }
        }
    }
    data
}

impl TrayHandle {
    pub fn new() -> Result<Self, tray_icon::Error> {
        let show_hide = MenuItem::new("Show / Hide Window", true, None);
        let pause_all = MenuItem::new("Pause All", true, None);
        let resume_all = MenuItem::new("Resume All", true, None);
        let quit = MenuItem::new("Quit", true, None);

        let menu = Menu::new();
        let _ = menu.append(&show_hide);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&pause_all);
        let _ = menu.append(&resume_all);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        let rgba = generate_icon_rgba(TrayState::Idle);
        let icon = Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE)
            .map_err(|e| tray_icon::Error::OsError(std::io::Error::other(e)))?;

        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip("IronTide")
            .build()?;

        Ok(Self {
            icon: tray,
            show_hide_id: show_hide.id().clone(),
            pause_all_id: pause_all.id().clone(),
            resume_all_id: resume_all.id().clone(),
            quit_id: quit.id().clone(),
            current_state: TrayState::Idle,
        })
    }

    #[must_use]
    pub fn resolve_action(&self, event: &MenuEvent) -> Option<TrayAction> {
        if event.id == self.show_hide_id {
            Some(TrayAction::ShowHide)
        } else if event.id == self.pause_all_id {
            Some(TrayAction::PauseAll)
        } else if event.id == self.resume_all_id {
            Some(TrayAction::ResumeAll)
        } else if event.id == self.quit_id {
            Some(TrayAction::Quit)
        } else {
            None
        }
    }

    pub fn update_state(&mut self, new_state: TrayState) {
        if self.current_state == new_state {
            return;
        }
        let rgba = generate_icon_rgba(new_state);
        if let Ok(icon) = Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE) {
            let _ = self.icon.set_icon(Some(icon));
            self.current_state = new_state;
        }
    }
}

#[must_use]
pub fn derive_tray_state(has_downloading: bool, has_seeding: bool, has_error: bool) -> TrayState {
    if has_error {
        TrayState::Error
    } else if has_downloading {
        TrayState::Downloading
    } else if has_seeding {
        TrayState::Seeding
    } else {
        TrayState::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_icon_rgba_dimensions() {
        let data = generate_icon_rgba(TrayState::Idle);
        let expected = (ICON_SIZE * ICON_SIZE * 4) as usize;
        assert_eq!(data.len(), expected);
    }

    #[test]
    fn generate_icon_rgba_centre_pixel_opaque() {
        let data = generate_icon_rgba(TrayState::Downloading);
        let centre = ICON_SIZE as usize / 2;
        let offset = (centre * ICON_SIZE as usize + centre) * 4;
        assert_eq!(data[offset + 3], 255, "centre pixel should be fully opaque");
        assert_eq!(
            data[offset], 66,
            "centre pixel R should match downloading blue"
        );
    }

    #[test]
    fn generate_icon_rgba_corner_transparent() {
        let data = generate_icon_rgba(TrayState::Seeding);
        assert_eq!(data[3], 0, "top-left corner should be transparent");
    }

    #[test]
    fn derive_tray_state_priority() {
        assert_eq!(derive_tray_state(false, false, false), TrayState::Idle);
        assert_eq!(
            derive_tray_state(true, false, false),
            TrayState::Downloading
        );
        assert_eq!(derive_tray_state(false, true, false), TrayState::Seeding);
        assert_eq!(derive_tray_state(true, true, false), TrayState::Downloading);
        assert_eq!(derive_tray_state(true, true, true), TrayState::Error);
        assert_eq!(derive_tray_state(false, false, true), TrayState::Error);
    }

    #[test]
    fn all_states_produce_valid_icons() {
        for state in [
            TrayState::Idle,
            TrayState::Downloading,
            TrayState::Seeding,
            TrayState::Error,
        ] {
            let data = generate_icon_rgba(state);
            assert_eq!(data.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
            assert!(
                Icon::from_rgba(data, ICON_SIZE, ICON_SIZE).is_ok(),
                "state {state:?} should produce a valid Icon"
            );
        }
    }
}
