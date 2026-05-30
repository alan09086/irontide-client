# IronTide — GUI Specification (v1.0, FOCUSED)

> **Stack:** Rust + Slint · **Target:** Linux (KDE Plasma primary), Windows, macOS
> **Visual source of truth:** `IronTide.html` (this project) + `DESIGN-SYSTEM.md`
> **Codebase actions:** `CODEBASE-CLEANUP.md`
> **Audience:** Claude Code + the engineering team

IronTide is a BitTorrent client with **feature parity to qBittorrent**, presented in the IronTide visual language (emerald-dark, KDE-native). This spec describes one window, one theme, one layout — every surface detailed so the Slint port is unambiguous.

This replaces the v0.1 spec. Where v0.1 said *"feature parity ≠ UI parity,"* v1.0 says: **match qBittorrent's information architecture and layout exactly, reskinned in the IronTide system.** A qBittorrent user should feel at home; the pixels should feel like nothing else.

---

## 1. Window chrome (KDE / Breeze)

Three stacked bars, full width, each with a 1px `--border-1` bottom edge.

1. **Title bar** (34px, `--bg-1`) — *server-side decoration look.* Left: emerald **bolt mark** + `IronTide` wordmark. Center: window title (Breeze centers it). Right: window controls **minimize · maximize · close** — flat Breeze buttons, 26px, symbol shows on hover; close tints coral `--st-error`. (On real KDE this is drawn by the WM; we render it so the mock reads native and so CSD platforms match.)
2. **Menu bar** (28px, `--bg-1`) — `File · Edit · View · Tools · Help`. Flat Breeze hover (`--bg-hover`); click opens a dropdown (`--bg-2`, 1px `--border-1`, `--shadow-lg`) with mono right-aligned shortcuts, checkmarks for toggles, emerald dots for radio groups, and `›` submenus. **Full map in §1a.**
3. **Toolbar** (`--chrome-h`, `--bg-0`) — see §2.

Bottom of window: **status bar** (§7).

---

## 1a. Menu bar — full map

Every menu, item, shortcut, and submenu. `▸` = submenu, `[✓]` = toggle (checkmark), `(•)` = radio group (one emerald dot). Action keys in `code` are the dispatch keys wired in `components/chrome.jsx` → `app.jsx onMenu`.

### File
| Item | Shortcut | Action |
|---|---|---|
| Add Torrent File… | `Ctrl+O` | Add dialog (File tab) |
| Add Torrent Link / Magnet… | `Ctrl+Shift+O` | Add dialog (Magnet tab) |
| Create New Torrent… | `Ctrl+N` | Create dialog |
| — | | |
| Export .torrent… | | Save selected torrent's metainfo |
| Export Torrent + Data… | | Save metainfo + payload |
| — | | |
| Exit IronTide | `Ctrl+Q` | Quit |

### Edit (operates on the current selection)
| Item | Shortcut |
|---|---|
| Resume | `Space` |
| Pause | |
| Force Resume | |
| — | |
| Force Recheck | `Ctrl+Shift+F` |
| Force Reannounce | |
| — | |
| Set Location… | |
| Rename… | `F2` |
| Category ▸ | Linux ISOs · Software · Uncategorized · — · New category… |
| Queue ▸ | Move to Top `Ctrl+Shift+Up` · Move Up `Ctrl+Up` · Move Down `Ctrl+Down` · Move to Bottom `Ctrl+Shift+Down` |
| — | |
| Remove | `Del` |
| Remove + Delete Files… | `Shift+Del` |
| — | |
| Select All | `Ctrl+A` |

### View
| Item | Shortcut | Notes |
|---|---|---|
| Command Palette… | `Ctrl+K` | |
| — | | |
| Sidebar ▸ | | (•) Full · Icons only · Hidden |
| Details Panel | `Ctrl+I` | [✓] toggle — collapses bottom pane |
| — | | |
| Density ▸ | | (•) Compact · Balanced · Spacious |
| Row Striping | | [✓] toggle |
| Emerald Glow | | [✓] toggle — brand-pulse on/off |

