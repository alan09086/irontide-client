# irontide-client — Session Notes

**Session date:** 2026-05-29
**Focus:** **Repo birth.** This repo was created at IronTide **M237b** by extracting the 5
application crates out of the `irontide` workspace (`git filter-repo`, full history
preserved) and rewiring them to consume the **published** `irontide` engine library from
crates.io. This is the inaugural SESSION_NOTES.
**Current version:** `1.0.1` (the app train's own SSOT, independent of the engine henceforth).
**Branch:** `main`. **Clean and green on both remotes — nothing in-flight.**

---

## 1. Session date + focus + current version

`irontide-client` holds the BitTorrent **client application**: `irontide-api`,
`irontide-webui-assets`, `irontide-config`, `irontide-cli`, `irontide-gui`. It consumes
the engine as a normal Rust dependency — `irontide = "1.0.1"` (and sibling lib crates)
from **crates.io**, with **no `[patch.crates-io]`** (pure-published). Created this session
from the eng-reviewed plan
`../irontide/docs/plans/2026-05-28-irontide-m237-repo-extraction.md` (Phase C).

---

## 2. Current task state and progress

**Born complete & pushed.** `git status` on a fresh clone is a clean tree on `main`,
HEAD at `2b2e3b6a`, tag `v1.0.1` (`05c7b8c7`) on both remotes. Nothing is mid-review or
on a feature worktree.

- **Workspace:** 5 crates. Lib edges (`irontide`, `irontide-format`, `irontide-session`,
  `irontide-core`, `irontide-bencode`) resolve from crates.io via `[workspace.dependencies]`;
  app-internal edges (`irontide-api`, `irontide-webui-assets`, `irontide-config`) are
  path deps. **No `[patch.crates-io]` block** — the committed build is the only build.
- **All crates `publish = false`** — this repo never publishes to crates.io.
- **CI:** `.github/workflows/ci.yml` mirrors the lib's test job (fmt-check + clippy
  `-D warnings` + `cargo test --workspace`, `CARGO_BUILD_JOBS=2`) plus the GTK/Slint/X11
  system-dep install the GUI needs. No release/publish workflow (GH-Actions minutes restricted).
- **This session's uncommitted delta:** `.cgcignore` (added in Phase D for CGC hygiene)
  + this `docs/SESSION_NOTES.md`. Both land in the Phase D commit.

---

## 3. What happened this session

1. **Extraction (Phase C/C1):** fresh `--no-local` clone of `irontide` → `git reset --hard`
   to the `m237-prep` Phase-A tip (app crates present at tip) → stripped other refs →
   `git filter-repo --path` for the 5 crate dirs + `LICENSE` + `.cargo/config.toml` +
   `rustfmt.toml`. 372 commits of app-crate lineage preserved back through M236 / v1.0 GA.
2. **Rewire (Phase C/C2-C3):** authored the pure-published workspace root (lib pins from
   crates.io, app-internal path deps, **no patch block**, `[workspace.lints.clippy]`
   copied from the lib); flipped each app crate's lib edges to `{ workspace = true }`.
   Cold build + test + clippy against the **published** `irontide 1.0.1` → **ALL_GREEN**.
3. **CI + changelog (C4):** wrote `ci.yml` + a fresh Keep-a-Changelog `CHANGELOG.md`
   (`## [1.0.1] — 2026-05-29 — Repo extraction`).
4. **Remotes + tag (C5) — ⛔ owner-signed-off "you create and push both":** created the
   empty repos (Codeberg `alan090` HTTP 201, GitHub `alan09086` via `gh`), pushed `main`
   (`2b2e3b6a`) + annotated `v1.0.1` (`05c7b8c7`) to both. Verified byte-identical.
5. **Phase D ripple:** seeded `TODOS.md` (binary-packaging — gitignored, D6); added
   `.cgcignore`; indexed into CGC (159 files, 2294 functions — sane); this SESSION_NOTES.

---

## 4. Decisions made with reasoning

| Decision | Why | Consequence |
|---|---|---|
| **Pure-published — no `[patch.crates-io]`** | A commented-out patch invites the "forgot to re-comment → silently build against a local engine" hazard. Pure-published guarantees every build (yours, CI, fresh clone) is on a real released engine. | To test against an unpublished engine change, **publish the engine first** (real or pre-release) and bump the pin here. No local override exists. |
| **App train versions independently of the engine** | Post-split the app and engine evolve on separate cadences; coupling their version numbers would be artificial. | `[workspace.package].version` here is the app's own SSOT. It happens to start at `1.0.1` (matching the extraction point) but moves on its own. |
| **No crates.io publish, ever (`publish = false`)** | This is an application, not a library — nothing downstream consumes it as a crate. | Releases here are git tags + (eventually) binary artifacts, never crates.io. |
| **Plain `v{version}` tags (not `app-v…`)** | Separate repos already disambiguate the two trains. | One-line `release-plz.toml`/tagging change if cross-repo tag disambiguation is ever wanted. |
| **release-plz dropped** | Its `release` command unconditionally calls a git-forge API (no-forge-token policy); and this repo publishes nothing, so release-plz offered no ordered-publish value here. | Tag manually (`git tag -a`). No `release-plz.toml`. |

