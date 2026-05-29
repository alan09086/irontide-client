# Changelog

All notable changes to `irontide-client` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/); this project
adheres to [Semantic Versioning](https://semver.org/). The client's version line is
independent of the `irontide` engine library it consumes — they share `1.0.1` at the
split point and diverge from here.

## [1.0.1] — 2026-05-29 — Repo extraction: `irontide-client` standalone application (M237b)

First release of `irontide-client` as a standalone repository, extracted from the
single unified IronTide workspace. The engine library is now published to crates.io
and consumed here as a normal dependency. (See the engine repo's CHANGELOG for the
library side of the split, also tagged `v1.0.1`.)

### Extracted
- The five application crates — `irontide-gui` (Slint desktop GUI, the primary user
  surface), `irontide-cli` (command-line client + `irontide daemon`), `irontide-api`
  (qBittorrent-v2-compatible HTTP REST API + Web UI), `irontide-config` (figment
  configuration pipeline), and `irontide-webui-assets` (HTMX + Pico CSS static assets)
  — moved into this repository via `git filter-repo`, preserving full commit lineage
  back through M236 (IronTide v1.0 GA).
- `LICENSE`, `.cargo/config.toml`, and `rustfmt.toml` carried over so the client builds
  and lints identically post-split.

### Changed
- The client now consumes the **published** `irontide = "1.0.1"` family from crates.io,
  declared once in the root `[workspace.dependencies]`. Library edges resolve from the
  registry; app-internal edges (`irontide-api`, `irontide-webui-assets`,
  `irontide-config`) remain path dependencies within this workspace.
- `irontide-api`'s `irontide-session` dev-dependency (the qBt-v2 A9 integration test's
  `test-util` feature) rewired from a sibling path to the published crate.
- `irontide-api` and `irontide-webui-assets` marked `publish = false` — no crate in this
  repository is published to crates.io.

### Boundary
- There is intentionally **no `[patch.crates-io]`** and **no local path to the engine**.
  Every build — yours, a fresh clone's, CI's — resolves the released library. To test
  the client against an unpublished engine change, publish the engine first (a real or
  pre-release version) and bump the `irontide*` pins in the root `Cargo.toml`; there is
  no local override.

### Notes
- Tagging is manual (`git tag v1.0.1`). release-plz is not used: its `release` command
  unconditionally contacts a git-forge API, which is incompatible with this project's
  no-forge-token + Codeberg-primary policy, and the application publishes nothing to
  crates.io, so release-plz offers no ordered-publish value here either.
- Binary-release packaging (GitHub Releases / AUR / `cargo-dist`) for the GUI + CLI is
  deferred — see `TODOS.md`.

[1.0.1]: https://codeberg.org/alan090/irontide-client/src/tag/v1.0.1
