# irontide-client — Session Notes

**Latest session:** 2026-05-30 · **Focus:** **Repo split follow-through (docs half).** The
M237b crate-extraction created this repo on 2026-05-29; this session finished the split on the
**documentation** side — authored the client `ROADMAP.md` (fresh at M1, visual-redesign-first),
received the `docs/design/` tree relocated from the engine repo, and added a repo-local `CLAUDE.md`.
**Current version:** `1.0.1` (unchanged — docs/housekeeping only; no code, no publish).
**Branch:** `main`, clean and green on both remotes.

> **Repo origin (2026-05-29, M237b):** this repo was born by extracting the 5 application crates
> from the unified `irontide` workspace (`git filter-repo`, full history preserved) and rewiring
> them onto the **published** `irontide` engine from crates.io. The birth details (extraction
> mechanics, the pure-published decision, version-independence) are in `CHANGELOG.md` (`[1.0.1]`)
> and the engine plan `../irontide/docs/plans/2026-05-28-irontide-m237-repo-extraction.md` (Phase C).

---

## 1. Current state

- **Workspace:** 5 crates (`irontide-api`, `irontide-webui-assets`, `irontide-config`,
  `irontide-cli`, `irontide-gui`). Lib edges resolve from crates.io (`irontide = "1.0.1"` +
  siblings); app-internal edges are path deps. **No `[patch.crates-io]`** — pure-published.
- **Green** — builds/tests/clippy pass against the published `irontide 1.0.1`.
- **Docs now present:** `ROADMAP.md` + `docs/design/` + `CLAUDE.md` (all this session), plus the
  inaugural `CHANGELOG.md` / `README.md` / `TODOS.md`.
- **Nothing in-flight** — no feature worktree, nothing mid-review. The first dedicated code
  milestone (**M1 — CODEBASE-CLEANUP drift removal**) is now *defined* in `ROADMAP.md`, not started.

---

## 2. What happened — 2026-05-30 (split follow-through)

The M237b crate-split moved the code cleanly but left the **roadmap and design docs** describing a
single pre-split world. This session split them to match the two-repo reality:

1. **Received the design docs.** The entire `docs/design/` tree (106 files — `DESIGN.md`,
   `DESIGN-SYSTEM.md`, `CODEBASE-CLEANUP.md`, the `styles/tokens.css` snapshot, components,
   screenshots, `archive/`) was relocated here from the engine repo, where it had been stranded
   since M239. These docs describe **this** repo's GUI/WebUI code (CODEBASE-CLEANUP.md is literally
   addressed to "CODING AGENTS" working on the Slint GUI). The retired OKLCH token-codegen toolchain
   was **not** carried over — it was dropped on the engine side, so `tokens.css` is now a static snapshot.
2. **Authored `ROADMAP.md`** — fresh at **M1**, visual-redesign-first: Phase 1 (M1 CODEBASE-CLEANUP
   drift removal → M2+ apply the redesign), Phase 2 (binary packaging), Phase 3 (feature maturation),
   plus the pure-published cross-repo boundary.
3. **Added a repo-local `CLAUDE.md`** — what the repo is, the pure-published hard rule, build/test
   commands, roadmap/design pointers, handoff + dual-remote conventions.
4. **CHANGELOG `[Unreleased]` docs note** + this SESSION_NOTES refresh.

On the **engine** side (separate repo, same day): the engine `ROADMAP.md` was rewritten engine-only
(pre-split history condensed to a Lineage pointer; Phase P M240–M250 forward detail kept; M239
reclassified as a client tombstone), and the `docs/design/` tree + the retired codegen scripts were
`git rm`'d there. See `../irontide/docs/SESSION_NOTES.md`.

---

## 3. Decisions (this session)

