# IronTide Design System

IronTide ships three surfaces: a **Slint desktop GUI** (M162-M164, default daily driver), a **browser Web UI** (M165-M167, for headless hosts), and a **terminal CLI + TUI** (M159-M160, for pipelines and scripts). Each surface has its own visual vocabulary and interaction idioms, but they share a common set of names, value formats, and mental models so that a user who moves between them is never surprised.

This document is the canonical reference for that shared vocabulary and the per-surface style choices. Live code owns the truth; when this doc drifts, the code wins and this file should be updated.

---

## Shared vocabulary

These names and formats appear in all three surfaces. Changing one requires changing all three.

### Torrent states

The engine's `TorrentState` enum maps to a single canonical lowercase label that every surface renders:

| Engine state | Label       | Colour hint  | Meaning                                                           |
|--------------|-------------|--------------|-------------------------------------------------------------------|
| Checking     | `checking`  | yellow       | Verifying on-disk data against piece hashes                       |
| Downloading  | `downloading` | green      | Active peer connections, accepting blocks                          |
| Complete     | `complete`  | blue         | All pieces verified; transitioning to seeding                      |
| Seeding      | `seeding`   | blue         | Upload-only, serving peers                                         |
| Paused       | `paused`    | grey         | User paused; no peer connections                                   |
| Stopped      | `stopped`   | grey         | Removed from session (terminal)                                    |
| Sharing      | `sharing`   | blue         | Share mode — relay in memory without writing to disk               |
| *(user flag)* | `seed only` | blue        | `user_seed_mode=true`; downloading stops, uploads continue         |
| *(no meta)*   | `fetching metadata` | grey | Magnet added; waiting on peers for `ut_metadata`                 |

State strings are produced by `irontide_format::format_state(state, user_seed_mode)` so no surface has to re-implement the user-seed-mode override logic.

### Value formatting

All three surfaces use the `irontide-format` crate for user-facing numeric output:

- **Size** → `format_size(bytes)` — binary units (`1.2 MiB`, `512 GB`)
- **Rate** → `format_rate(bytes_per_sec)` — `5.2 MB/s`, `512 KB/s`
- **ETA**  → `format_eta(remaining_bytes, rate_bps)` — `∞` / `1d 4h` / `35m` / `8s`
- **Ratio** → `format_ratio(uploaded, downloaded)` — `1.42`, `∞` for pure-seed
- **Progress pct** → `format!("{:.1}%", progress * 100.0)` — one decimal place consistently

### Info hash rendering

Every surface renders SHA-1 info hashes as **lowercase 40-char hex**. The parser (`parse_info_hash()`) accepts either case but normalizes via `Id20::to_hex()`, so downstream code never has to handle mixed case. SHA-256 (v2) info hashes are 64-char lowercase hex.

### Peer flag glyphs

GUI and Web UI share a fifteen-glyph vocabulary (CLI currently omits flags) — a qBittorrent-parity superset shipped in M171 (D5). `I` was "peer is interested" pre-M171; it now means **incoming connection** to match qBt. Peer-interested state is still visible through `U`/`u`.

| Glyph | Meaning                              | Condition                                          |
|:-----:|--------------------------------------|----------------------------------------------------|
| `D`   | Downloading from peer                | `!peer_choking && num_pieces > 0 && am_interested` |
| `d`   | We want data but peer chokes us      | `am_interested && peer_choking`                    |
| `U`   | Uploading to peer                    | `!am_choking && peer_interested`                   |
| `u`   | Peer wants data, we are choking them | `peer_interested && am_choking`                    |
| `K`   | We are choking the peer              | `am_choking`                                       |
| `?`   | We are interested in the peer        | `am_interested`                                    |
| `S`   | Peer is snubbed (no data in window)  | `snubbed`                                          |
| `O`   | Optimistic unchoke slot              | `is_optimistic`                                    |
| `I`   | Incoming connection                  | `source == Incoming`                               |
| `H`   | Discovered via DHT                   | `source == Dht`                                    |
| `X`   | Discovered via PeX (BEP 11)          | `source == Pex`                                    |
| `L`   | Discovered via LSD (BEP 14)          | `source == Lsd`                                    |
| `E`   | Encrypted connection (MSE/PE)        | `is_encrypted`                                     |
| `P`   | Using uTP (BEP 29)                   | `uses_utp`                                         |
| `F`   | Supports fast extension (BEP 6)      | `supports_fast`                                    |

Web UI renders each glyph as `<abbr title="…">X</abbr>` with an expandable `<details>` legend. `is_optimistic` and `is_encrypted` are tracked on `PeerInfo` but not yet populated by the choker / handshake state machine (follow-up polish in M172+); `uses_holepunch` is tracked on `PeerInfo` for observability but has no assigned glyph.

---

## Web UI (Slint surface #1 — browser)

**Tech stack:** HTMX 2.x · Pico CSS v2 (dark theme) · Askama templates · `rust-embed` static assets.

### Design tokens

Consolidated as comments at the top of `crates/irontide-webui-assets/assets/css/app.css`:

**Colours.** Built on Pico's dark theme CSS custom properties (`--pico-primary`, `--pico-muted-color`, `--pico-muted-border-color`, `--pico-color`, `--pico-primary-background`, `--pico-del-color`, `--pico-border-radius`, `--pico-font-family-monospace`). Custom state tones layer on top:

