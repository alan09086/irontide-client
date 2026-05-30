# IronTide — Design System (v1.0, FOCUSED)

> **Status:** Production. Single locked direction.
> **Replaces:** the v0.1 multi-skin system (Tide / Forge / Abyss × light / dark × 3 layouts).
> **Built on:** the Alan Gaudet design language — near-black canvas, one emerald signal, warm-charcoal containment, mono for data, borders over shadows.
> **Source of truth:** `styles/tokens.css` (prototype) → port 1:1 to `crates/irontide-gui/ui/tokens.slint`.

This is the only visual direction. There is **no light mode, no skin switching, no layout variants.** Anything in the codebase that implements those is drift and is slated for removal (see `CODEBASE-CLEANUP.md`).

---

## 1. Principles

1. **One signal.** Emerald `#00d992` is the *only* chromatic accent. It marks the current — active borders, the brand bolt, the most important interactive moment on screen. Never a flat fill across a large area.
2. **Borders carry elevation, not shadows.** Weight (1px → 2px → 3px) communicates hierarchy. Two named shadows exist for floating surfaces only.
3. **Data is mono.** Every number, hash, IP, path, speed, ratio and ETA is set in `JetBrains Mono` with `tabular-nums`. Names and prose are `Inter`.
4. **Quiet by default.** Status lives in a colored dot + tinted progress bar, not in chrome or modals.
5. **Dense, but with air.** Compact 26px rows by default; sections breathe with 16–24px gutters.
6. **Linux-native.** KDE Plasma / Breeze conventions: server-side-decoration title bar (controls on the right), an application menu bar, Linux filesystem paths.

---

## 2. Color

All values are the locked dark palette. (oklch is fine to author in; the codegen path `scripts/oklch-to-srgb.py` already emits sRGB — just collapse it to this one table.)

### Backgrounds — near-black, layered carbon
| Token | Hex | Use |
|---|---|---|
| `--bg-0` | `#050507` | App canvas, table body |
| `--bg-1` | `#0a0a0c` | Title bar, menu bar, sidebar, status bar |
| `--bg-2` | `#101010` | Cards, inputs, panels, code blocks |
| `--bg-3` | `#16161a` | Table header, solid buttons, raised surfaces |
| `--bg-hover` | `#1b1b1f` | Row / control hover |
| `--bg-selected` | `#0a2419` | Selection wash (emerald-tinted) |
| `--bg-inset` | `#020203` | Recessed wells — progress tracks, graph grounds |

### Foreground — snow / parchment / steel
| Token | Hex | Use |
|---|---|---|
| `--fg-0` | `#f2f2f2` | Primary text |
| `--fg-1` | `#b8b3b0` | Secondary text |
| `--fg-2` | `#8b949e` | Tertiary / metadata / column headers |
| `--fg-3` | `#5a5d61` | Disabled, faint labels |

### Containment — warm charcoal
| Token | Hex | Use |
|---|---|---|
| `--border-1` | `#262320` | Standard 1px containment (dimmed) |
| `--border-2` | `#3d3a39` | Emphasis / hovered / active container (2px) |
| `--border-dashed` | `rgba(184,179,176,.22)` | Blueprint / workflow diagrams only |
| `--divider` | `#19181a` | Hairline separators inside panels |

### Signal — emerald
| Token | Hex | Use |
|---|---|---|
| `--accent` | `#00d992` | The current — active border, brand bolt, key CTA |
| `--accent-hover` | `#00ffaa` | Hover state of the above |
| `--accent-fg` | `#04140d` | Text/icon on an emerald fill |
| `--accent-bg-soft` | `#04261b` | Emerald-tinted soft surface |
| `--mint` | `#2fd6a1` | Softer green for button *text* on dark surfaces |

The signature animation — the **green pulse** — `drop-shadow(0 0 2px → 8px #00d992)` over ~3.2s on the brand bolt and high-signal elements. Disabled under `prefers-reduced-motion` and the "Emerald glow" tweak.

### Status — reserved for state, never decoration
| State | Token | Hex |
|---|---|---|
| Downloading | `--st-downloading` | `#4cb3d4` (teal) |
| Seeding / Complete | `--st-seeding` / `--st-complete` | `#00d992` (emerald) |
| Paused | `--st-paused` | `#8b949e` (steel) |
| Queued | `--st-queued` | `#ffba00` (amber) |
| Checking / Metadata | `--st-checking` / `--st-metadata` | `#b98dff` (violet) |
| Stalled | `--st-stalled` | `#6e7177` (dim slate) |
| Error | `--st-error` | `#fb565b` (coral) |