### Tools
| Item | Shortcut | Action |
|---|---|---|
| Search | `Ctrl+F` | Search page |
| RSS Reader | | RSS page |
| Bandwidth Scheduler… | | Scheduler page |
| — | | |
| Statistics… | | Stats page |
| Logs… | | Logs page |
| IP Filter… | | IP-filter page |
| — | | |
| Web UI / Remote… | | Web UI + pairing page |
| — | | |
| Preferences… | `Ctrl+,` | Preferences modal |

### Help
| Item | Shortcut |
|---|---|
| About IronTide | |
| Documentation | `F1` |
| Keyboard Shortcuts | |
| Check for Updates… | |
| — | |
| Report a Bug… | |

> **Slint note — build a CUSTOM menu, not the native `MenuBar` widget.** Slint's built-in `MenuBar`/`Menu`/`MenuItem` renders **native OS menus** that ignore our color tokens and will *not* look like this design. Build styled `PopupWindow`-based menus instead (full recipe + acceptance criteria in `CODEBASE-CLEANUP.md §7a`). The intended look is captured statically in **`Menu Reference.html`** — match that. Toggles → check-marked rows bound to tweak state; radio groups → emerald-dot rows bound to the active token; submenus → nested popups. Accelerators are registered in `accel.rs` independently of the menu.

---

## 2. Toolbar

Left → right:
- **Add** (emerald primary) → Add-torrent dialog. **Magnet** (solid) → Add dialog on the Magnet tab.
- divider · **Resume · Pause** (icon) · **Queue ↑ · Queue ↓** (icon) · **Remove** (icon).
- divider · **Command bar** — a wide `--bg-2` field reading "Jump to torrent, action, or setting…" with a `Ctrl+K` Kbd hint (leading search-glyph icon, not a ⌘ mark). Opens the Command Palette (§9).
- flex spacer.
- **Transfer readout** (mono): `⇣ 13.7 MB/s   ⇡ 3.2 MB/s   · 128 peers`. Down icon tinted `--st-downloading`, up icon `--st-seeding`.
- divider · **Preferences** (gear) → §8.

No layout switcher, no theme toggle (both removed).

---

## 3. Sidebar (left, `--sidebar-w` = 224px, `--bg-1`)

Scrollable filter tree, qBittorrent-style, with three runtime modes (`full` / `icons` / `hidden`).

- **LIBRARY** — All torrents, Downloading, Seeding, Completed, Paused, Active, Inactive, Errored. Each row: status-tint icon + label + right-aligned count. Selected row = `--bg-selected` + 2px emerald left rail.
- **CATEGORIES** — user-defined, single-value, drive default save paths. `All`, then each category with count; `Uncategorized` last. `+` affordance in the section header.
- **TAGS** — multi-value, free-form, each with count.
- **TRACKERS** — auto-aggregated: Working / Unreachable / Error, then per-host with counts.

Section headers use **SectionLabel** (10px uppercase `--fg-3`).

---

## 4. Torrent list (top pane)

Virtualized table. Compact 26px rows, optional zebra striping.

### Columns (reorderable · resizable · sortable · toggleable)
Default visible, in order: **select checkbox · status dot · Name · Size · Progress · Status · Seeds · Peers · Down · Up · Ratio · ETA**.

- **Name** — `Inter`, flex-grow, ellipsis. Leading status dot in the row's status color.
- **Size / Down / Up / Ratio / Seeds / Peers / ETA** — mono, right-aligned, tabular. Seeds/Peers show `connected (swarm)` e.g. `812 (12,403)`; width 94px, never wraps. Down/Up tint to status color when non-zero, else `--fg-2`.
- **Progress** — tinted `ProgressBar` + mono `%`.
- **Status** — capitalized label, color matches dot.

Hidden-by-default columns (available via header context menu): Category, Tags, Added, Completed, Tracker, Save path, Availability, Last activity, Time active, Private, Comment, Ratio limit, Content layout, Content path, Info hash v1/v2.

### Header
`--bg-3`, 12px uppercase `--fg-2`, active sort shows a caret. Right-click header → column toggle menu.

### Row behavior
Click selects; `Ctrl`/`Shift`-click multi-select; selected = `--bg-selected`. Double-click = configurable (open folder / details / resume-pause). Right-click = full context menu (the complete tree is in §6a).

### Sub-footer (between list and details)
`12 torrents · 3 active` left; mono transfer + DHT node count right.