- `state-downloading` → `#22c55e` (green)
- `state-seeding` / `state-complete` / `state-sharing` / `state-seed-only` → `#3b82f6` (blue)
- `state-paused` / `state-fetching` / `state-stopped` / `state-unknown` → `#9ca3af` (neutral grey)
- `state-checking` → `#ffdc00` (yellow)
- `tracker-status-error` → `#ef4444` (red)

**Typography.** Pico default sans-serif. Monospace (`var(--pico-font-family-monospace)`) for info hashes and technical identifiers only.

**Spacing scale.** Pico default: `0.25rem / 0.5rem / 1rem / 1.5rem / 2rem / 3rem`. Tables use `font-size: 0.875rem` for density, dl labels `0.85rem` and values `0.9rem`.

**Border radius.** `var(--pico-border-radius)` (0.25rem) almost everywhere. State chips use fully rounded (`999px`) pill shape.

**Breakpoints.**

- `≤ 375px` — small phones (iPhone SE): stack detail header into three lines, drop Peers `client` + `flags` columns, collapse tracker status chip to a colour-only dot, scroll tablist horizontally.
- `≤ 768px` — tablets: 2-column header, full Peers/Trackers rows, condense action buttons.
- `≥ 1024px` — laptops+: layout caps at `max-width: 1280px` so long lines stay readable.

### Component vocabulary

- **Torrent row** — table `<tr>` with state-coloured `<progress>`, action-button cluster, and name link to `/webui/torrents/{hash}`.
- **Action-button cluster** — small square-ish transparent buttons, one per operation (pause/resume/seed/unseed/remove). Hover state tints by intent (green for seed, red for remove).
- **Settings form** — fieldset-grouped 11-field editor, `PATCH /webui/settings` with RFC 7396 merge-patch.
- **Detail header** — breadcrumb → `<h1>` torrent name → state chip + progress + percentage → summary row (`↓ rate · ↑ rate · ETA · ratio`).
- **Tab nav** — `role="tablist"` with `role="tab"` buttons. Active tab: `aria-selected="true"`, `tabindex="0"`, underline + primary colour. Keyboard: arrow keys cycle, Home/End jump, Enter/Space activate.
- **Data table** — `role="grid"`, right-aligned numeric columns, truncated long strings with `text-overflow: ellipsis`.
- **Fragment with hx-indicator** — every lazy-loaded panel has a shared `htmx-indicator` spinner that toggles during the request.
- **ARIA live region** — a single `.sr-only` `aria-live="polite"` node announces mutations to screen readers ("Priority changed to Normal for README.txt").
- **State chip** — coloured pill on the detail header; class name drives the hue.
- **Flag legend** — `<details>` expander below the Peers table listing each glyph and its meaning.

### Interaction patterns

- **HTMX events.**
  - List-view mutations → `HX-Trigger: refreshList` header; `ws-live.js` dispatches a `refreshList` CustomEvent on alert.
  - Detail-view mutations → `HX-Trigger: {"refreshDetail":{"hash":"<lower-hex>"}}` header; `ws-live.js` filters alerts to emit `refreshDetail` scoped to the currently-viewed torrent only.
  - Built via `serde_json::json!` rather than `format!` so there's no path for malformed JSON or header injection.
- **Adaptive polling.** Panels have two cadences: 2 s when the WebSocket is dead (so the UI still feels live) and 30 s while WS is connected (WS push drives the refresh). `setDetailPollCadence(fast)` swaps `hx-trigger` on each `[data-detail-poll]` element and calls `htmx.process(el)` explicitly — without that call, HTMX silently ignores the attribute change.
- **Graceful removal.** A body-level `htmx:responseError` listener catches any fragment 404 and swaps the whole detail view with a "This torrent was removed" banner + back link. `data-detail-hash` is cleared on the body so `ws-live.js` stops dispatching `refreshDetail` for the dead torrent.
- **Skeleton shimmer.** `.skeleton-row` with a `@keyframes shimmer` gradient pulse fills empty panels while a fragment is loading.

### Accessibility

- **Touch targets.** 44 px min-height on every interactive element (tabs, priority select, Force Reannounce, row-name anchor). Meets WCAG 2.1 AAA.
- **Focus.** Strong `:focus-visible` outline (`2px solid var(--pico-primary)`, `2px` offset) on every interactive element. Focus moves to the tabpanel when a tab is activated; to the back link when the removed-banner swaps in.
- **Keyboard tablist.** WAI-ARIA Authoring Practices pattern: `tabindex="0"` on active tab, `tabindex="-1"` on the rest; arrow keys cycle, Home/End jump first/last; Enter/Space activate.
- **Screen readers.** Every attacker-controlled string goes through Askama's default HTML escaper. URL attributes use explicit `|e`. Peer flags have `<abbr title="…">` tooltips. ARIA live region announces mutations.
- **Contrast.** Body text ≥ 4.5:1 vs background. Muted text ≥ 4.5:1. State-chip text ≥ 4.5:1 against its tinted background. Progress-bar fill ≥ 3:1 vs track.

### Security

The Web UI runs unauthenticated at `127.0.0.1` (default bind since M159). M167 expands the data-exposure surface (peer IPs, local download path, hostile metadata) but does not add new attack surface beyond the existing "user is on localhost or explicitly opened the port." CSRF is deferred to M168. Every mutation handler carries an inline `// NOTE: unauthenticated — M168 adds CSRF.` comment as a pre-merge guardrail.