| Decision | Why | Consequence |
|---|---|---|
| **Client roadmap starts at M1, not continuing pre-split M-numbers** | The app and engine plan independently post-split; re-numbering 236 pre-split milestones into the client would be archaeology. | Pre-split GUI history is lineage (git log + `CHANGELOG.md`); forward work is M1+. |
| **Visual redesign is the headline Phase 1** | The single-locked redesign (`docs/design/`) is the client's biggest forward arc, and M1 (CODEBASE-CLEANUP) unblocks it. | Binary packaging — the obvious "ship it" gap — is deferred to Phase 2, behind the redesign. |
| **Design docs relocated, not copied** | They describe client code; keeping a copy in the engine would re-strand them. | Engine keeps no design docs; the engine roadmap points here for visual scope. |
| **Retired OKLCH codegen dropped, not relocated** | The token-codegen toolchain is retired; `tokens.css` ships as a static snapshot; M1 reintroduces no regen toolchain. | The locked palette is hand-maintained from M1 onward. |

---

## 4. Next steps

**IMMEDIATE:** none required — green at `1.0.1`, docs now coherent across both repos.

**FIRST CODE MILESTONE — M1 (CODEBASE-CLEANUP drift removal).** Implement
`docs/design/CODEBASE-CLEANUP.md`'s Definition-of-Done (§10) against the GUI as it now lives here:
delete `Skin`/`Theme`/`RadiusPreset` enums, reduce the skin module to density-only, bake the locked
palette into the Slint tokens, confirm the OKLCH codegen + CI drift gate are gone, Preferences shows
Density only, KDE/Breeze chrome, Linux paths, `cargo test -p irontide-gui` green. **The cleanup spec's
file paths predate the split — locate the current post-split paths of `skin.rs` / `skin_tokens.rs` /
`ui/tokens.slint` / `prefs.rs` first; do not hardcode.** Recommended: `superpowers:writing-plans` →
`/milestone-cycle` (SLUG `alan090-irontide-client`).

**WHEN THE ENGINE SHIPS A NEW VERSION:** bump the `irontide*` pins in `[workspace.dependencies]`, run
the full gate (`cargo build/test/clippy --workspace`), tag + push both remotes. No local path shortcut.

**LATER:** Phase 2 (binary packaging — see `TODOS.md`), Phase 3 (feature maturation).

---

## 5. Commits this session (2026-05-30)

1. `docs: relocate design docs from engine repo (M239 reclassification — these describe client GUI/WebUI code)` — `docs/design/` (106 files) received from the engine. **`b0ccdcbb`**.
2. `docs: author client ROADMAP.md (Phase 1 visual redesign M1, Phase 2 packaging) + repo CLAUDE.md; CHANGELOG/SESSION_NOTES ripple` — this commit (ROADMAP.md + CLAUDE.md + CHANGELOG `[Unreleased]` + these notes).

Both pushed to `origin` (Codeberg `alan090`) + `github` (GitHub `alan09086`). OSS identity
`Alan Gaudet <alan@alangaudet.dev>`, no co-author trailer.

---

## 6. For the next Claude

**Read in order:**
1. **This file** — current state: docs split done, M1 is the first code milestone.
2. **`ROADMAP.md`** — the client arc (Phase 1 visual redesign from M1).
3. **`CLAUDE.md`** (this repo) + `/mnt/CYBERDECK_01/projects/CLAUDE.md` + `~/.claude/CLAUDE.md` —
   conventions (pure-published, dual-remote, handoff, CGC-first).
4. **`docs/design/`** — the visual redesign source of truth; `CODEBASE-CLEANUP.md` is the M1 spec.
5. **`CHANGELOG.md`** + **`../irontide/docs/SESSION_NOTES.md`** — the split record (both halves).

**Cue to start M1:** *"Invoke `superpowers:writing-plans`, reference `docs/design/CODEBASE-CLEANUP.md`'s
DoD §10, locate the current post-split paths of `skin.rs` / `skin_tokens.rs` / `ui/tokens.slint` /
`prefs.rs`, and plan the drift removal as a `/milestone-cycle` run (SLUG `alan090-irontide-client`)."*

---

**`irontide-client` is the application half of the M237b split — Slint GUI + CLI/daemon + HTMX WebUI
on a pure-published engine boundary. The crate split landed 2026-05-29; the docs split (roadmaps +
design relocation) landed 2026-05-30. Next: M1 visual-redesign cleanup.**
