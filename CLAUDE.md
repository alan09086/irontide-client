# CLAUDE.md — irontide-client

Guidance for Claude Code working in **`irontide-client`**, the **application half** of the
IronTide project. The BitTorrent **engine library** lives in the separate
[`irontide`](https://codeberg.org/alan090/irontide) repo (12 crates, published to crates.io);
this repo is the **client app** that consumes it. Cross-project conventions live in
`/mnt/CYBERDECK_01/projects/CLAUDE.md` and `~/.claude/CLAUDE.md`.

## What this repo is

Five application crates, extracted from the unified IronTide workspace at **M237b**
(2026-05-29) and rewired onto the published engine:

| Crate | Role |
|---|---|
| `irontide-gui` | Slint desktop GUI — the primary user surface |
| `irontide-cli` | command-line client + `irontide daemon` |
| `irontide-api` | qBittorrent-v2-compatible HTTP REST API + WebUI |
| `irontide-config` | figment configuration pipeline |
| `irontide-webui-assets` | HTMX + Pico CSS static assets |

## The pure-published boundary (hard rule)

This app builds against the **published** `irontide` from crates.io — **never add
`[patch.crates-io]`, and never point an engine edge at a local path.** Every build (yours, CI,
a fresh clone) must resolve the released engine. To consume an engine change: publish the
engine (in the `irontide` repo), then bump the `irontide*` pins in the root `Cargo.toml` and
run the full gate. There is no local override. Rationale: `docs/SESSION_NOTES.md` + `CHANGELOG.md`.

## Build, test, lint

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # zero warnings enforced
cargo build --release
cargo run -p irontide-cli            # CLI / daemon
cargo run -p irontide-gui            # Slint desktop GUI
```

The GUI needs GTK/Slint/X11 system deps (mirrored in CI). CI is **test-only by policy** (no
release job — GH-Actions minutes restricted; binary packaging is a Phase 2 roadmap milestone).

## Roadmap & design

- [`ROADMAP.md`](ROADMAP.md) — the client roadmap. Starts fresh at **M1**; the headline arc is
  the **visual redesign** (Phase 1). M-numbering does **not** carry over from the pre-split
  unified project — that history is lineage (git log + `CHANGELOG.md`).
- [`docs/design/`](docs/design/) — the **single locked visual redesign** source of truth
  (emerald-dark, KDE-native, FOCUSED): `DESIGN.md` (GUI spec), `DESIGN-SYSTEM.md`,
  `CODEBASE-CLEANUP.md` (the M1 drift-removal Definition-of-Done, §10). Relocated here from the
  engine repo at the split — they describe **this** repo's code. `styles/tokens.css` is a
  static snapshot (the OKLCH codegen that produced it was retired, not carried over).
- Engine arc this client consumes: [`../irontide/ROADMAP.md`](../irontide/ROADMAP.md)
  (Phase P, M240–M250).

## Session handoff

Non-trivial sessions end by refreshing [`docs/SESSION_NOTES.md`](docs/SESSION_NOTES.md):
current state, decisions with reasoning, code changes with intent, open questions/blockers,
ordered next steps. Pair it with a cross-doc audit (README, CHANGELOG, this file, ROADMAP).

## Remotes

`origin` = Codeberg (`alan090`), `github` = GitHub (`alan09086`). Push to **both** on every
push; never force-push `main`. Codeberg occasionally 504s — retry `origin`, then re-verify
3-way sync (`main` ≡ `origin/main` ≡ `github/main`).