---

## GUI (Slint desktop — surface #2)

**Tech stack:** Slint UI language · tokio session on a background thread · Slint main thread for the event loop.

Established in **M162** (scaffold + main window), **M163** (torrent list with live updates), **M164** (torrent controls: add/remove/pause/resume/seed, context menu, keyboard shortcuts).

### Design tokens (`crates/irontide-gui/ui/palette.slint`)

```slint
export global Palette {
    out property <color> background: #1a1a2e;    // app background
    out property <color> surface: #16213e;       // cards, toolbars
    out property <color> text: #e0e0e0;
    out property <color> accent: #0f3460;        // primary buttons
    out property <color> border: #333333;
    out property <color> menu-bg: #222244;
    out property <color> menu-hover: #2a2a5e;
    out property <color> disabled-text: #666666;
    out property <color> row-alt: #1e1e34;       // alternating table rows
    out property <color> col-separator: #2a2a4a;
    out property <color> danger: #e53935;        // remove actions
    out property <color> input-bg: #0d1117;
    out property <color> input-border: #444466;
    out property <color> checkbox-check: #4caf50;
    in-out property <bool> dark-mode: true;
}
```

The palette is dark-mode only in M162-M167 (the `dark-mode` flag is plumbed but there is no light-mode colour set yet).

### Custom widgets

Slint's `std-widgets` are Qt-native and ignore Slint sizing for hit regions (see `feedback_slint_native_widget_hit_regions.md`). Every GUI widget in IronTide is built from `Rectangle + Text + TouchArea + PopupWindow` so that hit regions exactly match the visual chrome:

- **`MenuButton`** — toolbar-style button with hover highlight + keyboard focus ring.
- **`MenuBar`** — horizontal menu strip with dropdown popups (File, Edit, View, …).
- **`Spinner`** — animated loading indicator (8-spoke rotating gradient).
- **`Palette.row-alt`** — alternating table row backgrounds for the 10-column torrent list.

### Component vocabulary

- **Main window** — single top-level `Window` hosting MenuBar, torrent list, toast overlay, and modal dialogs.
- **Torrent list** — 10-column sortable table (Name / Progress / State / Down / Up / Seeds / Peers / ETA / Size / Ratio). State-coloured progress bars driven by the same `TorrentState` enum as the Web UI. Multi-selection via click / Ctrl+click / Shift+click / Ctrl+A. Column order, visibility, and width are persisted via `[gui]` section of `config.toml`.
- **Context menu** — right-click on a selected row opens an 8-action menu (Pause, Resume, Start Seeding, Stop Seeding, Force Recheck, Set File Priorities, Remove, Remove + Delete Data). Dynamic enable/disable based on the selected torrents' current states.
- **Dialogs** — Add Magnet, Add Torrent File (native file chooser), Confirm Delete (with "Also delete files on disk" checkbox), Set File Priorities, Error.
- **Toast overlay** — transient success/error messages that fade out after a fixed delay.
- **Status bar** — aggregate session stats (active torrents, total ↓/↑ rate, ratio).

### Keyboard shortcuts (M164)

| Key            | Action                                       |
|----------------|----------------------------------------------|
| `Space`        | Toggle pause/resume on selected torrent(s)   |
| `Delete`       | Open Remove confirmation                     |
| `Ctrl+A`       | Select all                                   |
| `Enter`        | Open properties / detail (planned M172)      |
| `Ctrl+O`       | Add Torrent File                             |
| `Ctrl+M`       | Add Magnet                                   |
| `F5`           | Refresh torrent list (force poll)            |
| `Ctrl+Q`       | Quit                                         |

### Interaction patterns

- **Command channel.** GUI actions send `SessionCommand` messages to the background session thread and wait on `oneshot` replies. There is no direct synchronous coupling between the Slint event loop and the session actor.
- **Poll loop.** The GUI polls `list_torrent_summaries()` every 500 ms to refresh the table. This is intentionally simpler than the Web UI's WebSocket push — the GUI has direct in-process access and doesn't need heartbeats.
- **Config persistence.** Column order/visibility/widths are written to `~/.config/irontide/config.toml` on close and restored on open.

### GUI-only features

The GUI does not currently have an equivalent of the Web UI's per-torrent detail tabs. M172-M175 will add Files / Peers / Trackers / Info tabs using the same custom-widget approach.

---

## CLI + TUI (surface #3)

**Tech stack:** `clap` (`cli_def.rs` with `include!()` for build.rs completions) · `rustyline` (shell REPL) · `ratatui` + `crossterm` (TUI dashboard) · `tracing` + `tracing-subscriber` (logs).

Established across M159 (daemon + batch subcommands + REPL + TUI), M160 (TOML config + shell completions), M161 (fast-resume + daemon auto-restore).

### CLI surface — plain text + structured JSON

The CLI has two mutually-exclusive output modes on every subcommand that produces data:

- **Default:** human-readable columnar text. State labels and values use the same `irontide_format::format_*` helpers as the GUI and Web UI.
- **`--json`:** one JSON object per line (newline-delimited JSON). Field names match the REST API DTOs — the CLI talks to the daemon over HTTP, so the DTOs are the same types.

