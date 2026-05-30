# IronTide — DESIGN.md

> **Status:** v0.1 · Design specification for implementation
> **Stack:** Rust + Slint
> **Audience:** Engineering team, Claude Code
> **See also:** `IronTide Design Spec.html` (visual/interactive spec), `IronTide Prototype.html` (source of truth for layout/interactions), `styles/tokens.css` (tokens).

---

## 1. Product

IronTide is a cross-platform BitTorrent client with **feature parity to qBittorrent** delivered in an original visual language. It targets macOS, Windows, and Linux through a unified Slint-rendered GUI — no native-chrome mimicry. Audience: engineers and power users.

## 2. Design principles

1. **Power-user first** — every qBittorrent feature is reachable without hunting.
2. **Data density with air** — 32px balanced rows by default; compact (28px) and spacious (40px) one click away.
3. **Keyboard-driven** — every action has a shortcut; `⌘K` routes to any torrent, setting, or action.
4. **Original, not derivative** — feature parity ≠ UI parity.
5. **Slint-honest** — designs respect Slint's widget set; custom components are called out.
6. **Quiet by default** — status lives in color + sparkline, not modals.

## 3. Design system

### 3.1 Typography
| Token | Family | Use |
|---|---|---|
| `--font-ui` | Inter 400/500/600 | All UI text |
| `--font-mono` | JetBrains Mono 400/500 | Hashes, IPs, speeds, file paths |

Scale: `--fs-11` through `--fs-32` (11/12/13/14/16/18/22/28/32). Floors: 11px for status bar, 12px for dense tables; nothing smaller.

### 3.2 Color — three skins × light/dark

All color in `oklch()`. Shared status palette across skins:

| Status | oklch |
|---|---|
| downloading | `oklch(.70 .14 220)` |
| seeding | `oklch(.72 .15 150)` |
| paused | `oklch(.65 .02 250)` |
| queued | `oklch(.78 .14 85)` |
| checking | `oklch(.72 .12 280)` |
| error | `oklch(.65 .18 25)` |

Skins:
- **Tide** (default) — cool slate + teal. Accent `oklch(.60 .11 210)`.
- **Forge** — warm graphite + amber. Accent `oklch(.76 .13 70)`.
- **Abyss** — near-black + phosphor green, mono-heavy. Accent `oklch(.78 .18 150)`.

Full values in `styles/tokens.css`. Switch via `[data-skin="..."][data-theme="..."]` attributes on `<html>`.

### 3.3 Spacing & radius

4px base grid. Scale `s-1` (2) through `s-10` (48). Three radius presets:
- **sharp** — 0 everywhere
- **balanced** — 3/6/10/14
- **rounded** — 4/8/12/16 (default)

### 3.4 Icons

Custom 16×16 line set, 1.5px stroke, round caps/joins. ~40 glyphs. No third-party icon library.

### 3.5 Density

Row height: compact 28 · balanced 32 · spacious 40. Affects table rows, sidebar items, toolbar height.

---

## 4. Information architecture

**Sidebar sections:**
- Library: All, Downloading, Seeding, Completed, Paused, Active, Inactive, Errored
- Categories (user-defined, single-value)
- Tags (multi-value, free-form)
- Trackers (auto-aggregated by status: Working / Unreachable / Error)
- Tools: Search, RSS, Scheduler, Stats, Logs, IP Filter, Create, Web UI

**Modals:** Add torrent · Create torrent · Command palette · Preferences (8 tabs).

## 5. Layout variants (user-switchable at runtime)

- **L1 — 3-pane (default):** sidebar | torrent list (top) | details (bottom).
- **L2 — Inspector drawer:** sidebar | torrent list (full) | 460px right drawer.
- **L3 — Command workspace:** minimal chrome; sidebar can collapse to icons or hide; details on-demand (`Enter` or `⌘I`).

All three share toolbar, sidebar, and status bar.

---

## 6. Torrent list

### 6.1 Columns (all reorderable, resizable, sortable, toggleable)

Default visible: Select, status dot, Name, Size, Progress, Status, Seeds, Peers, Down, Up, Ratio, ETA.

Available but hidden by default: Category, Tags, Added, Completed, Tracker, Save path, Availability, Last activity, Time active, Private, Comment, Ratio limit, Content layout, Content path, Info hash v1, Info hash v2.

### 6.2 Row behavior

- Click to select; ⌘/⇧ for multi-select.
- Double-click → configurable action (open folder / open details / resume-pause).
- Right-click → full context menu (resume, pause, force recheck, move, rename, set location, trackers, tags, category, queue ↑/↓/top/bottom, remove, remove with data, copy name/hash/magnet/URL).
- Virtualized — render only visible rows.