---

## 5. Details pane (bottom, docked — L1)

Tab strip (active tab = emerald underline) over a scroll area. Six tabs:

1. **General** — Transfer card (name, status dot+label, progress, size, dl/ul, ratio, ETA, availability, pieces). Information card (save path, hashes v1/v2, added/completed dates, comment, private flag). Error card (coral) when applicable. A **piece-availability heatmap** (48×N grid; render in Rust via `tiny_skia` → `Image`).
2. **Trackers** — DHT / PeX / LSD pseudo-rows first (`** [DHT] **` etc.), then user trackers: URL (mono) · status dot+label · seeds · leech · downloaded · message. Tier grouping respected.
3. **Peers** — IP:port (mono) · country flag · connection type (BT / µTP) · protocol flag string + legend · client/version · progress bar · down · up · relevance. Down tints `--st-downloading`, up `--st-seeding`.
4. **HTTP Sources** — BEP-19 web seeds: status dot · URL (mono) · state chip · remove.
5. **Content** — recursive file tree: name · size (mono) · per-file progress bar + % · priority (High / Normal / Low / Skip) via dropdown. Toolbar: Sequential download, First/last pieces first.
6. **Speed** — per-torrent speed graph (60s / 5m / 1h / 1d) drawn with `Path` over a ring buffer; per-torrent rate overrides.

---

## 6. Dialogs

**Add torrent** — three source tabs (File / Magnet / URL). Shared preview card (name, size, pieces, file count, comment, hash) + options: save path (quick-browse), category + tags, start-paused / skip-hash / pre-allocate / download-sequentially / first-last-pieces / alt-limits / automatic-torrent-management, and the per-file priority tree. Magnet accepts multiple magnets (one per line, batched). URL fetches in-app.

**Create torrent** — source file/folder · tracker list (blank line = tier break) · web seeds · piece size (Auto / 16 KiB→16 MiB) · format v1 / v2 / Hybrid (BEP-52) · private flag · optimize alignment · source tag · comment · live hashing-time estimate.

**Confirm-delete** — name(s), a "also delete files from disk" toggle (coral when on), Cancel / Remove.

---

## 6a. Context menus (right-click)

All six render with the same styling as the menu bar (`--bg-2` panel, emerald checks/radios, mono accelerators, `›` submenus). Visual: **`Menu Reference.html` → "Context menus"**. `[✓]` toggle, `(•)` radio, `▸` submenu.

### Torrent row (right-click a selection)
Resume `Space` · Pause · Force Resume · — · Force Recheck `Ctrl+Shift+F` · Force Reannounce · — · Open Destination Folder · Open Details · — · **Queue ▸** (Move to Top `Ctrl+Shift+Up` / Up `Ctrl+Up` / Down `Ctrl+Down` / Bottom `Ctrl+Shift+Down`) · **Category ▸** ((•) Reset / Linux ISOs / Software / — / New category…) · **Tags ▸** ([✓] verified / distro / archival / — / Add tag…) · — · Set Location… · Rename… `F2` · Set Download Limit… · Set Upload Limit… · Set Share Limit… · — · [✓] Automatic Torrent Management · [✓] Super Seeding · [✓] Download Sequentially · [✓] Download First/Last Pieces First · — · **Copy ▸** (Name / Info hash v1 / Info hash v2 / Magnet link) · Export .torrent… · — · Remove `Del` · Remove + Delete Files… `Shift+Del`

### Column header (right-click the table header)
Checkable column list — default on: Name · Size · Progress · Status · Seeds · Peers · Down · Up · Ratio · ETA. Default off: Category · Tags · Added On · Tracker · Save Path · Availability (plus the remaining hidden columns from §4). — · Reset to Defaults · Auto-fit Columns · [✓] Lock Columns.

### Sidebar — Category
Add Category… · Edit Category… · Remove Category · Remove Unused Categories · — · Resume Torrents · Pause Torrents · — · Set Default Save Path…

### Sidebar — Tag
Add Tag… · Remove Tag · Remove Unused Tags · — · Resume Torrents · Pause Torrents

### Sidebar — Tracker
Copy Tracker URL · Edit Trackers… · — · Resume Torrents · Pause Torrents · Remove Torrents

