# IronTide — Design Documents

Two complementary design docs live here. They cover different concerns
and should never be treated as duplicates of each other.

## `DESIGN.md` — v1 GUI design spec (prescriptive)

Target-state visual, interaction, and IA specification for a
Slint-rendered IronTide GUI. v0.1, external design handoff. Covers:

- 6 design principles (power-user first, data density with air,
  keyboard-driven, original-not-derivative, Slint-honest, quiet by default)
- Design system: typography scale, three skins × light/dark in
  `oklch()` (Tide / Forge / Abyss), spacing grid, icon set, density scale
- Information architecture, three layout variants (3-pane / drawer /
  command workspace)
- Torrent list (12 default + 16 optional columns, row behaviour,
  virtualization), details pane (6 tabs), add-torrent flow, torrent
  creation tool
- Preferences (8 tabs), tool pages (RSS / search / scheduler / IP
  filter / logs / stats / Web UI / create)
- Keyboard shortcut map
- 5 novel features: `⌘K` command palette, smart category suggest,
  bandwidth intent, pair-to-phone QR, verify-before-download
- Slint implementation notes: direct widget mapping, custom components,
  suggested `.slint` module layout (atoms/molecules/organisms), tokens
  as globals, Rust↔Slint boundaries
- Recommended implementation order

**Status — aspirational.** Describes where the GUI should go, not
what it is. Adopting this spec is a planning decision: the token
vocabulary, column count, and module layout all diverge from current
code (`palette.slint`, 10-column table, flat layout). Reconcile with
`docs/plans/2026-04-11-irontide-roadmap-v5.md` before any rebuild work
starts. The spec's §19 also references `IronTide Design Spec.html`,
`IronTide Prototype.html`, `styles/tokens.css`, `components/*.jsx`,
and `screenshots/` — those are a future delivery; not in-repo yet.

## `design-system.md` — cross-surface system (descriptive)

Canonical reference for the **shared vocabulary** and **per-surface
style choices** across IronTide's four surfaces: Slint GUI, browser
Web UI, CLI / TUI, and the qBt v2 compat HTTP API. Covers:

- Shared vocabulary: `TorrentState` labels, `irontide-format`
  helpers (`format_size` / `format_rate` / `format_eta` /
  `format_ratio`), info hash rendering, 15-glyph peer flag legend
- Web UI: HTMX + Pico CSS tokens, component vocabulary, HTMX
  interaction patterns, accessibility, security
- GUI (Slint): palette tokens, custom widgets, component vocabulary,
  keyboard shortcuts, interaction patterns
- CLI + TUI: `clap` / `rustyline` / `ratatui` stack, output modes,
  shell completions
- qBt v2 compat surface: boundary + scope, spoofed fields, state
  mapping table, auth model

**Status — living.** Tracks live code. "When this doc drifts, the code
wins and this file should be updated."

## How the two docs relate

`design-system.md` is **descriptive** (what IronTide is today, across
every surface). `DESIGN.md` is **prescriptive** (what the GUI should
become after a ground-up visual redesign).

When a GUI rebuild eventually lands, the Slint section of
`design-system.md` will update to match the new reality. The shared
vocabulary (state labels, formatters, peer flag glyphs) is stable —
those are code contracts, not visual choices, and the rebuild does
not touch them.

## Reading order for someone new to the project

1. This file (orientation, under a minute).
2. `design-system.md` §§ "Shared vocabulary" and whichever surface
   you're touching.
3. `DESIGN.md` — only if you're planning or executing GUI rebuild work.