## 7. Details pane (6 tabs)

1. **General** — progress, speeds, ratio, ETA, availability, pieces info; save path, hashes (v1/v2), dates, comment, privacy flag; error card when applicable; 48×N piece-availability heatmap.
2. **Trackers** — DHT/PeX/LSD pseudo-trackers first; user trackers with status, peer/seed/leech counts, last-announce, message.
3. **Peers** — IP/port, connection type (BT/µ), protocol flags + legend, client/version, progress bar, rates, relevance.
4. **HTTP Sources** — BEP-19 web seeds.
5. **Content** — recursive file tree with per-file progress + priority (High/Normal/Low/Skip). Toolbar: Sequential, First/last pieces first.
6. **Speed** — 60s/5m/1h/1d speed graph, per-torrent rate overrides.

## 8. Add-torrent flow

Modal. Three source tabs: **File**, **Magnet**, **URL**. Shared preview card (name, size, pieces, file count, comment, hash) + options panel:

- Save path (with quick-browse)
- Category + Tags
- Start paused / Skip hash check / Pre-allocate / Sequential / First-last / Alt limits / Auto-manage
- Per-file tree with priority dropdown

Magnet flow accepts multiple magnets (one per line, batched). URL flow fetches in-app.

## 9. Torrent creation tool

Standalone page (also a modal). Fields:
- Source file/folder
- Tracker list (blank line = tier break)
- Web seeds (HTTP/URL list)
- Piece size: Auto / 16 KiB → 16 MiB
- Format: v1 / v2 / Hybrid (BEP-52)
- Privacy flag, Optimize alignment, Source tag, Comment
- Live hashing-time estimate

## 10. Preferences — 8 tabs

### 10.1 Behavior
Theme, Skin, Density, Language, delete confirm, pause-all confirm, double-click action, shortcut set (Default/Emacs/Vim/qBittorrent-parity). Startup: login-start, start-minimized, minimize-to-tray-on-close, resume-previous-session. Notifications: on-complete / on-error / on-RSS-match, sound, external program with `%N %F %D %I` substitution.

### 10.2 Downloads
Default save path, separate incomplete folder, `.!it` extension, auto-categories, add-torrent dialog, start-paused, skip-hash, pre-allocate, append-date, smart category suggestion. Watched folder, .torrent copy/move paths, delete-on-add. On-complete: move-to folder, external program.

### 10.3 Connection
Incoming port, random-on-start, UPnP/NAT-PMP, port-status badge. Connection limits: global + per-torrent + upload slots + active dl/ul/total. Proxy: None / HTTP / SOCKS4/5, hostname lookups via proxy. IP filter: enable + path + refresh-on-startup.

### 10.4 Speed
Global DL/UL limits with toggles. Alternative limits + scheduled hours (preset + custom). Transport-overhead / µTP / LAN-exempt toggles.

### 10.5 BitTorrent
DHT, PeX, LSD, encryption (Prefer/Require/Disable), anonymous mode. Seeding limits (ratio / seed time / inactive time → Pause / Remove / Remove+data / Super-seed). Queueing toggle + slow thresholds.

### 10.6 RSS
Enable, refresh interval, max articles. Auto-download on/off, smart episode filter, smart-filter regex, repack handling.

### 10.7 Web UI
Enable, bind address + port, HTTPS (cert/key paths). Auth: username, password, bypass-on-localhost, session timeout, login-attempts ban. Security: clickjacking/CSRF/host-header, allowed hosts, reverse-proxy X-Forwarded-For trust list. Dynamic DNS (DynDNS / No-IP / DuckDNS / Cloudflare).

### 10.8 Advanced
libtorrent tuning (async-I/O threads, hashing threads, file-pool size, outstanding-memory-checking, disk cache, cache expiry, coalesce R/W, piece-extent affinity, socket buffers). Network: interface selection, bind IP, reported-IP mode, update-check, resolve peer countries/hostnames. Behavior: strict super-seeding, announce-tier rules, µTP-TCP mixed mode, always-announce, resume-data interval. Export config (reveal `config.toml`), reset to defaults.

---

## 11. Tool pages

### RSS reader + auto-download
Two-pane: feeds sidebar (unread counts + last-updated) + items list + docked rules panel. Each rule: enable, name, must-contain, must-not-contain, episode filter, smart filter, category, save path, applicable feeds.

### Search plugins
Query + plugin filter + category filter. Results table: name, size, seeds, peers, engine, date, Add button → routes to Add-torrent dialog pre-filled. Plugin manager: enable/disable per plugin, install from URL, update all.

