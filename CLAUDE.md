# CLAUDE.md

Multiplayer party game (in the style of Pummel Party) built with **Bevy** and **Lightyear**.
A shared "hub"/board flow hosts many short, self-contained minigames.

## Versions — read this first
- Bevy: `0.20.1` · Lightyear: `0.31` (see `Cargo.toml`)
- Both crates make breaking API changes between releases. Do NOT write code from memory of older versions. Check the exact version in `Cargo.lock`, and look at the matching docs.rs pages or the examples in `~/.cargo/registry/src/*/lightyear-*/` / `bevy-*/` before using an API.
- If something doesn't compile, suspect an API change before trying other workarounds.

## Verification & Quality Checks
Before considering any task complete, verify that both lints and tests pass cleanly:
- **Clippy:** Run `cargo clippy --all-targets -- -D warnings` and resolve all errors and warnings before finishing.
- **Tests:** Run `cargo test` and ensure the entire test suite passes.
- **Format:** Run `cargo fmt` to ensure everything is formatted propertly.
- **Documentation:** Maintain clear rustdoc comments (`///` and `//!`) on modules, public types, components, and systems, and keep `README.md` updated whenever architecture, usage, or features change. Run `cargo doc --no-deps` to verify there are no broken doc links or warnings.