---

## 5. Code changes with intent

| File / area | What | Why |
|---|---|---|
| `Cargo.toml` (root) | Pure-published workspace: lib pins from crates.io, app-internal path deps, **no patch block**, clippy lints copied | The boundary the whole split exists to create — provably on a published engine. |
| `crates/*/Cargo.toml` | Lib edges → `{ workspace = true }` (now crates.io-backed); `publish = false` on api + webui-assets | App consumes the engine like any Rust dep; nothing here is a publishable crate. |
| `.github/workflows/ci.yml` | Mirror lib test job + GUI system deps; no release job | Identical lint/test gate post-split; GH-Actions minutes restricted. |
| `CHANGELOG.md` | Fresh `1.0.1` extraction entry | App-side record of the split. |
| `.cgcignore` | `benchmarks/`, `**/*.min.js`, `target/` (+ CGC defaults) | `webui-assets` bundles minified htmx — keep CGC indexing Rust, not vendored JS. |
| `TODOS.md` *(gitignored)* | Binary-release packaging entry (D6) | The GUI/CLI binaries live here; distribution is the obvious next gap. |

---

## 6. User-provided context worth preserving

- **Dual remotes:** `origin` = Codeberg (`alan090`), `github` = GitHub (`alan09086`).
  Push to **both** on every push.
- **Pure-published is a hard rule** — never add `[patch.crates-io]`. Engine changes reach
  this app only via a crates.io publish (in the `irontide` repo) + a pin bump here.
- **Memory candidate (promote):** `project_irontide_repo_split` — the lib/app split, the
  pure-published boundary, and the publish-then-bump workflow apply to every future session
  touching either repo.
- The engine lib's own handoff lives in `../irontide/docs/SESSION_NOTES.md`.

---

## 7. Open questions and blockers

**BLOCKERS:** none.

**OPEN QUESTIONS (deferred, non-blocking):**
- **Binary-release packaging** (the `TODOS.md` entry) — `cargo-dist` vs a scoped release
  workflow; must override `target-cpu=native` in `.cargo/config.toml` for portable
  artifacts; per-platform matrix (linux/macos/windows) + the GUI's GTK/Slint/X11 deps.
- Tag prefix stays plain `v{version}` unless cross-repo disambiguation is wanted later.

---

## 8. Clear next steps in order

**IMMEDIATE:** none required — the repo is green and released at `1.0.1`.

**WHEN THE ENGINE SHIPS A NEW VERSION:** bump the `irontide`/sibling pins in
`[workspace.dependencies]` to the new published version, run the full gate
(`cargo build/test/clippy --workspace`), then tag + push both remotes. This is the
normal "consume a new engine release" loop — there is no local path to shortcut it.

**FIRST DEDICATED MILESTONE (recommended): binary-release packaging.** Pick `cargo-dist`
or a scoped per-platform release workflow; override `target-cpu=native`; produce
installable GUI + CLI artifacts. Reference the `TODOS.md` entry. Recommended skill:
`superpowers:writing-plans` then `/milestone-cycle` (SLUG `alan090-irontide-client`).

---

## 9. Commits this session (app repo)

1. `build(M237b): irontide-client app workspace — consumes irontide 1.0.1 from crates.io` (**`2b2e3b6a`**, C4)
2. *(Phase D commit — `.cgcignore` + this `docs/SESSION_NOTES.md` — pending at time of writing)*

Plus annotated tag **`v1.0.1`** (`05c7b8c7`), pushed to both remotes.

---

## 10. For the next Claude picking this up

**Read in this order:**

1. **This file** — the repo is the app side of the M237b split; consumes the published
   `irontide`; pure-published (never add `[patch.crates-io]`); next dedicated work is
   binary packaging.
2. **`/mnt/CYBERDECK_01/projects/CLAUDE.md`** — the `irontide-client/` row + cross-project
   Rust commands. Plus `~/.claude/CLAUDE.md` (global): dual-remote push, CGC-first,
   handoff discipline, verify-HEAD-before-commit.
3. **`TODOS.md`** (gitignored, local) — the binary-packaging scope.
4. **`../irontide/docs/SESSION_NOTES.md`** — the engine side; how/when a new engine
   version gets published (which is what you'd bump the pins to).
5. *(For the split mechanics)* `../irontide/docs/plans/2026-05-28-irontide-m237-repo-extraction.md`.

**Cue to start the next work:** *"To package binaries: invoke `superpowers:writing-plans`,
reference `TODOS.md`'s binary-release entry, decide cargo-dist vs a release workflow, and
remember `.cargo/config.toml`'s `target-cpu=native` must be overridden for portable
artifacts."*

---

**`irontide-client` is the downstream consumer of the `irontide` engine library — born at
M237b from a history-preserving extraction, building green against the published
`irontide 1.0.1`, with a pure-published boundary that guarantees every build is on a real
engine release.**