### System tray (right-click the tray icon)
Show / Hide IronTide · — · Add Torrent File… · Add Torrent Link… · — · Pause All · Resume All · [✓] Alternative Speed Limits · — · Preferences… · — · Quit IronTide

> Build these as the **same custom popup component** as the menu bar (`CODEBASE-CLEANUP.md §7a`), opened at the cursor on right-click. The tray menu uses the platform tray API (`tray.rs`) — native tray menus are acceptable there since the OS owns the tray.

---

## 7. Status bar (bottom, `--bg-1`, 11px)

Left: connection status dot + `Connected` / DHT node count. Center-right: free-disk meter (`1.2 TB free · 68%` with a thin bar). Right: global `⇣ / ⇡` mono readout, alt-limits indicator, external-IP/port badge.

---

## 8. Preferences (modal, 8 tabs)

Left rail of tabs, content right, `Cancel / Apply / OK` footer. Control types are explicit: `[toggle]` switch · `[select: a/b]` dropdown · `[number]` stepper · `[text]` field · `[path]` field + Browse… · `[radio]` group. Groups are `SectionLabel` headers (10px uppercase `--fg-3`). Each row: label left (`--fg-1`), control right; optional `hint` line beneath in `--fg-2`.

### 8.1 Behavior
- **Interface** — Density `[select: Compact/Balanced/Spacious]` · Language `[select]`. *(No Theme, no Skin — removed.)*
- **Confirmations** — Confirm before deleting `[toggle]` · Confirm pause-all / resume-all `[toggle]` · Show splash screen `[toggle]` · Show torrent-added toast `[toggle]`.
- **Actions** — Double-click on torrent `[select: Open folder/Open details/Resume-Pause/Nothing]` · Keyboard shortcut set `[select: Default/Emacs/Vim/qBittorrent-parity]`.
- **Startup** — Start IronTide on system login `[toggle]` · Start minimized `[toggle]` · Minimize to tray on close `[toggle]` · Resume previous session on startup `[toggle]`.
- **Notifications** — On download complete `[toggle]` · On error `[toggle]` · On RSS match `[toggle]` · Play sound `[toggle]` · Run external program `[text]` (hint: `%N` name `%F` content-path `%D` save-dir `%I` info-hash).

### 8.2 Downloads
- **Save management** — Default save path `[path]` · Use incomplete-downloads folder `[toggle]` → Incomplete path `[path]` · Append `.!it` to incomplete files `[toggle]` · Use category save-path rules `[toggle]`.
- **When adding a torrent** — Show add-torrent dialog `[toggle]` · Start in paused state `[toggle]` · Skip hash check `[toggle]` · Pre-allocate disk space `[toggle]` · Append date to save path `[toggle]` · Smart category suggestion `[toggle]` (hint: local classifier, no network).
- **Torrent-file handling** — Watched folder `[path]` (hint: auto-add dropped `.torrent`) · Copy `.torrent` files to `[path]` · Move completed `.torrent` files to `[path]` · Delete `.torrent` after adding `[toggle]`.
- **On completion** — Move to folder `[path]` · Run external program `[text]`.

### 8.3 Connection
- **Listening port** — Incoming port `[number]` · Randomize on startup `[toggle]` · Use UPnP / NAT-PMP `[toggle]` · Port status `[badge: Open/Closed/Unknown]`.
- **Connection limits** — Global max connections `[number]` · Per-torrent max `[number]` · Global upload slots `[number]` · Max active downloads `[number]` · Max active uploads `[number]` · Max active total `[number]`.
- **Proxy** — Type `[select: None/HTTP/SOCKS4/SOCKS5]` · Host `[text]` · Port `[number]` · Authentication `[toggle]` (user/pass) · Use proxy for hostname lookups `[toggle]`.
- **IP filtering** — Enable IP filter `[toggle]` · Filter file `[path]` · Refresh on startup `[toggle]`.

### 8.4 Speed
- **Global rate limits** — Download limit `[number] KiB/s` + enable `[toggle]` · Upload limit `[number] KiB/s` + enable `[toggle]`.
- **Alternative rate limits** — Download `[number]` · Upload `[number]` · Scheduled hours `[select preset]` + custom range · Auto-switch on schedule `[toggle]`.
- **Limits apply to** — Transport (TCP/µTP) overhead `[toggle]` · µTP connections `[toggle]` · Exempt LAN peers `[toggle]`.