No colours by default (terminal-friendly for pipelines). Error output goes to stderr with a non-zero exit code.

### TUI dashboard (`irontide tui`)

**Tech stack:** `ratatui 0.28` + `crossterm 0.28` with `event-stream`.

- Full-screen table view with the same 10 columns as the GUI.
- Modal dialogs for add-magnet, confirm-remove, etc.
- Keyboard-driven: `↑↓` or `jk` navigation, Enter (details), `s/p/r/d/a` for common actions, `q` to quit, `?` for help, `Esc` to cancel modals, `F5` to force refresh.
- Refreshes on WebSocket event-driven invalidation rather than a timer.

### Shell REPL (`irontide shell`)

- `rustyline` persistent history at `$XDG_CACHE_HOME/irontide/history`.
- Dynamic prompt shows live daemon state (e.g. `[3 active, 2 paused] >`).
- Tab completion for subcommands and hash prefixes.

### Shell completions

Generated at build time via `build.rs` (`bash`/`zsh`/`fish`/`elvish`/`powershell`) and also available at runtime via `irontide completions <shell>`.

---

## How the three surfaces relate

The three surfaces serve different use cases but share a consistent mental model:

| Aspect              | GUI (Slint)        | Web UI (HTMX)             | CLI / TUI                |
|---------------------|--------------------|---------------------------|--------------------------|
| Audience            | Daily driver       | Headless / remote         | Pipelines / power users  |
| Latency             | Direct in-process  | HTTP + optional WS push   | HTTP (stateless subcmds) |
| Update mechanism    | 500 ms poll        | 2 s↔30 s adaptive + WS    | On-demand subcommand     |
| State labels        | Shared `format_state()` | Shared `format_state()` | Shared `format_state()` |
| Rate / size / ETA   | Shared `irontide_format` helpers | same | same                     |
| Info hash format    | Lowercase hex      | Lowercase hex             | Lowercase hex            |
| Authentication      | Local process      | *(deferred M168)*         | Local process            |

A user who pauses a torrent in the GUI sees the state flip to `paused` in the Web UI within two seconds (WS-down fallback polling) or <1 s (WS push). The CLI's `irontide list --json` against the same daemon returns the same state label. No surface has a private vocabulary.

### Cross-surface consistency checks

1. **State labels.** Changing `format_state()` in `irontide-format` automatically updates the Web UI detail chip, GUI table cell, and CLI list output. Never introduce a surface-local state label.
2. **Formatters.** The `format_size`, `format_rate`, `format_eta`, `format_ratio` helpers are the **only** way to produce user-facing numeric text. Re-implementing any of them in a single surface is a bug.
3. **Flag vocabulary.** Peer flag glyphs (D/U/K/?/I/S) are shared between GUI and Web UI; extending them requires updating both the `peer_flags()` helpers and the legend/tooltip content.

---

## qBt v2 compatibility surface (M168 + M169 + M170)

A **fourth surface** opts into the qBittorrent WebUI v2 HTTP API so that `*arr` clients (Radarr / Sonarr / Prowlarr / Lidarr) can drive IronTide as if it were qBt. Unlike the GUI / Web UI / CLI surfaces which share IronTide's vocabulary, this one **adopts qBt's vocabulary wholesale** — IronTide state strings, field names, and enum codings are projected onto qBt's canonical names so unmodified `*arr` code works.

### Boundary + scope