---

## 3. Type

| Token | Family | Use |
|---|---|---|
| `--font-ui` | `Inter` 400/500/600/700 | All UI text, names, prose |
| `--font-mono` | `JetBrains Mono` 400/500/600 → `SFMono`, Menlo, Consolas | **All data**: speeds, sizes, ratios, ETA, hashes, IPs, paths, piece counts |

Scale: `--fs-10 … --fs-32` (10/11/12/13/14/16/20/24/32). **Floors:** 11px for status bar / dense metadata, 12px for table cells. Base UI size 13px. `font-feature-settings: 'calt' 1, 'rlig' 1`. Mono always `font-variant-numeric: tabular-nums`.

Headings use compressed line-height (1.12) and slightly negative letter-spacing for a dense, technical feel. No serifs, no display faces.

---

## 4. Spacing, radius, density

- **Spacing** — 4px base. `--s-1..10` = 2/4/6/8/12/16/20/24/32/48. Tight inside components (6–8px), generous between sections (16–24px).
- **Radius** — restrained: `--r-sm 4` (inline/code) · `--r-md 6` (buttons, inputs) · `--r-lg 8` (cards, panels) · `--r-xl 10` (dialogs, palette) · `--r-pill 9999` (tags, badges, status dots). **Radius is locked** — no sharp/rounded toggle.
- **Density** — the one geometry tweak. `compact` (row 26 / chrome 38) is default; `balanced` (32/44) and `spacious` (40/52) remain as a runtime switch.

---

## 5. Motion

Slow, deliberate, mechanical. Entrances `cubic-bezier(0.16, 1, 0.3, 1)`, 200–420ms. Micro-interactions 120ms. **No bounce, no spring, no scale-on-hover, no tilt.** Hover = background/border shift only. Press = opacity 0.6–0.8. Marquees and the green pulse respect `prefers-reduced-motion`.

---

## 6. Component anatomy (atoms → organisms)

These are the building blocks the prototype ships (`components/primitives.jsx`) and that the Slint port mirrors (`ui/atoms`, `ui/molecules`).

- **Button** — height 26 (sm) / 28 (md). `primary` = emerald fill + `--accent-fg`; `solid` = `--bg-3` + `--border-1`; `ghost` = transparent → `--bg-hover`. Radius `--r-md`.
- **IconButton** — 28×28, transparent → `--bg-hover`. Icons inherit `currentColor`, default `--fg-1`.
- **Icon set** — custom 16px line glyphs, 1.5px stroke, round caps/joins (`components/primitives.jsx` → `Icon`). ~50 glyphs, no third-party icon font. (Lucide is the spiritual reference; IronTide ships its own to keep the Slint port asset-free.)
- **StatusDot** — 8px pill in the status color. Downloading/checking get an expanding halo (`it-pulse`, 1.8s).
- **ProgressBar** — track `--bg-inset` + `--border-1`, fill = status color, height 6px, radius half-height. Optional right-aligned mono % label.
- **Chip / Tag** — pill, `--bg-2` + `--border-1`, 11px. Optional leading status dot.
- **Toggle** — 30×18 track; on = `--accent`. White 14px knob.
- **TextInput / Select** — height 28, `--bg-2` + `--border-1`, radius `--r-md`. Mono variant for paths/numbers. Select has a trailing chevron.
- **Kbd** — mono 11px, `--bg-2` + `--border-1`, radius 3.
- **Card** — `--bg-2`, 1px `--border-1`, radius `--r-lg`, padding 16–24. Active card → 2px `--border-2` or emerald.
- **SectionLabel** — 10px uppercase, `--fg-3`, letter-spacing .08em.

---

## 7. Iconography

Custom line set only (see §6). 1.5px stroke; 16px inline, 20px controls, 24px nav/headers. Color inherits `currentColor` — default `--fg-1`, emerald `--accent` for active/accent moments. **No emoji. No unicode symbol icons.** The brand mark is the **emerald bolt** (`Chrome.BoltMark`) with the green pulse.

---

## 8. What was removed (and must stay removed)

- ❌ Light theme — every `*-light` palette.
- ❌ Skins — Forge (amber) and Tide (teal) are gone; the product *is* the former "Abyss" direction, refined.
- ❌ Layout variants L2 (drawer) and L3 (command workspace) — **L1 3-pane is the only layout.**
- ❌ Radius presets (sharp/balanced/rounded) and the platform-chrome / font pickers.

The only runtime knobs that survive: **Density**, **Sidebar mode** (full/icons/hidden), **Row striping**, **Emerald glow**.
