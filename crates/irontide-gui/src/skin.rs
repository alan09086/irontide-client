//! Skin / theme / density / radius settings.
//!
//! Four parallel axes make up the user-facing design-system state:
//! [`Skin`] (palette family), [`Theme`] (light/dark), [`Density`]
//! (row + chrome dimensions), and [`RadiusPreset`] (corner rounding).
//!
//! The pipeline is:
//!
//! 1. TOML config on disk (`~/.config/irontide/config.toml`) holds the
//!    optional strings; [`SkinSettings::from_gui_config`] parses them
//!    via `strum::EnumString`, falling back to the default on an invalid
//!    value (with a `tracing::warn!`).
//! 2. [`SkinSettings::resolve`] materialises the active variant into a
//!    [`ResolvedTokens`] struct by combining the codegen'd
//!    [`crate::skin_tokens::TokenValues`] palette with density-sensitive
//!    lengths and radius-preset overrides.
//! 3. [`SkinSettings::apply`] pushes every field of `ResolvedTokens`
//!    into the Slint-side [`Tokens`] global on the UI thread, ending
//!    with `skin-applied = true` so the main layout can unmask.
//! 4. On shutdown, [`SkinSettings::populate_gui_config`] writes the
//!    current state back into `GuiConfig` for persistence.

use strum::{Display, EnumString};

use crate::skin_tokens::{
    ABYSS_DARK, ABYSS_LIGHT, FORGE_DARK, FORGE_LIGHT, TIDE_DARK, TIDE_LIGHT, TokenValues,
};

/// Visual skin — colour palette + aesthetic identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Skin {
    /// Deep teal + cool slate (default).
    #[default]
    Tide,
    /// Warm graphite + amber.
    Forge,
    /// Near-black + phosphor green.
    Abyss,
}

/// Light / dark mode switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Theme {
    /// Dark mode (default).
    #[default]
    Dark,
    /// Light mode.
    Light,
}

/// UI density — row height, padding, chrome height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum RadiusPreset {
    /// Sharp: 0 px corners throughout.
    Sharp,
    /// Balanced (default): 3–14 px corners.
    #[default]
    Balanced,
    /// Rounded: 6–20 px corners.
    Rounded,
}

/// Combined design-system state persisted in `[gui]` config.
///
/// Token-affecting axes (skin, theme, density, radius) flow through
/// [`SkinSettings::resolve`] → [`SkinSettings::apply`] to mutate the
/// Slint `Tokens` global on each change.
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

