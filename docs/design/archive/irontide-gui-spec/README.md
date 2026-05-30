# Handoff: IronTide GUI

## Overview
IronTide is a cross-platform BitTorrent client written in **Rust** with a **Slint**-rendered GUI. This handoff covers the full GUI: feature parity with qBittorrent across all menus, settings, and workflows, in an original visual language with three skins (Tide/Forge/Abyss), three layout variants, light/dark themes, three density levels, and five novel capabilities (command palette, smart category suggestion, bandwidth intent, pair-to-phone QR, verify-before-download).

## About the Design Files
The files in this bundle are **design references created in HTML/React** — a working prototype that shows intended look, layout, and behavior. They are **not** production code to copy verbatim.

Your task is to **recreate these designs in the target project's real stack** (Rust + Slint, per the design spec). Use Slint's native widget set wherever possible; implement the handful of custom components listed in §21 of the design spec. Consume the design tokens from `styles/tokens.css` as a Slint `global Tokens` — don't hand-translate values per-component.

Open `IronTide Prototype.html` and click around. Toggle Tweaks (in the toolbar above the iframe) to see all skins, themes, densities, radius presets, and layout variants.

## Fidelity
**High-fidelity.** Final colors (all `oklch()`), typography (Inter + JetBrains Mono), spacing (4px grid), radii, and interaction patterns are all specified. Recreate pixel-perfectly in Slint — lift values from `styles/tokens.css` and the details in `IronTide Design Spec.html`.

## Primary references

- **`IronTide Design Spec.html`** — read this first. 21 sections covering principles, design system, IA, all 8 preferences tabs, every tool page, keyboard shortcuts, the 5 novel features, and detailed **Slint implementation notes** (widget mapping, custom component list, suggested `.slint` module layout, token-globals example, Rust model boundaries, virtualization + a11y callouts).
- **`IronTide Prototype.html`** — source of truth for layout, spacing, interactions.
- **`styles/tokens.css`** — full token definitions across 3 skins × 2 themes × 3 densities × 3 radius presets. Port directly to Slint globals (see spec §21).
- **`components/*.jsx`** — per-screen component source. Read these for exact layout logic; don't copy the JSX.

## Screens / Views

All views are in `IronTide Prototype.html`; the sidebar and Tools submenu switch between them.

### Library (torrents)
Sidebar filters (All, Downloading, Seeding, Completed, Paused, Active, Inactive, Errored) + Categories + Tags + Trackers (auto-aggregated). Torrent list with virtualized rows. Default columns: Select, status dot, Name, Size, Progress, Status, Seeds, Peers, Down, Up, Ratio, ETA. All columns reorderable, resizable, sortable, toggleable. Full column list in spec §5.

### Per-torrent details (6 tabs)
General, Trackers, Peers, HTTP Sources, Content (file tree with priorities), Speed. See spec §6.

### Add Torrent dialog
Three source tabs (File / Magnet / URL) with shared preview card and options panel. Spec §7.

### Create Torrent tool
Full-page. Source, trackers, web seeds, piece size, format (v1/v2/Hybrid BEP-52), privacy. Spec §8.

### Preferences dialog (8 tabs)
Behavior, Downloads, Connection, Speed, BitTorrent, RSS, Web UI, Advanced. Every setting from qBittorrent is present. Spec §9.

### RSS, Search, Scheduler, IP Filter, Logs, Stats, Web UI
Each is a full tool page. Spec §§10–16.

### Command palette (⌘K)
Fuzzy-matched global navigation and action launcher. Spec §20 #1.

## Interactions & Behavior

- Navigation: sidebar `nav` items switch the main view; filter/category clicks reset the torrent list predicate.
- Multi-select in the list (⌘ / ⇧), keyboard navigation, Enter opens details in L3.
- Double-click row → opens Content folder (configurable in Prefs → Behavior).
- Modals: Add (⌘N), Create, Command palette (⌘K), Preferences (⌘,). Close with Esc.
- Tweaks panel: runtime skin/theme/density/layout/radius switcher. Persist to localStorage in the prototype; persist to a config file in the real app.
- Animations: status-dot pulse for active torrents (animate `opacity` + `scale` in Slint). Toast slide-in for notifications. No other ambient motion.

Full shortcut list: spec §19.

## State management

Expose these from Rust as Slint models (spec §21, "Rust-side boundaries"):

- `TorrentModel` — backs the virtualized torrent table. Emit `model_changed` per affected row only.
- `PeerModel`, `TrackerModel`, `FileTreeModel` — scoped to the selected torrent; swap wholesale on selection change.
- `LogModel` — ring buffer (~10k entries). Level filter client-side via `FilterModel`.
- `Stats`, `SpeedSample` — `in-out property` on root, polled at 1 Hz.
- Selection, filter, category, nav, modal state — root properties.

## Design tokens

All tokens live in `styles/tokens.css`. Port to a Slint `global Tokens` singleton (see spec §21 for example syntax). Three skins (Tide / Forge / Abyss), each with light + dark. Spacing, radius, shadow, type, and density scales are shared.

## Novel features to implement

1. **Command palette** (⌘K) — custom Slint `PopupWindow` + virtualized `ListView`.
2. **Smart category suggestion** — local classifier (file names + sizes → category + save path). No network. User-trainable via right-click "Train as: X".
3. **Bandwidth intent** — goals like "leave 10 Mbps for calls," "≤80% of uplink," "pause during Zoom/Meet." Probes bandwidth on a timer and auto-adjusts.
4. **Pair-to-phone QR** — encodes URL + username + one-time token. Generate with `qrcode` crate, pass as Slint `Image`.
5. **Verify-before-download** — fast local hash pass when files already exist in the save path, before any network traffic.

## Custom Slint components required

See spec §21 for the full list with approach notes:

TorrentTable (virtualized), tinted ProgressBar, Sparkline/SpeedGraph, PieceAvailabilityMap, BandwidthScheduler (7×24 grid), CommandPalette, StatusDot (animated pulse), Toast stack, QR code, flag spritesheet.

## Platform

Target platforms: macOS, Windows, Linux. Slint renders its own widgets — we get a unified appearance across all three, which is intentional. Do **not** mimic the host OS's native window chrome; IronTide provides its own.

## Files in this bundle

- `IronTide Design Spec.html` — comprehensive engineering handoff. Read first.
- `IronTide Prototype.html` — interactive prototype. Source of truth.
- `components/*.jsx` — per-screen layout logic (for reference, not for copying).
- `styles/tokens.css` — all design tokens. Port to Slint globals.

## Recommended implementation order

1. Port `tokens.css` to `tokens.slint` (all 3 skins × 2 themes × scale). Wire skin-switching callback.
2. Build atom components (button, icon_button, chip, progress_bar, status_dot, toggle, text_input, select).
3. Build window chrome + menu bar + toolbar + sidebar. Wire routing.
4. Build TorrentTable (virtualized) + status columns. Hook up `TorrentModel`.
5. Build details pane with all 6 tabs.
6. Build Preferences dialog (8 tabs).
7. Tools pages: Search → RSS → Scheduler → Stats → Logs → IP Filter → Create → Web UI.
8. Command palette + Add/Create modals.
9. Novel features (smart suggest, bandwidth intent, verify-before-download, QR pair).
10. A11y sweep — every interactive element gets `accessible-role` + `accessible-label`.