### Bandwidth scheduler
7×24 grid (days × hours). Cell states: **Full speed** · **Alternative limits** · **Paused**. Click to toggle, drag to paint. Presets: Work hours, Overnight, Weekends.

### IP filter
Stats strip (enabled, total ranges, blocked today, last refresh) + editable range table (range, level = block/allow, comment). Banned-peers-this-session list with reasons. Import from URL, .p2p/.dat format support.

### Logs
Monospace. Level filters (INFO/WARN/ERROR/DEBUG), tail mode, clear, export. Rows: timestamp (ms), level pill, message. Auto-scroll pauses on user interaction.

### Statistics
10 stat cards (all-time DL/UL/ratio/shared-time, session DL/UL/uptime, global peers, DHT nodes, active/max connections). 90-day rolling transfer graph. Top peer countries bar chart. Cache & I/O table (read-cache hit rate, write queue, disk R/W latencies, piece DL time).

### Web UI (remote access page)
Live endpoint info (local/LAN/external URLs, status, uptime, active sessions). Active sessions table with revoke. Pair-a-device panel with QR (URL + user + one-time token). All settings in Preferences → Web UI.

### Create torrent
See §9.

## 12. Queue management

Toggleable in Preferences → BitTorrent. Caps: max active DL / UL / total. Slow-torrent exemption (configurable KB/s thresholds). Selected rows expose queue ↑/↓ chevrons; `⌘↑` / `⌘↓`.

## 13. Categories, tags, save-path rules

Categories: single-valued, drive default save paths. Tags: multi-valued, free-form. Both appear in sidebar with counts and drive filter predicate. Save-path rules support token expansion:
- `{category}`, `{tracker}`, `{yyyy}/{mm}`, `{content_type}`

## 14. Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `⌘K` | Command palette |
| `⌘,` | Preferences |
| `⌘N` | Add torrent |
| `⌘⇧N` | Add magnet |
| `Space` | Pause/Resume selected |
| `Delete` / `⌫` | Remove (with confirm) |
| `⌘⇧F` | Force recheck |
| `⌘↑` / `⌘↓` | Queue up/down |
| `⌘I` | Toggle inspector |
| `⌘L` | Focus torrent list |
| `/` | Filter torrent list |
| `⌘1…9` | Jump to sidebar section |
| `Enter` | Open details for selection (L3) |
| `Esc` | Close modal / clear selection |

---

## 15. Novel features (non-qBittorrent)

1. **Command palette (⌘K)** — fuzzy-matched, grouped; every action, setting, and torrent reachable by typing.
2. **Smart category suggestion** — local classifier (no network) inspects file names + sizes → suggests category + save path. User-trainable via right-click → "Train as: X".
3. **Bandwidth intent** — goals like "leave 10 Mbps for calls," "≤80% of measured uplink," "pause during Zoom/Meet." Probes uplink every few minutes, auto-adjusts caps.
4. **Pair-to-phone QR** — Web UI → Pair-a-device shows a QR encoding URL + username + one-time token. Tokens rotate and revoke from the sessions table.
5. **Verify-before-download** — when files already exist at the save path, offer a local hash pass before any network traffic. Common re-add case; saves enormous bandwidth.

---

## 16. Slint implementation

### 16.1 Direct Slint mapping

| Element | Slint |
|---|---|
| Window, Menu bar | `Window { MenuBar { ... } }` (1.8+) |
| Buttons, switches | `Button`, `Switch` |
| Text inputs | `LineEdit`, `TextEdit` |
| Combo | `ComboBox` |
| Scroll views | `ScrollView`, `Flickable` |
| Tabs | `TabWidget` (restyled) |
| Tooltips | `Tooltip` (1.7+) |

### 16.2 Custom components required

| Component | Why custom | Approach |
|---|---|---|
| `TorrentTable` | Virtualized, multi-select, reorderable columns, density variants | `StandardTableView` base + custom row delegate; fixed item-height `ListView` virtualization; column-drag via custom header |
| `ProgressBar` (tinted) | Status-tinted per row | Custom with `value` + `tone` props mapping to color tokens |
| `Sparkline` / `SpeedGraph` | No built-in chart | Draw with `Path`; feed a ring-buffer property |
| `PieceAvailabilityMap` | Dense grid, potentially thousands of pieces | Custom image rendered in Rust via `tiny_skia`, passed as `Image` |
| `BandwidthScheduler` | 7×24 paintable grid | Custom; ~168 rectangles + click-drag state |
| `CommandPalette` | Fuzzy-matched, grouped, keyboard-driven | `PopupWindow` + `LineEdit` + virtualized `ListView` |
| `StatusDot` (pulse) | Animated halo | `animate` on child `Rectangle`'s `opacity` + `scale` |
| `Toast` | Stacking notifications | Bottom-right container + `TimerTask` dismiss |
| `QRCode` | — | Generate in Rust (`qrcode` crate) → `Image` |
| Country flags | — | Spritesheet + `Image { source-clip-*: }` |