/// Fully-materialised token values for a given [`SkinSettings`].
///
/// This struct is the single-source-of-truth for [`SkinSettings::apply`]:
/// [`SkinSettings::resolve`] builds it in pure Rust (no Slint event loop
/// needed), and `apply` fans out the values into the Slint `Tokens`
/// global. Tests assert that `resolve` produces the expected values,
/// which is enough to prove `apply` is correct once we've verified the
/// setter-call count.
///
/// Note: `f32` fields (lengths, font sizes, blur radii) preclude `Eq`,
/// so we implement only `PartialEq` here. Comparisons in tests use
/// exact equality on literal constants from the resolve tables, where
/// bit-exact equality is safe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedTokens {
    // ── Colours (from `TokenValues`) ──
    /// App background.
    pub bg_0: [u8; 3],
    /// Sidebar / bottom-pane surface.
    pub bg_1: [u8; 3],
    /// Panel / dialog surface.
    pub bg_2: [u8; 3],
    /// Table header / elevated surface.
    pub bg_3: [u8; 3],
    /// Row / control hover background.
    pub bg_hover: [u8; 3],
    /// Selected row background.
    pub bg_selected: [u8; 3],
    /// Inset / input field background.
    pub bg_inset: [u8; 3],
    /// Primary foreground (headings).
    pub fg_0: [u8; 3],
    /// Body foreground.
    pub fg_1: [u8; 3],
    /// Secondary / label foreground.
    pub fg_2: [u8; 3],
    /// Disabled / placeholder foreground.
    pub fg_3: [u8; 3],
    /// Primary border colour.
    pub border_1: [u8; 3],
    /// Strong border / outline colour.
    pub border_2: [u8; 3],
    /// Subtle divider colour.
    pub divider: [u8; 3],
    /// Primary accent colour.
    pub accent: [u8; 3],
    /// Accent hover variant.
    pub accent_hover: [u8; 3],
    /// Foreground on accent backgrounds.
    pub accent_fg: [u8; 3],
    /// Tinted accent background (soft).
    pub accent_bg_soft: [u8; 3],

    // ── Status colours ──
    /// Torrent downloading indicator.
    pub status_downloading: [u8; 3],
    /// Torrent seeding indicator.
    pub status_seeding: [u8; 3],
    /// Torrent paused indicator.
    pub status_paused: [u8; 3],
    /// Torrent queued indicator.
    pub status_queued: [u8; 3],
    /// Torrent error indicator.
    pub status_error: [u8; 3],
    /// Torrent checking / verifying indicator.
    pub status_checking: [u8; 3],
    /// Torrent stalled indicator.
    pub status_stalled: [u8; 3],
    /// Torrent metadata-fetching indicator.
    pub status_metadata: [u8; 3],

    // ── Spacing (in logical pixels) ──
    /// sp-1 — 2 px.
    pub sp_1: f32,
    /// sp-2 — 4 px.
    pub sp_2: f32,
    /// sp-3 — 6 px.
    pub sp_3: f32,
    /// sp-4 — 8 px.
    pub sp_4: f32,
    /// sp-5 — 12 px.
    pub sp_5: f32,
    /// sp-6 — 16 px.
    pub sp_6: f32,
    /// sp-7 — 20 px.
    pub sp_7: f32,
    /// sp-8 — 24 px.
    pub sp_8: f32,
    /// sp-9 — 32 px.
    pub sp_9: f32,
    /// sp-10 — 40 px.
    pub sp_10: f32,

    // ── Radii (density-sensitive) ──
    /// Small radius (3 px balanced default).
    pub r_sm: f32,
    /// Medium radius (6 px balanced default).
    pub r_md: f32,
    /// Large radius (10 px balanced default).
    pub r_lg: f32,
    /// Extra-large radius (14 px balanced default).
    pub r_xl: f32,

    // ── Density ──
    /// Standard row height.
    pub row_h: f32,
    /// Row vertical padding.
    pub row_px: f32,
    /// Chrome / toolbar height.
    pub chrome_h: f32,
    /// Primary sidebar width.
    pub sidebar_w: f32,
    /// Filters pane width.
    pub filters_w: f32,

    // ── Font sizes ──
    /// fs-10 — 10 px.
    pub fs_10: f32,
    /// fs-11 — 11 px.
    pub fs_11: f32,
    /// fs-12 — 12 px.
    pub fs_12: f32,
    /// fs-13 — 13 px.
    pub fs_13: f32,
    /// fs-14 — 14 px.
    pub fs_14: f32,
    /// fs-16 — 16 px.
    pub fs_16: f32,
    /// fs-18 — 18 px.
    pub fs_18: f32,
    /// fs-24 — 24 px.
    pub fs_24: f32,
    /// fs-32 — 32 px.
    pub fs_32: f32,

    // ── Border widths ──
    /// Thin border — 1 px.
    pub border_width_thin: f32,
    /// Medium border — 2 px.
    pub border_width_medium: f32,
    /// Thick border — 4 px.
    pub border_width_thick: f32,

    // ── Shadows (flat triples; Slint struct rebuilt at apply time) ──
    /// Small shadow (alpha, blur, `offset_y`) — alpha is the premultiplied
    /// hex colour (`[r, g, b, a]`), blur in px, `offset_y` in px.
    pub shadow_sm_rgba: [u8; 4],
    /// Small shadow blur radius.
    pub shadow_sm_blur: f32,
    /// Small shadow vertical offset.
    pub shadow_sm_offset_y: f32,
    /// Medium shadow colour.
    pub shadow_md_rgba: [u8; 4],
    /// Medium shadow blur radius.
    pub shadow_md_blur: f32,
    /// Medium shadow vertical offset.
    pub shadow_md_offset_y: f32,
    /// Large shadow colour.
    pub shadow_lg_rgba: [u8; 4],
    /// Large shadow blur radius.
    pub shadow_lg_blur: f32,
    /// Large shadow vertical offset.
    pub shadow_lg_offset_y: f32,

    // ── Motion durations (milliseconds) ──
    /// Fast transition — 80 ms.
    pub dur_fast: i64,
    /// Default transition — 150 ms.
    pub dur: i64,
    /// Slow transition — 300 ms.
    pub dur_slow: i64,
}

