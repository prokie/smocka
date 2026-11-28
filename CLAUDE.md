# CLAUDE.md

Multiplayer party game built with **Bevy** and **Lightyear**. A party is several short,
self-contained minigames played back to back, with points adding up to one winner. There is no
board game between minigames (unlike Pummel Party), and none is planned. Be concise.

## Versions — read this first
- Bevy: `0.20.1` · Lightyear: `0.31` (see `Cargo.toml`)
- Both crates make breaking API changes between releases. Do NOT write code from memory of older versions. Check the exact version in `Cargo.lock`, and look at the matching docs.rs pages or the examples in `~/.cargo/registry/src/*/lightyear-*/` / `bevy-*/` before using an API.
- If something doesn't compile, suspect an API change before trying other workarounds.

## Verification & Quality Checks
Before considering any task complete, verify that all checks pass cleanly:
- **Clippy:** Run `cargo clippy --all-targets -- -D warnings` and resolve all errors and warnings before finishing.
- **Tests:** Run `cargo test` and ensure the entire test suite passes.
- **Format:** Run `cargo fmt` to ensure everything is formatted properly.
- **Documentation:** Maintain clear rustdoc comments (`///` and `//!`) on public types, components, and non-obvious systems. Run `cargo doc --no-deps` to verify there are no broken doc links or warnings.

## Commit messages
`git-cliff` (`cliff.toml`) builds player-facing release notes from commit subjects, grouped by the first word.

- **Format:** Imperative, capitalized, no trailing period, ≤72 chars. One change per commit (avoid "and").
- **Player-facing:** Describe the visible effect and name the minigame/screen when not obvious (`Fix players joining twice when Enter is held`, not `Fix bug in handle_join`). Put technical details in the body after a blank line.
- **First word sets the section:**
  - `Add` / `Show` / `Let` / `Allow` / `Support` / `Implement` → **Added**
  - `Fix` → **Fixed**
  - `Remove` / `Drop` → **Removed**
  - Anything else (`Improve`, `Make`, `Rename`, `Speed up`, …) → **Changed**
- **Excluded housekeeping:** Start with `Update TODO` or `Update CLAUDE.md` so `cliff.toml` skips them. Start commits that only change tests with `Test` (e.g. `Test hosting on ports the OS picks`) for the same reason.
- Preview with `git cliff --unreleased`.

## Logging
Use Bevy's `info!` / `warn!` / `error!` / `debug!` (from `bevy::prelude`), never `println!`. Adding logs for state changes, endings, and refused requests is part of task completion alongside the checks above.

- **`info!`** — peer lifecycle and state transitions: connect/disconnect, `AppState`/`SessionPhase` changes, player join/leave, minigame start, win (by whom), or abandon.
- **`warn!`** — refused requests or recoverable surprises (e.g. non-host tries to start a minigame).
- **`error!`** — invariants violated (bugs).
- **`debug!`** — gameplay events inside a minigame (spawns, goals, laps) and rejected input (e.g. unparsable address).
- **Nothing (or `trace!`)** — per-tick/per-frame systems (`FixedUpdate` movement, input buffering, transform sync).

Rules:
- **Log on clients too**, not just host authority — each peer's terminal must show what it observed (phase changes followed, disconnect cause).
- **Fire once per event** (on state change or at the trigger/command site), never every frame while a condition holds.
- **Log the technical cause at the trigger site**: log why a minigame fires `MinigameEnded::Abandoned`, and log underlying reasons (e.g. Lightyear's `DisconnectedReason`) alongside friendly UI messages.
- **Include who and what**: `player.name()` and/or `PeerId`, minigame or phase, and values (e.g. `"Session phase {:?} -> {:?}"`).