### 16.3 Suggested `.slint` module layout

```
src/ui/
  tokens.slint                 // color, type, spacing, radius globals
  atoms/
    button.slint
    icon_button.slint
    chip.slint
    progress_bar.slint
    status_dot.slint
    status_pill.slint
    toggle.slint
    text_input.slint
    select.slint
  molecules/
    sidebar_item.slint
    torrent_row.slint
    tab_strip.slint
    speed_graph.slint
    piece_map.slint
    scheduler_grid.slint
  organisms/
    window_chrome.slint
    menu_bar.slint
    toolbar.slint
    sidebar.slint
    torrent_table.slint
    details_pane.slint
    prefs_dialog.slint
    add_torrent_dialog.slint
    create_torrent_dialog.slint
    command_palette.slint
    rss_page.slint
    search_page.slint
    scheduler_page.slint
    stats_page.slint
    logs_page.slint
    ipfilter_page.slint
    webui_page.slint
  app.slint                     // top-level window, routing
```

### 16.4 Tokens as Slint globals

```slint
export global Tokens := {
    // color
    in-out property <color> bg-0: #2c313d;
    in-out property <color> bg-1: #333844;
    in-out property <color> fg-0: #f4f5f7;
    in-out property <color> accent: #5ba9d6;
    in-out property <color> st-downloading: #5ea6e8;
    in-out property <color> st-seeding: #6ebf8e;
    in-out property <color> st-error: #e0694f;
    // spacing
    in-out property <length> s-4: 8px;
    in-out property <length> s-6: 16px;
    in-out property <length> row-h: 32px;
    // radius
    in-out property <length> r-md: 6px;
    in-out property <length> r-lg: 10px;
}
```

Skin/theme/density switching rewrites all tokens in one atomic batch from Rust to avoid flicker.

### 16.5 Rust ↔ Slint boundaries

- `TorrentModel` → virtualized torrent table. Diff against previous snapshot, emit `row_changed(N)` per affected row only.
- `PeerModel`, `TrackerModel`, `FileTreeModel` → scoped to selected torrent. Swap wholesale on selection change.
- `LogModel` → ring buffer, ~10k entries. Level filter Slint-side via `FilterModel`.
- `Stats`, `SpeedSample` → root `in-out property`, polled by 1 Hz timer.
- Selection / filter / category / nav / modal state → root properties.

### 16.6 Callouts

- **Virtualization is mandatory.** With 1000+ torrents, non-virtualized lists stutter. Keep row delegates lightweight — no per-row sparklines.
- **Theme switching at runtime.** Update every token in one batch.
- **Accessibility.** `accessible-role` + `accessible-label` on every interactive element. Torrent rows: `accessible-role: list-item`.

---

## 17. Defaults (shipping)

| Knob | Default |
|---|---|
| Skin | Tide |
| Theme | Dark |
| Density | Compact |
| Sidebar | Full |
| Layout | L1 (3-pane) |
| Radius | Rounded |
| Row striping | On |
| UI font | Inter |

## 18. Recommended implementation order

1. Port `tokens.css` → `tokens.slint`. Wire skin/theme switching callback.
2. Atoms (button, icon_button, chip, progress_bar, status_dot, toggle, text_input, select).
3. Window chrome + menu bar + toolbar + sidebar. Wire routing.
4. `TorrentTable` (virtualized) + status columns. Hook up `TorrentModel`.
5. Details pane with all 6 tabs.
6. Preferences dialog (8 tabs).
7. Tool pages: Search → RSS → Scheduler → Stats → Logs → IP Filter → Create → Web UI.
8. Command palette + Add/Create modals.
9. Novel features (smart suggest, bandwidth intent, verify-before-download, QR pair).
10. Accessibility sweep.

---

## 19. References in this bundle

- `IronTide Prototype.html` — source of truth for layout, spacing, interactions.
- `IronTide Design Spec.html` — visual spec with swatches and diagrams.
- `styles/tokens.css` — all tokens, all skins.
- `components/*.jsx` — reference layout logic (do not copy verbatim).
- `screenshots/` — 7 captures covering main variations.

---

*IronTide · Design specification · v0.1 · April 2026*