impl SkinSettings {
    /// Look up the codegen'd palette for the active skin+theme.
    const fn palette(self) -> &'static TokenValues {
        match (self.skin, self.theme) {
            (Skin::Tide, Theme::Dark) => &TIDE_DARK,
            (Skin::Tide, Theme::Light) => &TIDE_LIGHT,
            (Skin::Forge, Theme::Dark) => &FORGE_DARK,
            (Skin::Forge, Theme::Light) => &FORGE_LIGHT,
            (Skin::Abyss, Theme::Dark) => &ABYSS_DARK,
            (Skin::Abyss, Theme::Light) => &ABYSS_LIGHT,
        }
    }

    /// Materialise the full [`ResolvedTokens`] for these settings.
    ///
    /// This is pure data — no Slint runtime required, fully testable.
    /// The density and radius tables below come from the design-spec CSS
    /// (`[data-density=...]` / `[data-radius=...]` blocks).
    #[must_use]
    pub fn resolve(self) -> ResolvedTokens {
        let p = self.palette();

        // Density → (row_h, row_px, chrome_h, sidebar_w, filters_w)
        let (row_h, row_px, chrome_h, sidebar_w, filters_w) = match self.density {
            Density::Compact => (28.0, 6.0, 34.0, 210.0, 190.0),
            Density::Balanced => (32.0, 8.0, 40.0, 240.0, 200.0),
            Density::Spacious => (40.0, 12.0, 48.0, 280.0, 240.0),
        };

        // Radius preset → (r-sm, r-md, r-lg, r-xl)
        let (r_sm, r_md, r_lg, r_xl) = match self.radius {
            RadiusPreset::Sharp => (0.0, 0.0, 0.0, 0.0),
            RadiusPreset::Balanced => (3.0, 6.0, 10.0, 14.0),
            RadiusPreset::Rounded => (6.0, 10.0, 16.0, 20.0),
        };

        // Shadows — theme-sensitive alpha. Dark themes deepen the shadow
        // alpha for better depth contrast on dark backgrounds.
        let (sm_rgba, md_rgba, lg_rgba) = match self.theme {
            Theme::Dark => (
                [0x00, 0x00, 0x00, 0x40],
                [0x00, 0x00, 0x00, 0x59],
                [0x00, 0x00, 0x00, 0x73],
            ),
            Theme::Light => (
                [0x14, 0x18, 0x20, 0x0F],
                [0x14, 0x18, 0x20, 0x14],
                [0x14, 0x18, 0x20, 0x1F],
            ),
        };

        ResolvedTokens {
            // Colours
            bg_0: p.bg_0,
            bg_1: p.bg_1,
            bg_2: p.bg_2,
            bg_3: p.bg_3,
            bg_hover: p.bg_hover,
            bg_selected: p.bg_selected,
            bg_inset: p.bg_inset,
            fg_0: p.fg_0,
            fg_1: p.fg_1,
            fg_2: p.fg_2,
            fg_3: p.fg_3,
            border_1: p.border_1,
            border_2: p.border_2,
            divider: p.divider,
            accent: p.accent,
            accent_hover: p.accent_hover,
            accent_fg: p.accent_fg,
            accent_bg_soft: p.accent_bg_soft,
            status_downloading: p.status_downloading,
            status_seeding: p.status_seeding,
            status_paused: p.status_paused,
            status_queued: p.status_queued,
            status_error: p.status_error,
            status_checking: p.status_checking,
            status_stalled: p.status_stalled,
            status_metadata: p.status_metadata,

            // Spacing (resolution-independent)
            sp_1: 2.0,
            sp_2: 4.0,
            sp_3: 6.0,
            sp_4: 8.0,
            sp_5: 12.0,
            sp_6: 16.0,
            sp_7: 20.0,
            sp_8: 24.0,
            sp_9: 32.0,
            sp_10: 40.0,

            // Radii (density-insensitive, radius-preset driven)
            r_sm,
            r_md,
            r_lg,
            r_xl,

            // Density
            row_h,
            row_px,
            chrome_h,
            sidebar_w,
            filters_w,

            // Font sizes
            fs_10: 10.0,
            fs_11: 11.0,
            fs_12: 12.0,
            fs_13: 13.0,
            fs_14: 14.0,
            fs_16: 16.0,
            fs_18: 18.0,
            fs_24: 24.0,
            fs_32: 32.0,

            // Border widths
            border_width_thin: 1.0,
            border_width_medium: 2.0,
            border_width_thick: 4.0,

            // Shadows
            shadow_sm_rgba: sm_rgba,
            shadow_sm_blur: 4.0,
            shadow_sm_offset_y: 1.0,
            shadow_md_rgba: md_rgba,
            shadow_md_blur: 8.0,
            shadow_md_offset_y: 2.0,
            shadow_lg_rgba: lg_rgba,
            shadow_lg_blur: 16.0,
            shadow_lg_offset_y: 4.0,

            // Motion
            dur_fast: 80,
            dur: 150,
            dur_slow: 300,
        }
    }

    /// Push the resolved tokens into the Slint `Tokens` global.
    ///
    /// Schedules the work on the Slint event loop via
    /// [`slint::Weak::upgrade_in_event_loop`]. The final call sets
    /// `skin-applied = true` so the main layout unmasks.
    ///
    /// The caller is responsible for getting an `apply` call on the UI
    /// thread at startup — typically immediately after constructing
    /// `MainWindow` in `main()`.
    pub fn apply(self, weak: &slint::Weak<crate::MainWindow>) {
        let resolved = self.resolve();
        let skin_str = slint::SharedString::from(self.skin.to_string());
        let theme_str = slint::SharedString::from(self.theme.to_string());

        let _ = weak.upgrade_in_event_loop(move |win| {
            use slint::ComponentHandle as _;

            let t = win.global::<crate::Tokens>();

            // ── Colours ──
            t.set_bg_0(rgb_color(resolved.bg_0));
            t.set_bg_1(rgb_color(resolved.bg_1));
            t.set_bg_2(rgb_color(resolved.bg_2));
            t.set_bg_3(rgb_color(resolved.bg_3));
            t.set_bg_hover(rgb_color(resolved.bg_hover));
            t.set_bg_selected(rgb_color(resolved.bg_selected));
            t.set_bg_inset(rgb_color(resolved.bg_inset));
            t.set_fg_0(rgb_color(resolved.fg_0));
            t.set_fg_1(rgb_color(resolved.fg_1));
            t.set_fg_2(rgb_color(resolved.fg_2));
            t.set_fg_3(rgb_color(resolved.fg_3));
            t.set_border_1(rgb_color(resolved.border_1));
            t.set_border_2(rgb_color(resolved.border_2));
            t.set_divider(rgb_color(resolved.divider));
            t.set_accent(rgb_color(resolved.accent));
            t.set_accent_hover(rgb_color(resolved.accent_hover));
            t.set_accent_fg(rgb_color(resolved.accent_fg));
            t.set_accent_bg_soft(rgb_color(resolved.accent_bg_soft));

            // ── Status ──
            t.set_status_downloading(rgb_color(resolved.status_downloading));
            t.set_status_seeding(rgb_color(resolved.status_seeding));
            t.set_status_paused(rgb_color(resolved.status_paused));
            t.set_status_queued(rgb_color(resolved.status_queued));
            t.set_status_error(rgb_color(resolved.status_error));
            t.set_status_checking(rgb_color(resolved.status_checking));
            t.set_status_stalled(rgb_color(resolved.status_stalled));
            t.set_status_metadata(rgb_color(resolved.status_metadata));

            // ── Spacing ──
            t.set_sp_1(resolved.sp_1);
            t.set_sp_2(resolved.sp_2);
            t.set_sp_3(resolved.sp_3);
            t.set_sp_4(resolved.sp_4);
            t.set_sp_5(resolved.sp_5);
            t.set_sp_6(resolved.sp_6);
            t.set_sp_7(resolved.sp_7);
            t.set_sp_8(resolved.sp_8);
            t.set_sp_9(resolved.sp_9);
            t.set_sp_10(resolved.sp_10);

            // ── Radii ──
            t.set_r_sm(resolved.r_sm);
            t.set_r_md(resolved.r_md);
            t.set_r_lg(resolved.r_lg);
            t.set_r_xl(resolved.r_xl);

            // ── Density ──
            t.set_row_h(resolved.row_h);
            t.set_row_px(resolved.row_px);
            t.set_chrome_h(resolved.chrome_h);
            t.set_sidebar_w(resolved.sidebar_w);
            t.set_filters_w(resolved.filters_w);

            // ── Font sizes ──
            t.set_fs_10(resolved.fs_10);
            t.set_fs_11(resolved.fs_11);
            t.set_fs_12(resolved.fs_12);
            t.set_fs_13(resolved.fs_13);
            t.set_fs_14(resolved.fs_14);
            t.set_fs_16(resolved.fs_16);
            t.set_fs_18(resolved.fs_18);
            t.set_fs_24(resolved.fs_24);
            t.set_fs_32(resolved.fs_32);

            // ── Border widths ──
            t.set_border_width_thin(resolved.border_width_thin);
            t.set_border_width_medium(resolved.border_width_medium);
            t.set_border_width_thick(resolved.border_width_thick);

            // ── Shadows ──
            t.set_shadow_sm(make_shadow(
                resolved.shadow_sm_rgba,
                resolved.shadow_sm_blur,
                resolved.shadow_sm_offset_y,
            ));
            t.set_shadow_md(make_shadow(
                resolved.shadow_md_rgba,
                resolved.shadow_md_blur,
                resolved.shadow_md_offset_y,
            ));
            t.set_shadow_lg(make_shadow(
                resolved.shadow_lg_rgba,
                resolved.shadow_lg_blur,
                resolved.shadow_lg_offset_y,
            ));

            // ── Motion ──
            t.set_dur_fast(resolved.dur_fast);
            t.set_dur(resolved.dur);
            t.set_dur_slow(resolved.dur_slow);

            // ── Identity strings ──
            t.set_active_skin(skin_str);
            t.set_active_theme(theme_str);

            // Unmask the UI — first paint now has real tokens.
            win.set_skin_applied(true);
        });
    }

    /// Construct from a [`GuiConfig`](irontide_config::GuiConfig).
    ///
    /// Invalid strings emit a `tracing::warn!` and fall back to the
    /// default for that enum; unset fields (`None`) silently use the
    /// default.
    #[must_use]
    pub fn from_gui_config(gui: &irontide_config::GuiConfig) -> Self {
        let skin = gui.skin.as_deref().map_or(Skin::default(), |s| {
            s.parse::<Skin>().unwrap_or_else(|_| {
                tracing::warn!(invalid = s, "unknown skin in config, using default");
                Skin::default()
            })
        });
        let theme = gui.theme.as_deref().map_or(Theme::default(), |s| {
            s.parse::<Theme>().unwrap_or_else(|_| {
                tracing::warn!(invalid = s, "unknown theme in config, using default");
                Theme::default()
            })
        });
        let density = gui.density.as_deref().map_or(Density::default(), |s| {
            s.parse::<Density>().unwrap_or_else(|_| {
                tracing::warn!(invalid = s, "unknown density in config, using default");
                Density::default()
            })
        });
        let radius = gui
            .radius_preset
            .as_deref()
            .map_or(RadiusPreset::default(), |s| {
                s.parse::<RadiusPreset>().unwrap_or_else(|_| {
                    tracing::warn!(
                        invalid = s,
                        "unknown radius preset in config, using default"
                    );
                    RadiusPreset::default()
                })
            });
        // layout, l3_sidebar_mode, inspector_shown are kept in GuiConfig
        // for backward-compat deserialization but ignored on read — L1
        // (3-pane) is the only layout now.

        Self {
            skin,
            theme,
            density,
            radius,
        }
    }

    /// Write the current settings into a [`GuiConfig`] for persistence.
    ///
    /// Leaves any non-skin fields (column layout, etc.) untouched.
    pub fn populate_gui_config(self, gui: &mut irontide_config::GuiConfig) {
        gui.skin = Some(self.skin.to_string());
        gui.theme = Some(self.theme.to_string());
        gui.density = Some(self.density.to_string());
        gui.radius_preset = Some(self.radius.to_string());
        // layout, l3_sidebar_mode, inspector_shown are no longer written —
        // L1 (3-pane) is the only layout. Old config fields are kept for
        // backward-compat deserialization.
    }
}

