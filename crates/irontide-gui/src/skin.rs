//! Skin / theme / density / radius settings.
//!
//! Lane A (this file): type declarations + `Default`. Lane B populates
//! `resolve()` and `apply()`, which will exercise the currently-dormant
//! variants and the `from_gui_config` stub. The `#[allow(dead_code)]` here
//! keeps the build clean under `-D warnings` during the B gap.

#![allow(dead_code)]

/// Visual skin — colour palette + aesthetic identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skin {
    /// Deep-teal default.
    #[default]
    Tide,
    /// Warm bronze variant.
    Forge,
    /// Cool indigo variant.
    Abyss,
}

/// Light / dark mode switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// Dark mode (default).
    #[default]
    Dark,
    /// Light mode.
    Light,
}

/// UI density — row height, padding, chrome height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    /// Compact: tighter spacing, smaller row height.
    Compact,
    /// Balanced (default).
    #[default]
    Balanced,
    /// Spacious: looser spacing.
    Spacious,
}

/// Corner radius preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadiusPreset {
    /// Sharp: 0-2px corners throughout.
    Sharp,
    /// Balanced (default): 3-10px corners.
    #[default]
    Balanced,
    /// Rounded: 6-16px corners.
    Rounded,
}

/// Combined skin/theme/density/radius state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkinSettings {
    /// Active skin.
    pub skin: Skin,
    /// Active theme (light/dark).
    pub theme: Theme,
    /// Active density.
    pub density: Density,
    /// Active radius preset.
    pub radius: RadiusPreset,
}

impl SkinSettings {
    /// Construct from `GuiConfig` string fields.
    ///
    /// Stub for Lane A — Lane B will use `strum::EnumString` for real parsing.
    #[must_use]
    pub fn from_gui_config(_gui: &irontide_config::GuiConfig) -> Self {
        Self::default()
    }
}