### 8.5 BitTorrent
- **Privacy / discovery** — DHT `[toggle]` · Peer Exchange (PeX) `[toggle]` · Local Service Discovery (LSD) `[toggle]` · Encryption `[select: Prefer/Require/Disable]` · Anonymous mode `[toggle]`.
- **Seeding limits** — When ratio reaches `[number]` `[toggle]` · When seeding time reaches `[number] min` `[toggle]` · When inactive seeding reaches `[number] min` `[toggle]` · Then `[select: Pause/Remove/Remove+data/Enable super-seeding]`.
- **Queueing** — Enable queueing `[toggle]` · Slow-torrent download threshold `[number] KiB/s` · Upload threshold `[number]` · Inactivity timer `[number] s`.

### 8.6 RSS
- **Reader** — Enable RSS `[toggle]` · Refresh interval `[number] min` · Max articles per feed `[number]`.
- **Auto-download** — Enable auto-downloading `[toggle]` · Smart episode filter `[toggle]` · Smart-filter regex `[text]` · Download REPACK/PROPER `[toggle]`.

### 8.7 Web UI
- **Server** — Enable Web UI / remote control `[toggle]` · Bind address `[text]` · Port `[number]` · Use HTTPS `[toggle]` → Certificate `[path]` · Key `[path]`.
- **Authentication** — Username `[text]` · Password `[text]` · Bypass auth on localhost `[toggle]` · Session timeout `[number] min` · Ban after N failed attempts `[number]`.
- **Security** — Clickjacking protection `[toggle]` · CSRF protection `[toggle]` · Host-header validation `[toggle]` · Allowed hosts `[text]` · Trust reverse-proxy X-Forwarded-For `[toggle]` + list `[text]`.
- **Dynamic DNS** — Enable `[toggle]` · Service `[select: DynDNS/No-IP/DuckDNS/Cloudflare]` · Domain / user / key `[text]`.

### 8.8 Advanced
- **libtorrent** — Async I/O threads `[number]` · Hashing threads `[number]` · File-pool size `[number]` · Outstanding memory when checking `[number] MiB` · Disk cache `[number] MiB` (−1 = auto) · Cache expiry `[number] s` · Coalesce reads & writes `[toggle]` · Piece-extent affinity `[toggle]` · Send/recv socket buffers `[number]`.
- **Network** — Network interface `[select]` · Optional bind IP `[text]` · IP reported to tracker `[select: Auto/Custom/None]` · Check for program updates `[toggle]` · Resolve peer countries `[toggle]` · Resolve peer hostnames `[toggle]`.
- **Maintenance** — Configuration file `/home/alan/.config/irontide/config.toml` + Reveal · Reset all settings to defaults `[button]`.

---

## 8a. First-run setup wizard

Shown once on first launch (gated by a config flag); re-openable via **Help ▸ Setup Wizard…**. A 640×520 modal with a left step rail (emerald bolt + numbered steps; completed steps show an emerald check, active step an emerald left-border) and a Back / Next (→ Finish) footer with a step counter.

| Step | Contents |
|---|---|
| 1 · Welcome | Bolt mark, "Welcome to IronTide", one-line intro, **Get started** |
| 2 · Downloads | Default save path `[path]` · Separate incomplete folder `[toggle]` · Show add dialog `[toggle]` · Start paused `[toggle]` |
| 3 · Connection | Incoming port `[number]` + Randomize `[toggle]` · UPnP/NAT-PMP `[toggle]` · live port-reachability card |
| 4 · Privacy | DHT / PeX / LSD `[toggle]`s · Encryption `[select]` · Anonymous mode `[toggle]` |
| 5 · Done | Confirmation, "press Ctrl+K to find anything", **Start IronTide** |

Every value here is a subset of Preferences (§8) — the wizard writes the same config keys.

> Visual: **`Screens Reference.html`** → "First-run setup wizard". The whole app's screens are captured there as a single contact sheet.

---

## 9. Tool pages (replace the main pane via sidebar Tools / menu)