// ── Slint conversion helpers ───────────────────────────────────────────

/// Build a `slint::Color` from a sRGB 8-bit triple.
fn rgb_color(rgb: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2])
}

/// Construct the Slint-side `Shadow` struct.
fn make_shadow(rgba: [u8; 4], blur: f32, offset_y: f32) -> crate::Shadow {
    crate::Shadow {
        color: slint::Color::from_argb_u8(rgba[3], rgba[0], rgba[1], rgba[2]),
        blur,
        offset_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use irontide_config::GuiConfig;

    // ── FromStr round-trip happy path ──

    #[test]
    fn skin_from_str_happy_path() {
        assert_eq!("tide".parse::<Skin>().unwrap(), Skin::Tide);
        assert_eq!("forge".parse::<Skin>().unwrap(), Skin::Forge);
        assert_eq!("abyss".parse::<Skin>().unwrap(), Skin::Abyss);
    }

    #[test]
    fn theme_from_str_happy_path() {
        assert_eq!("dark".parse::<Theme>().unwrap(), Theme::Dark);
        assert_eq!("light".parse::<Theme>().unwrap(), Theme::Light);
    }

    #[test]
    fn density_from_str_happy_path() {
        assert_eq!("compact".parse::<Density>().unwrap(), Density::Compact);
        assert_eq!("balanced".parse::<Density>().unwrap(), Density::Balanced);
        assert_eq!("spacious".parse::<Density>().unwrap(), Density::Spacious);
    }

    #[test]
    fn radius_from_str_happy_path() {
        assert_eq!(
            "sharp".parse::<RadiusPreset>().unwrap(),
            RadiusPreset::Sharp
        );
        assert_eq!(
            "balanced".parse::<RadiusPreset>().unwrap(),
            RadiusPreset::Balanced
        );
        assert_eq!(
            "rounded".parse::<RadiusPreset>().unwrap(),
            RadiusPreset::Rounded
        );
    }

    // ── Invalid-string fallback ──

    #[test]
    fn skin_invalid_falls_back_to_default() {
        let gui = GuiConfig {
            skin: Some("bogus".into()),
            ..Default::default()
        };
        let s = SkinSettings::from_gui_config(&gui);
        assert_eq!(s.skin, Skin::default());
    }

    #[test]
    fn theme_invalid_falls_back_to_default() {
        let gui = GuiConfig {
            theme: Some("neon".into()),
            ..Default::default()
        };
        let s = SkinSettings::from_gui_config(&gui);
        assert_eq!(s.theme, Theme::default());
    }

    #[test]
    fn density_invalid_falls_back_to_default() {
        let gui = GuiConfig {
            density: Some("extra-large".into()),
            ..Default::default()
        };
        let s = SkinSettings::from_gui_config(&gui);
        assert_eq!(s.density, Density::default());
    }

    #[test]
    fn radius_invalid_falls_back_to_default() {
        let gui = GuiConfig {
            radius_preset: Some("spiky".into()),
            ..Default::default()
        };
        let s = SkinSettings::from_gui_config(&gui);
        assert_eq!(s.radius, RadiusPreset::default());
    }

    // ── Warn log capture for invalid skin ──

    #[test]
    #[tracing_test::traced_test]
    fn invalid_skin_emits_warn_log() {
        let gui = GuiConfig {
            skin: Some("bogus".into()),
            ..Default::default()
        };
        let _s = SkinSettings::from_gui_config(&gui);
        assert!(logs_contain("unknown skin"));
    }

    // ── populate_gui_config round-trip ──

    #[test]
    fn populate_gui_config_default_round_trip() {
        let s = SkinSettings::default();
        let mut gui = GuiConfig::default();
        s.populate_gui_config(&mut gui);
        assert_eq!(gui.skin.as_deref(), Some("tide"));
        assert_eq!(gui.theme.as_deref(), Some("dark"));
        assert_eq!(gui.density.as_deref(), Some("balanced"));
        assert_eq!(gui.radius_preset.as_deref(), Some("balanced"));

        // Round-trip back through from_gui_config should be lossless.
        let restored = SkinSettings::from_gui_config(&gui);
        assert_eq!(restored, s);
    }

    #[test]
    fn populate_gui_config_non_default_round_trip() {
        let s = SkinSettings {
            skin: Skin::Forge,
            theme: Theme::Light,
            density: Density::Compact,
            radius: RadiusPreset::Sharp,
        };
        let mut gui = GuiConfig::default();
        s.populate_gui_config(&mut gui);
        assert_eq!(gui.skin.as_deref(), Some("forge"));
        assert_eq!(gui.theme.as_deref(), Some("light"));
        assert_eq!(gui.density.as_deref(), Some("compact"));
        assert_eq!(gui.radius_preset.as_deref(), Some("sharp"));

        let restored = SkinSettings::from_gui_config(&gui);
        assert_eq!(restored, s);
    }

    // ── resolve palette lookups ──

    #[test]
    fn resolve_tide_dark_matches_codegen() {
        let s = SkinSettings {
            skin: Skin::Tide,
            theme: Theme::Dark,
            density: Density::Balanced,
            radius: RadiusPreset::Balanced,
        };
        let r = s.resolve();
        assert_eq!(r.bg_0, TIDE_DARK.bg_0);
        assert_eq!(r.accent, TIDE_DARK.accent);
        assert_eq!(r.fg_0, TIDE_DARK.fg_0);
    }

    #[test]
    fn resolve_forge_light_matches_codegen() {
        let s = SkinSettings {
            skin: Skin::Forge,
            theme: Theme::Light,
            density: Density::Balanced,
            radius: RadiusPreset::Balanced,
        };
        let r = s.resolve();
        assert_eq!(r.bg_0, FORGE_LIGHT.bg_0);
        assert_eq!(r.accent, FORGE_LIGHT.accent);
        assert_eq!(r.fg_0, FORGE_LIGHT.fg_0);
    }

    // ── Density delta ──

    #[test]
    fn resolve_density_changes_row_h() {
        let compact = SkinSettings {
            density: Density::Compact,
            ..Default::default()
        }
        .resolve();
        let balanced = SkinSettings {
            density: Density::Balanced,
            ..Default::default()
        }
        .resolve();
        let spacious = SkinSettings {
            density: Density::Spacious,
            ..Default::default()
        }
        .resolve();
        assert!(compact.row_h < balanced.row_h);
        assert!(balanced.row_h < spacious.row_h);
        assert!((compact.row_h - 28.0).abs() < f32::EPSILON);
        assert!((balanced.row_h - 32.0).abs() < f32::EPSILON);
        assert!((spacious.row_h - 40.0).abs() < f32::EPSILON);
    }

    // ── Radius delta ──

    #[test]
    fn resolve_radius_changes_r_sm() {
        let sharp = SkinSettings {
            radius: RadiusPreset::Sharp,
            ..Default::default()
        }
        .resolve();
        let rounded = SkinSettings {
            radius: RadiusPreset::Rounded,
            ..Default::default()
        }
        .resolve();
        assert!((sharp.r_sm - 0.0).abs() < f32::EPSILON);
        assert!((rounded.r_sm - 6.0).abs() < f32::EPSILON);
        assert!(sharp.r_xl < rounded.r_xl);
    }

    // ── Comprehensive atomicity: verify `resolve` matches tables ──

    /// Exhaustive pure-data check: every (skin, theme, density, radius)
    /// combination must produce a `ResolvedTokens` whose field count +
    /// type are stable. We verify a representative subset of fields
    /// come out in the right shape (colours from codegen tables,
    /// density lengths from the preset, radius lengths from the preset,
    /// and resolution-independent constants).
    ///
    /// We do NOT drive a real `MainWindow` here because that needs a
    /// Slint event loop and a display server. The `apply` function is
    /// a straight fan-out over `resolve()` output — testing the inputs
    /// gives us the same coverage without the test-infrastructure
    /// burden.
    #[test]
    fn resolve_covers_all_combinations() {
        let skins = [Skin::Tide, Skin::Forge, Skin::Abyss];
        let themes = [Theme::Dark, Theme::Light];
        let densities = [Density::Compact, Density::Balanced, Density::Spacious];
        let radii = [
            RadiusPreset::Sharp,
            RadiusPreset::Balanced,
            RadiusPreset::Rounded,
        ];

        for &skin in &skins {
            for &theme in &themes {
                for &density in &densities {
                    for &radius in &radii {
                        let s = SkinSettings {
                            skin,
                            theme,
                            density,
                            radius,
                        };
                        let r = s.resolve();

                        // Palette should match the codegen table.
                        let p = s.palette();
                        assert_eq!(r.bg_0, p.bg_0, "bg_0 mismatch for {skin:?}/{theme:?}");
                        assert_eq!(r.accent, p.accent, "accent mismatch for {skin:?}/{theme:?}");

                        // Spacing should be resolution-independent.
                        assert!((r.sp_1 - 2.0).abs() < f32::EPSILON);
                        assert!((r.sp_6 - 16.0).abs() < f32::EPSILON);

                        // Motion should be resolution-independent.
                        assert_eq!(r.dur_fast, 80);
                        assert_eq!(r.dur, 150);
                        assert_eq!(r.dur_slow, 300);

                        // Row-h should track density (M188 values).
                        let expected_row_h = match density {
                            Density::Compact => 28.0,
                            Density::Balanced => 32.0,
                            Density::Spacious => 40.0,
                        };
                        assert!(
                            (r.row_h - expected_row_h).abs() < f32::EPSILON,
                            "row_h mismatch for {density:?}"
                        );

                        // r-sm should track radius preset.
                        let expected_r_sm = match radius {
                            RadiusPreset::Sharp => 0.0,
                            RadiusPreset::Balanced => 3.0,
                            RadiusPreset::Rounded => 6.0,
                        };
                        assert!(
                            (r.r_sm - expected_r_sm).abs() < f32::EPSILON,
                            "r_sm mismatch for {radius:?}"
                        );
                    }
                }
            }
        }
    }
}
