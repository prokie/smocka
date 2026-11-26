# Smocka

A multiplayer party game in the style of *Pummel Party*, built with [Bevy](https://bevyengine.org/) and [Lightyear](https://github.com/cBournhonesque/lightyear). Players gather in a shared session and play a series of short minigames against each other.

> **Status:** early development. Two minigames: two-player Pong (first to 5 goals) and Racing
> (3 laps on an oval track, with a countdown and a finishing order).

## Download and play

Download the zip for your system from the
[Releases page](https://github.com/prokie/smocka/releases), unzip it, and run `smocka`
(`smocka.exe` on Windows). Keep the `assets` folder next to it.

- **Everyone needs the same version.** The main menu shows it under the title. A player on
  another version can't join: their join times out.
- The builds aren't signed, so the first launch shows a warning. On **Windows**, click
  *More info → Run anyway*. On **macOS**, right-click `smocka` and choose *Open* (or run
  `xattr -dr com.apple.quarantine smocka` in the unzipped folder). The macOS build is for Apple
  Silicon.
- **Same network** (home Wi-Fi or LAN): join with the host's local address, such as
  `192.168.1.10`.
- **Over the internet:** the host must forward **UDP port 5000** on their router to their
  computer, and the others join with the host's public IP address.

## Requirements

- Rust (edition 2024)
- Bevy's native dependencies for your platform (see the
  [Bevy setup guide](https://bevyengine.org/learn/quick-start/getting-started/setup/))

## Running

```sh
cargo run
```

One player picks **Host game**. The others type the host's address (for example
`192.168.1.10`, or `192.168.1.10:5000`) and press **Join**. The host listens on UDP port 5000 on
every interface. Once at least two players are in the lobby, the host picks a minigame. `Esc`
leaves the session.

To skip the menu while developing:

```sh
cargo run -- host                # host straight away
cargo run -- join                # join a host on this machine
cargo run -- join 192.168.1.10   # join another machine (port 5000 unless given)
```

To test two players on one machine, `tools/dev.sh` builds once and starts a host on the left half
of the secondary monitor and a joiner on the right (`SMOCKA_MONITOR=primary tools/dev.sh` for
the primary one); closing the joiner closes both. The halves come from `--left` / `--right`, plus
`--secondary`, which also work on their own. Wayland doesn't let apps place their windows,
so the script runs the game on X11 (XWayland) there.

## Controls

- **Pong:** `W` / `↑` up, `S` / `↓` down.
- **Racing:** `W` / `↑` accelerate, `S` / `↓` brake and reverse, `A` / `←` and `D` / `→` steer.
  `F3` draws the track's road edges over the art.

## Art

The art is flat and stylized (solid colours, bold outlines), drawn for a 1080p screen: 3 texture
pixels per world unit of the 640×360 virtual screen (`ART_SCALE` in `src/config.rs`). Every
sprite is given its size in world units, so an image's resolution never changes gameplay; 4K
screens scale it up and smaller ones down, with smooth filtering.

Sprites live in `assets/minigames/<minigame>/`. To replace one, overwrite the PNG with the same
name, at 3× its size in world units (e.g. a 16×10 car is 48×30 pixels; a full-screen background is
1920×1080), keeping the same proportions. Draw player-coloured parts in white or grey: the game
tints them.

The placeholders come from `python3 tools/make_art.py`. The racing track's art must match the
shape set in `config::minigames::racing` (`TRACK_*` constants), as must the copy of those
constants in the script; press `F3` in a race to check.

## Development

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

GitHub Actions runs these checks on every push to `main` and every pull request
(`.github/workflows/ci.yml`).

Integration tests run real host and client apps headlessly over localhost UDP. Add new suites as
modules in `tests/integration/main.rs`, not as new files under `tests/`.

To also see the game's `debug!` logs (goals, car spawns and the like):

```sh
RUST_LOG=info,smocka=debug,wgpu=error,naga=warn cargo run
```

`RUST_LOG` replaces Bevy's default log filter, so the last two parts keep the graphics libraries
quiet again.

### Making a release

1. Bump `version` in `Cargo.toml` (and commit it, with the updated `Cargo.lock`). The version
   decides who can play together, so every release needs a new one.
2. Tag the commit with the same version and push the tag:
   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```
3. `.github/workflows/release.yml` builds Linux, Windows and macOS zips and publishes them as a
   GitHub Release. It fails if the tag doesn't match the version in `Cargo.toml`.

The release notes are written for you: a download link per system, then every commit since the
previous tag, grouped by its first word (Add…, Fix…, Remove…, everything else under Changed) by
[git-cliff](https://git-cliff.org) using `cliff.toml`. So write commit messages as one-line,
imperative summaries a player could read. To preview the notes before tagging, run
`git cliff --unreleased`.