- **Search** — query + plugin + category filters; results table (name, size, seeds, peers, engine, date, Add → routes to Add dialog pre-filled); plugin manager (enable/disable, install-from-URL, update-all).
- **RSS** — feeds sidebar (unread + last-updated) · items list · docked rules panel (enable, name, must-contain, must-not-contain, episode filter, smart filter, category, save path, feeds).
- **Bandwidth scheduler** — 7×24 paintable grid; cell = Full speed / Alternative / Paused; click toggles, drag paints; presets Work hours / Overnight / Weekends.
- **IP filter** — stats strip (enabled, total ranges, blocked today, last refresh) + editable range table (range, block/allow, comment) + banned-this-session list; import-from-URL, .p2p/.dat.
- **Logs** — mono; level filters (INFO/WARN/ERROR/DEBUG), tail mode, clear, export; rows = timestamp (ms) + level pill + message; auto-scroll pauses on interaction.
- **Statistics** — stat cards (all-time dl/ul/ratio/shared-time, session, global peers, DHT nodes, active/max connections); 90-day transfer graph; cache & I/O table.
- **Web UI / Pair-a-device** — live endpoint info (local/LAN/external URLs, status, uptime, sessions); sessions table with revoke; QR pairing panel (URL + user + one-time token).

---

## 10. Novel features (kept — they fit the terminal aesthetic)

1. **Command palette (Ctrl+K)** — fuzzy, grouped (Action / Navigation / Torrents / Settings); every action, setting and torrent reachable by typing. Footer shows ↑↓ select · ⏎ open · Ctrl+K close.
2. **Smart category suggestion** — local classifier inspects names + sizes → suggests category + save path; right-click → "Train as: X".
3. **Bandwidth intent** — goals ("leave 10 Mbps for calls", "≤80% of uplink", "pause during Meet"); probes uplink and auto-adjusts caps.
4. **Pair-to-phone QR** — Web UI pairing with rotating one-time tokens, revocable from the sessions table.
5. **Verify-before-download** — when files exist at the save path, offer a local hash pass before any network traffic.

---

## 11. Keyboard shortcuts

All accelerators are **Linux/KDE conventions** (Ctrl, not ⌘). Registered in `accel.rs`; the macOS build may map Ctrl→⌘ at that layer, but every displayed label in the UI reads Ctrl.

`Ctrl+K` palette · `Ctrl+,` preferences · `Ctrl+O` add file · `Ctrl+Shift+O` add link/magnet · `Ctrl+N` create · `Space` pause/resume · `Delete`/`Backspace` remove · `Shift+Delete` remove + data · `Ctrl+Shift+F` force recheck · `Ctrl+Up`/`Ctrl+Down` queue up/down · `Ctrl+Shift+Up`/`Ctrl+Shift+Down` queue top/bottom · `Ctrl+I` toggle details · `Ctrl+L` focus list · `Ctrl+A` select all · `Ctrl+F` search · `F2` rename · `F1` docs · `/` filter · `Ctrl+1…9` jump to sidebar section · `Esc` close modal / clear selection.

---

## 12. Defaults (shipping)

| Knob | Default |
|---|---|
| Theme | Emerald-dark (only option) |
| Layout | L1 3-pane (only option) |
| Density | Compact |
| Sidebar | Full |
| Row striping | On |
| Emerald glow | On |
| UI font | Inter · Data font | JetBrains Mono |

---

## 13. Slint mapping (unchanged from v0.1, still valid)

Direct: `Window`/`MenuBar`, `Button`, `Switch`, `LineEdit`/`TextEdit`, `ComboBox`, `ScrollView`/`Flickable`, `TabWidget` (restyled), `Tooltip`.

Custom: `TorrentTable` (virtualized `ListView` + custom header), tinted `ProgressBar`, `Sparkline`/`SpeedGraph` (`Path`), `PieceAvailabilityMap` (`tiny_skia` → `Image`), `BandwidthScheduler` (168-cell paint grid), `CommandPalette` (`PopupWindow` + virtualized `ListView`), `StatusDot` (animated halo), `Toast`, `QRCode` (`qrcode` crate → `Image`), country flags (spritesheet + `source-clip`).

Token globals live in `ui/tokens.slint` — now a **single** palette (see `CODEBASE-CLEANUP.md`).
