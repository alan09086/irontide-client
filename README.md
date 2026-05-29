# irontide-client

Desktop + CLI BitTorrent client built on the [`irontide`](https://crates.io/crates/irontide) engine.

This repository holds the **application** half of the IronTide project (extracted from the
engine at M237b): five crates that consume the published `irontide` library as a normal
crates.io dependency.

| Crate | What it is |
|---|---|
| `irontide-gui` | Slint desktop GUI (primary user surface) |
| `irontide-cli` | `irontide` command-line client + `irontide daemon` (HTTP API host) |
| `irontide-api` | qBittorrent-v2-compatible HTTP REST API + Web UI |
| `irontide-config` | Shared configuration pipeline (figment TOML/env/CLI merge) |
| `irontide-webui-assets` | Vendored Web UI static assets (HTMX + Pico CSS) |

## Library dependency — published, not local

The client builds against the **published** `irontide` from crates.io (pinned in
`[workspace.dependencies]`). There is intentionally **no** `[patch.crates-io]` and no
local path override: every build — yours, a fresh clone's, CI's — resolves the same
released engine. To test the client against an unpublished engine change, publish the
engine first (a real or pre-release version), then bump the `irontide*` pins in the root
`Cargo.toml`.

The engine lives in its own repository: <https://codeberg.org/alan090/irontide>.

## Build

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The GUI needs GTK / Slint / X11 system libraries (see the engine repo's build notes).

## License

GPL-3.0-or-later