- **Separate URL namespace.** All endpoints sit at `/api/v2/*`, fully disjoint from the native `/api/v1/*` surface. The v1 API is frozen and never changes based on whether qbt_compat is enabled.
- **Opt-in.** `qbt_compat.enabled` defaults to `false`; when false, the `qbt_gate` middleware returns 404 for every `/api/v2/*` request — the route must appear non-existent, not return 403 (security-through-minimization, matches qBt Desktop's own opt-in model).
- **No design-system consistency with GUI / Web UI / CLI.** Field casing, value encoding, and default values follow qBt conventions (`save_path` not `download_dir`, `num_leechs` not `num_peers - num_seeds`, encryption `0/1/2` for `Prefer/Force/Disable`). A reader moving from the IronTide GUI to the qBt v2 surface should expect a protocol-compatibility shim, not a second native view.

### Spoofed fields

Three fields in `/api/v2/app/*` responses are configurable so users can match whatever qBt version their `*arr` stack expects:

| Field | Default | Config key |
|-------|---------|------------|
| `app/version` body | `v5.1.4` | `qbt_compat.spoof_app_version` |
| `app/webapiVersion` body | `2.11.4` | `qbt_compat.spoof_webapi_version` |
| `app/buildInfo.qt` etc. | pinned recent qBt values | hardcoded |

`bitness` is derived from `std::mem::size_of::<usize>() * 8` so it's correct on ARMv7 / 32-bit x86 builds.

### State mapping

IronTide's `TorrentState` is richer than qBt's state enum in some axes (e.g., `FetchingMetadata`, `Sharing`) and poorer in others (qBt has both `downloading` and `stalledDL` for the same internal state, depending on dlspeed). The `qbt_state_string(&TorrentStats)` helper is the single point of translation:

| IronTide TorrentState | qBt state | Additional condition |
|-----------------------|-----------|----------------------|
| FetchingMetadata | `metaDL` | — |
| Checking | `checkingUP` or `checkingDL` | progress >= 1.0 |
| Downloading | `downloading` or `stalledDL` | download_rate > 0 |
| Complete / Seeding | `uploading` or `stalledUP` | upload_rate > 0 |
| Paused | `pausedUP` or `pausedDL` | progress >= 1.0 (takes precedence over other flags) |
| Stopped | `pausedDL` | — |
| Sharing | `forcedUP` | — |
| *(any, with error)* | `error` | error string non-empty |

### What M170 landed, what's still deferred (M171 / M172)

**M170 shipped (v0.170.0):** `GET /torrents/files?hash=X` with `QbtFile` DTO, `QbtTorrentProperties` populated with real `save_path` / `created_by` / `creation_date` / `piece_size`, category CRUD (`createCategory` / `editCategory` / `removeCategories` persisted at `$XDG_CONFIG_HOME/irontide/categories.toml`), add-time category → save_path resolution, `category` filter on `/torrents/info`, `deleteFiles=true` on `/torrents/delete` with empty-parent-dir cleanup and delete-re-add race guard, `QbtTorrent.category` populated on the list DTO.

**Still deferred to M171 (qBt v2 parity completion):**
- `max_ratio_act`, `max_seeding_time_enabled`, `max_inactive_seeding_time_enabled`, `queueing_enabled`, `auto_tmm_enabled`, `create_subfolder_enabled` — preferences dials that IronTide has settings for but aren't yet connected. The M170 scope kept them as `FIXME(M171)` with safe defaults.
- `tags`, `auto_tmm`, `priority` on `QbtTorrent` — tags CRUD is M171; auto_tmm stays hardcoded `false` (no automatic torrent management in IronTide; if *arr refuses to proceed when it's `false`, revisit during M172 Docker integration tests).
- Read-only detail endpoints: `/trackers`, `/webseeds`, `/pieceStates`, `/pieceHashes` — M171.
- `POST /api/v2/app/setPreferences` JSON merge-patch round-trip — M171.
- `dht_nodes` on `transferInfo` — hardcoded `0` until `SessionHandle` exposes a DHT node count accessor. Retargeted from `FIXME(M170)` to `FIXME(M171)` during M170 ship.

**Deferred further (M172):** argon2 password hashing, Referer/Origin CSRF on `/webui/*`, brute-force ban, Docker-based `*arr` integration test suite.

**Deferred beyond the 209-milestone plan:** `setCategory` with auto-TMM file relocation — a standalone file-relocation subsystem, not qBt endpoint plumbing. Proposed for a dedicated milestone after Phase O.

### Auth model

- **Login:** `POST /api/v2/auth/login` (form body `username=X&password=Y`) → 200 `Ok.` + `Set-Cookie: SID=<32 chars URL-safe base64>; HttpOnly; Path=/; SameSite=Lax`.
- **Logout:** `POST /api/v2/auth/logout` → always 200 `Ok.` (idempotent — qBt accepts it with or without a valid cookie).
- **Session token:** 24 bytes from `aws_lc_rs::rand::SystemRandom::fill` → URL-safe base64 (32 chars). xorshift64 would give 64 bits of real entropy regardless of token length, which is inadequate for auth.
- **TTL:** 24 hours with lazy expiry on `validate()` + `last_used` refresh so active sessions never timeout at boundaries. 1024-session LRU cap prevents login-storm memory growth.
- **In-memory only:** daemon restart forces `*arr` to re-authenticate on the next 403. `*arr` handles this transparently — documented as an acceptable robustness gap.
- **Plaintext password compare (M168).** Local-only bind mitigates the timing-side-channel; argon2 hashing + constant-time compare land in M171.
- **No CSRF (M168).** Real qBt only enforces CSRF on the browser Web UI surface, not the API, so this matches production qBt. M171 adds Referer/Origin validation for `/webui/*` only.

### Future work

- **M170:** `/torrents/files`, `/trackers`, `/webseeds`, `/pieceStates`, `/pieceHashes`, `/filePrio`; category CRUD (`createCategory`, `editCategory`, `removeCategories`) + tag CRUD; `setPreferences` (RFC 7396 merge-patch into `Settings` via `json_merge_patch` helper from session.rs:120); `app/shutdown`.
- **M171:** argon2 hashing (one-way, prevents plaintext extraction on disk), CSRF on `/webui/*` (Referer / Origin header check), brute-force ban on `auth/login` (10 failures → 5-minute ban on the source IP), Docker-based `*arr` integration tests (spin up Radarr + Sonarr + Prowlarr + Lidarr in containers and run real RSS-to-download-to-complete flows against an IronTide daemon).

---

## Phase N + Phase O additions (M168-M232)

Everything above this section describes the shipped-and-stable surfaces as of v0.168.0. The sections below capture every additional surface that landed between M168 (v0.168.0 / 2026-04-18) and M232 (v1.0.0-rc11 / 2026-05-28), the active end of Phase O. New work is grouped by surface, not by milestone — the canonical per-milestone narrative lives in `CHANGELOG.md`.

### GUI evolution (Slint desktop — surface #2)

- **Detail pane tabs (M172-M177).** General + Trackers + Peers + Files + Speed + Pieces tabs. `M172b` closes the qBt v2 lane-B round-trip gap (settings-pack vs WebUI patch shape). `M173` adds the left-rail Sidebar with collapsible State / Category / Tag / Tracker chip filters. `M174` adds the multi-modal Add Torrent dialog (file / URL / magnet). `M175` does the inspector pane (later collapsed in M187 — see "L2/L3 retirement" below). `M176` adds commit-staging (preview before commit). `M177` finalises detail-pane tab persistence via `GuiConfig.detail_active_tab`.
- **Command palette (M183).** `Ctrl+K` palette with fuzzy command search, scoped action vocabulary (Add / Pause / Resume / Remove / Recheck / Move / Set Priority / Open Folder / etc).
- **Preferences dialog evolution (M184/M185/M187/M214/M215/M226).** First-class 8-tab dialog mirroring the qBt v2 Preferences shape. `M184` ships Downloads + Connection + Speed tabs. `M185` adds BitTorrent + Web UI + Advanced + Behaviour tabs. `M187` adds RSS + final layout polish. `M214` wires Connection + Speed engine round-trip. `M215` wires BitTorrent + Advanced engine round-trip. `M226` expands the engine surface to cover notifications + watched-folder + network-interface + incomplete/completed paths + 14 new `Settings` fields (Class B closure).
- **Menu bar (M216).** Full File / Edit / View / Help menu surface with platform-native styling. Column visibility submenu under View added later in `M229`.
- **Keyboard shortcuts (M217).** qBt v2 parity sweep — adds Copy Magnet, Find, additional torrent navigation. Builds on the M164 shortcut spec.
- **Add Torrent URL (M218).** Replaces the URL tab placeholder with a real `reqwest::blocking::Client` fetcher under `spawn_blocking`. Two-layer SSRF mitigation (pre-flight `url_guard::validate_user_url` + in-flight redirect re-validation). 10 MiB streaming cap. Content-Type sniff + BEP 3 bencode magic-byte fallback. Generation counter races out stale results on fast typing.
- **Window state persistence (M219).** `GuiConfig.window: Option<WindowConfig>` (`width`/`height`/`x`/`y`/`maximized`). New `clamp_position` helper guarantees ≥200 logical px title-bar horizontal + ≥64 px vertical visibility on restore. KDE Wayland `set_position` documented as advisory.
- **First-Run wizard redesign (M220).** Step 0 (Download Directory) gains real `rfd::FileDialog::pick_folder` + writability probe (`OpenOptions::create_new` atomic). Step 1 (Connection Settings) gains live port-range validation (1024-65535) with inline red error Text. Rebuilt on `PrefField`/`PrefToggle`/`PrefTextInput` molecules for visual parity with M184.
- **L2/L3 inspector pane retirement (M187 → M229 D2 formalisation).** The 3-pane layout (M175) was collapsed to L1-only in M187. The dormant `GuiConfig.inspector_shown` field is retained ONLY for backward-compat deserialization, pinned by 6 round-trip test cases.
- **Column visibility submenu (M229 D3).** View → Columns submenu wiring the existing M188 dispatcher infrastructure. 10 `col-*-visible` properties on `MenuBar` + `MainWindow` with ✓/spacer text. Save batched via `columns_dirty` flag on shutdown.

### Web UI evolution (Pico CSS + HTMX + Askama — surface #1)

- **Polish + live stats (M189).** Live-updating speed gauges + state counts via the existing 2 s polling fragment.
- **Torrent table hardening (M188).** Column visibility groundwork, dispatcher infrastructure later reused by M229. Selection model. Click-and-drag-target affordances.
- **Settings page (M165 single-form, retired M232).** The M165 11-field `/settings` page is retired. `GET /webui/settings` now returns `302 Found` → `/webui/preferences`.
- **Add Torrent dialog (M230).** 3-tab dialog — file upload (multipart, 10 MiB guard + `DefaultBodyLimit` route layer) / URL fetch (same two-layer SSRF defence as M218) / magnet (unchanged shape). `switchAddTab(name)` JS clears `#add-torrent-error.innerHTML` on tab switch.
- **Sidebar IA + filters (M231).** Filter sidebar mirroring GUI M173 — 4 collapsible sections (State / Category / Tag / Tracker) with chips driving server-side predicates on the 2 s polling fragment. Sentinels `uncategorised` / `untagged` / `no_tracker` for "absence" filters. OR-within-axis / AND-across-axes. `<aside id="sidebar">` shell with `grid-template-columns: 240px 1fr` at ≥768px (stacked on narrow). `localStorage.irontide.sidebar.collapsed` persists chevron state.
- **Preferences dialog (M232).** 8-tab Askama template (Downloads / Connection / Speed / BitTorrent / Web UI / Behaviour / Advanced / About) replacing the M165 single-form. `PreferencesForm` 41-field struct mirroring GUI M184/M185/M214/M215/M226. `POST /webui/preferences/save` calls `apply_settings_classified` and emits `HX-Trigger: {"settingsSaved": {"restartPending": [...]}}` — payload is **nested** under `settingsSaved` so HTMX 2.x dispatches a single event whose `detail.restartPending` carries the list (flat payload would split into two events; see [[project_irontide_htmx2_flat_hx_vals]]).

### Engine evolution (irontide-session + irontide-storage + irontide-utp)

- **BEP completion sweep (M186 / M190).** M186 closes BEP quick fixes (peer-flag bits, BEP 38 mtime). M190 finishes BEP protocol completion across the wire surface.
- **uTP integration (M182).** Event-driven `SocketActor` — no safety-net tick (see [[feedback_irontide_utp_no_safety_tick]]); missing wakes must fail tests, not get papered over.
- **Parallel-add tail (M221.1a + M223).** M221.1a adds per-command `tracing::info_span!` instrumentation on every `SessionCommand` with `queue_wait_ms` + `handler_ms`. M223 fixes the spawn-per-add tail by splitting `handle_add_torrent` into off-actor *prep* (`tokio::spawn`'d) and on-actor *commit* — parallel-7 add now `max < 5 s` and spread `≤ 1.5×` (pre-fix ~2.37×).
- **Settings truth: connection caps (M224).** `LiveConnectionGuard` RAII at the TCP accept gate enforces `max_connections_global`. `max_uploads_per_torrent` propagates through `Choker::set_unchoke_slots`. Latent `select!` deadlock in `ListenerTask::run` discovered and fixed via a drain test.
- **Settings truth: timers + threads + live bans + uTP cap (M225).** `save_resume_interval` + `hashing_threads` + `ip_filter_enabled` reclassified `restart_required` → `immediate`. Notify-rebuilt `Interval` for save-resume. `BruteForceRegistry::shrink_preserving_recent_bans` two-tier algorithm preserves active bans across capacity shrinks. uTP inbound admit gate at `SocketActor::handle_inbound_syn` via public `AdmitGate { max_connections_global, live_count, ip_blocker }`.
- **Engine settings expansion: Class B closure (M226).** 14 new `Settings` fields covering notifications + watched-folder + network-interface + incomplete/completed paths. `SettingsDelta` 15-branch fan-out with `Option<Option<>>` for clear-to-None semantics. `classify_immediate` 15 wire aliases. New `notification` module (notify-rust 4.17 with pure-Rust zbus, async-trait DI for `LibNotifySink`/`InMemorySink`). New `watched_folder` module (`notify-debouncer-full` 500 ms debounce + 4-permit semaphore + symlink-aware path sandbox + retry + `.duplicate`/`.malformed` rename).

### qBt v2 compatibility surface (continued from M168-M170)

- **Round-trip completion (M171 / M172b).** Lane B sync — settings-pack vs WebUI patch shape reconciliation. Closes 6 `FIXME(M171)` markers.
- **setPreferences + files response completion (M228).** `QbtPreferencesPatch` gains 16 new `Option<T>` + `#[serde(default)]` partial-update fields with matching apply arms. `QbtPreferences` GET gains 15 new fields. Per-file `priority` sourced from `session.file_priorities(id)`. Per-file `availability` computed as mean have-count per piece across the file's range. Wire-format risk inventory documented inline: `auto_delete_mode` wire-`1` lossy round-trip, `preallocate_all=false` writes `Some(Sparse) → None` on read, 5 fields STORED ONLY (storage / subprocess / rustls / SO_BINDTODEVICE / .!it-ext deferred).
- **`X-IronTide-Restart-Pending` semantics.** qBt v2 `setPreferences` classification surfaces a `restart_required` field set. Web UI (M232) consumes it via `HX-Trigger`. GUI (M214/M215) consumes it via the Preferences dialog footer.
- **CSRF guard.** Reverse-proxy semantics in `crates/irontide-api/src/routes/qbt_v2/security.rs` use `resolve_client_ip` + `proxies.iter().any(|net| net.contains(&client_ip))` — known follow-up tracked in CHANGELOG M232 "Known invariants & follow-ups".

### Quality-of-life features (M194-M204 sprint)

- **Watched folder auto-add (M194).** Mirrors qBt v2's `watched_folders` config — `inotify`-driven, BEP 3 bencoded `.torrent` detection.
- **File associations + magnet handler (M195).** Linux MIME + `xdg-mime` registration; macOS `LSHandlerContentType`; Windows registry keys.
- **Search + plugin framework (M196).** Plugin discovery in `~/.local/share/irontide/search-plugins/`. qBt-compatible plugin shape.
- **RSS reader (M197).** Auto-add rules with regex/wildcard match. Per-feed throttle.
- **Bandwidth scheduler (M198).** Time-of-day rate-limit profiles with overlap resolution.
- **IP filter (M199).** P2P-format and DAT-format support. Per-rule deny/allow.
- **Logs + statistics (M200).** Logs panel with structured filter (level / span). Session statistics summary tab.
- **Clippy style + refactor sweep (M201).** Workspace-wide clippy cleanup pre-1.0.
- **Smart category suggestion (M202).** Title pattern-match → category proposal at add time.
- **Bandwidth intent (M203).** Per-torrent intent ("background download" / "active stream") drives auto-tuning of bandwidth + slot allocation.
- **Pair-to-phone QR (M204).** Web UI auth pairing flow for mobile via QR code containing one-shot token.

### Packaging + first-run (M205-M210)

- **Verify-before-download (M205).** Optional pre-add metadata verification step.
- **Linux packaging (M206).** Debian + RPM + AppImage build matrix.
- **Windows packaging (M207).** MSI + signed `.exe`.
- **macOS packaging (M208).** `.dmg` + notarisation flow.
- **Auto-update framework (M209).** Self-update checker (opt-in, manual trigger only).
- **First-run wizard (M210).** Initial setup walkthrough (later redesigned in M220).

### Power-user features + a11y (M211-M212)

- **Power-user features (M211).** Headless mode entry point, `--config` override flag, structured-log streaming.
- **Accessibility sweep (M212).** Keyboard nav audit across GUI + Web UI; ARIA roles fix-up; high-contrast focus rings.

### Documentation reconciliation (M213 / M233)

- **M213 sweep.** Reconciled README, ROADMAP, CHANGELOG, and design-system.md against the Phase L-N reality. Deferred CHANGELOG body sections for M218-M221 + M223-M232 to a future sweep.
- **M233 (this milestone).** Backfilled the 11 deferred CHANGELOG body sections + refreshed this file's status line.

### Investigation / observability (M221)

- **Tracing instrumentation (M221.1a).** Per-`SessionCommand` `tracing::info_span!` with `queue_wait_ms` + `handler_ms`.
- **Heaptrack baseline (M221.3).** v0.221.0 parallel-7 peak heap = 117.20 MB (6.1× vs v0.173.3 baseline). Full top-30 + observations in `benchmarks/data/v0.221.0_2026-05-26_parallel7_heaptrack/FINDINGS.md`.
- **Flamegraph dispatcher PID fix (M221.4).** Process-tree BFS via new `lib/pid_walk.sh`.

### v1.0 release engineering

- **1.0 release candidate (M222).** Full GUI re-dogfood against the Phase N audit checklist. Version line moves to `v1.0.0-rc1`.
- **Phase O — closing the v1.0 gates (M223-M236).** 14 milestones in the Phase O block:
  - **Engine** (M223 / M224 / M225 / M226 / M228) — POST tail spawn-per-add fix, settings-truth caps, settings-truth timers, settings-truth Class B closure, qBt v2 setPreferences + files completion.
  - **GUI** (M227 / M229) — wire-up sweep, deferral cleanup.
  - **WebUI** (M230 / M231 / M232) — Add Torrent, Sidebar IA, full Preferences.
  - **Docs** (M233) — this milestone.
  - **WebUI tail** (M234 / M235) — design tokens + light theme, responsive + col-resize wiring.
  - **GA** (M236) — `v1.0.0` cut.

## Known invariants & follow-ups (post-M232)

- **HTMX 2.x HX-Trigger payload shape.** When carrying structured detail, **nest** the detail object UNDER the event key (`{"settingsSaved": {"restartPending": [...]}}`), NEVER as a sibling top-level key. HTMX 2.x dispatches one event per top-level key — a sibling payload would split into two events and `ev.detail.<sibling>` would be `undefined` on the listener. See [[project_irontide_htmx2_flat_hx_vals]] and the M232 entry in CHANGELOG.
- **Enum slugs MUST be one of the documented variants.** `encryption_mode` (Disabled / Enabled / PreferPlaintext / Forced) and `preallocate_mode` (None / Sparse / Full) wire-format slugs are exhaustive — `parse_*` helpers return `Option<T>` and `apply()` returns `Err` on `None`. Empty enum slugs in form posts are a client bug (HTML `<select>` cannot submit empty unless the consumer tampers via dev-tools).
- **Numeric form fields are `<input type="number">` with `required`.** Empty-numeric submission is blocked by the rendered HTML; if a hand-crafted POST sends an empty numeric, Axum's `serde_urlencoded` extractor returns 400. **Follow-up:** consider a custom `Deserialize` shim that accepts empty as "no change" — tracked for M234 if cheap.
- **CSRF reverse-proxy semantics.** `crates/irontide-api/src/routes/qbt_v2/security.rs` uses `resolve_client_ip` + `proxies.iter().any(|net| net.contains(&client_ip))`. Outside-voice review of M232 (post-ship) flagged this as a potential bypass when the daemon sits behind a proxy that doesn't strip `X-Forwarded-For`. **Follow-up:** dedicated security-only commit during M234 (or held to a security-only release commit before the v1.0.0 GA tag).

## Current status (v1.0.0-rc12 / 2026-05-28)

- **GUI:** Slint desktop surface complete. Detail-pane tabs, sidebar filters, Add Torrent dialog (file/URL/magnet), command palette, full 8-tab Preferences, menu bar with column visibility submenu, full keyboard shortcut parity, window state persistence, first-run wizard with live validators.
- **Web UI:** Pico+HTMX surface complete. Add Torrent dialog (3-tab), sidebar filter IA, full 8-tab Preferences, polling fragment with state/category/tag/tracker filters, OOB swaps for restart banner.
- **qBt v2 compat:** Full surface — auth + app/* + preferences (GET + setPreferences with 31 fields between them) + torrents + sync + transfer + per-file priority/availability + restart-pending semantics.
- **Engine:** POST tail spawn-per-add fix landed. Settings-truth for connection caps + timers + threads + live bans + uTP cap + watched-folder + notifications + network-interface + incomplete/completed paths. Class B closure complete. 17-crate workspace at 3,787 tests passing / 0 failed / 2 ignored.
- **CLI / TUI:** Stable surface, unchanged shape since M167.
- **Packaging:** Linux (deb/rpm/AppImage) + Windows (MSI) + macOS (dmg). Self-update opt-in.

Phase O 11/14 done. Remaining: M234 (design tokens + light theme), M235 (responsive + col-resize), M236 (`v1.0.0` GA cut).

This document should be updated whenever a design-review skill finds a gap, a surface adds a major component, or a shared primitive (state label, formatter, glyph) changes.
