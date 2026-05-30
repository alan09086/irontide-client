# IronTide Client Roadmap

**Current:** `v1.0.1` (app) · 5 crates (`irontide-api` / `irontide-webui-assets` / `irontide-config` / `irontide-cli` / `irontide-gui`) · consumes **published** `irontide 1.0.1` from crates.io · **Phase 1 — Visual redesign · M1 OPEN**

**Engine library:** the BitTorrent engine this app drives lives in the separate [`irontide`](https://codeberg.org/alan090/irontide) repo (12 crates, published to crates.io). This roadmap covers the **client application only** — the Slint desktop GUI, the CLI/daemon, and the HTMX WebUI.

> **Why this roadmap starts at M1.** `irontide-client` was born at the **M237b** repo split (2026-05-29) when the 5 application crates were extracted from the unified IronTide workspace and rewired to consume the *published* engine. The engine and the app now version and plan **independently**. The unified project's 236-milestone history (through v1.0.0 GA) is the app's *lineage*, not its roadmap — it lives in this repo's git log and [`CHANGELOG.md`](CHANGELOG.md). Forward client planning begins fresh at **M1**.

---

## Lineage (pre-split)

The IronTide client descends from the unified IronTide project:

- The **Slint desktop GUI**, the **HTMX/Pico WebUI** browser dashboard, and the **qBittorrent-v2-compatible API** were all built in the unified workspace across Phases B–O (M162–M236).
- **v1.0.0 GA** shipped 2026-05-28 — the unified project's terminal release of its 236-milestone arc.
- At **M237b** (2026-05-29) the 5 application crates were extracted by `git filter-repo` (full history preserved) into this repo and rewired onto the **published** engine. The extraction record is in [`CHANGELOG.md`](CHANGELOG.md) (`[1.0.1]`) and [`docs/SESSION_NOTES.md`](docs/SESSION_NOTES.md).

Pre-split GUI/WebUI milestones are **not** re-numbered into this roadmap — they remain recoverable from this repo's git log (`git log -- crates/irontide-gui` etc.). Independent client planning starts at **M1** below.

---

## Visual redesign — the source of truth

The client's headline forward arc is a **single locked visual redesign**: an emerald-dark, KDE-native, focused-scope direction that replaces the inherited multi-skin / multi-theme / multi-density / multi-radius token machinery. The canonical reference lives in this repo under [`docs/design/`](docs/design/) (relocated here from the engine repo at the split — these docs describe *this* repo's GUI/WebUI code):

| Doc | What it is |
|---|---|
| [`docs/design/DESIGN.md`](docs/design/DESIGN.md) | **GUI Specification (v1.0, FOCUSED)** — single window, single theme, single layout; emerald-dark, KDE-native. |
| [`docs/design/DESIGN-SYSTEM.md`](docs/design/DESIGN-SYSTEM.md) | **Design System (v1.0, FOCUSED)** — the locked tokens, components, and density model. |
| [`docs/design/CODEBASE-CLEANUP.md`](docs/design/CODEBASE-CLEANUP.md) | The **M1 drift-removal spec** — directions + Definition-of-Done (§10) for collapsing the multi-skin machinery to the one locked direction. |
| [`docs/design/styles/tokens.css`](docs/design/styles/tokens.css) | A **static snapshot** of the locked design tokens. |
| [`docs/design/archive/`](docs/design/archive/) | The superseded v0.1 multi-skin GUI Design Spec, kept for reference. |

> **Note on `tokens.css`.** It arrives as a **static snapshot**. The OKLCH token-codegen toolchain that originally generated it (a Python writer + a CI token-drift gate) was **retired and deleted at the split — not carried into this repo**. M1's cleanup removes the runtime token CSS/Slint plumbing and reintroduces **no** regeneration toolchain; the locked palette is hand-maintained from here on.

---

## Phase 1 — Visual redesign (M1–M…)

Collapse the inherited multi-skin machinery to the one locked direction, then apply the redesign across both UI surfaces.

| Milestone | Description | Status |
|-----------|-------------|:------:|
| **M1** | **CODEBASE-CLEANUP drift removal.** Implement [`docs/design/CODEBASE-CLEANUP.md`](docs/design/CODEBASE-CLEANUP.md)'s Definition-of-Done (§10) against the GUI as it now lives in this repo: delete the `Skin` / `Theme` / `RadiusPreset` enums; reduce the skin module to **density-only**; bake the final palette into the Slint tokens; delete the OKLCH token-codegen + the CI token-drift gate (already removed at the split — verify gone); Preferences shows **Density** only (no Theme/Skin/Radius controls); KDE/Breeze window chrome; Linux-first filesystem paths; `cargo test -p irontide-gui` green. *(The cleanup spec's file paths — `crates/irontide-gui/src/skin.rs`, `skin_tokens.rs`, `ui/tokens.slint`, `prefs.rs` — predate the split; **locate their current post-split paths before editing**, do not hardcode the pre-split layout.)* | ⏳ |
| **M2+** | **Apply the redesign** across the Slint GUI and the HTMX/Pico WebUI per `DESIGN.md` / `DESIGN-SYSTEM.md` — token wiring, layout, component pass. Broken into milestones at plan-write time. | ⏳ |

---

## Phase 2 — Distribution

Binary-release packaging for the GUI + CLI — the deferred M237b [`TODOS.md`](TODOS.md) item. Today the client ships source + a published-lib dependency only; users cannot yet download and run it.

- Decide **`cargo-dist`** vs a scoped per-platform release workflow.
- Override `.cargo/config.toml`'s `target-cpu=native` for **portable** binaries — it currently targets the build host's CPU.
- Per-platform matrix (linux / macos / windows) + the GUI's GTK/Slint/X11 system deps.
- CI today is **test-only by policy** (no release job; GH-Actions minutes restricted) — weigh a local `cargo-dist` run against a scoped release workflow.

---

## Phase 3 — Feature maturation

GUI + CLI/daemon + WebUI feature parity and polish — a sketch, detailed at plan-write time once Phases 1–2 land. Candidate threads: WebUI ↔ GUI feature parity, daemon/headless ergonomics, and the long-tail UX gaps deferred during the 1.0 push.

---

## Cross-repo coordination — the pure-published boundary

This app builds against the **published** `irontide` engine from crates.io — **no path dependency, no `[patch.crates-io]`**. The boundary is deliberate:

- **Every build** (yours, CI, a fresh clone) is provably on a real released engine.
- **Engine capabilities reach this app only via a crates.io publish + a pin bump here.** To consume an engine change: publish the engine (in the [`irontide`](https://codeberg.org/alan090/irontide) repo), then bump the `irontide` / sibling-crate pins in this repo's root `Cargo.toml` and run the full gate (`cargo build/test/clippy --workspace`).
- There is **no local override** — to test against an unpublished engine change, publish a (pre-)release first, then bump the pin.

**Release cadence (this repo):** the app versions **independently** of the engine; plain `v{version}` git tags; **no crates.io publish** (`publish = false` — this is an application, not a library); binary releases begin once Phase 2 lands.

The engine arc this client consumes is tracked in the engine roadmap — [`../irontide/ROADMAP.md`](../irontide/ROADMAP.md) (Phase P — engine de-risking + build velocity + settings truth, M240–M250).

---

**`irontide-client` is the application half of the M237b split** — the Slint GUI, the CLI/daemon, and the HTMX WebUI, building against the published `irontide` engine over a pure-published boundary. Its first dedicated arc is the **visual redesign** (Phase 1, M1), collapsing the inherited multi-skin machinery to a single locked emerald-dark direction.
