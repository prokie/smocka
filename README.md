# Smocka

A multiplayer party game, built with [Bevy](https://bevyengine.org/) and [Lightyear](https://github.com/cBournhonesque/lightyear). Players gather in a shared session and play a series of short minigames against each other.

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

Type your name at the top of the main menu; it's remembered for next time (in
`smocka/settings.toml` in your system's settings folder, with your controls and volumes). One player picks **Host game**. The others type the host's
address (for example `192.168.1.10`, or `192.168.1.10:5000`) and press **Join**. The host listens
on UDP port 5000 on every interface.

In the lobby, everyone picks a colour: each colour can belong to only one player, and it's used
for your car, your Pong paddle and your score. Once at least two players are there, the host
picks a minigame. `Esc` leaves the session.

To skip the menu while developing:

```sh
cargo run -- host                # host straight away
cargo run -- join                # join a host on this machine
cargo run -- join 192.168.1.10   # join another machine (port 5000 unless given)
cargo run -- host --name=Ada     # play as "Ada" this time, without changing the saved name
cargo run -- join --mute         # play without sound
```

To test two players on one machine, `tools/dev.sh` builds once and starts a host on the left half
of the secondary monitor and a joiner on the right (`SMOCKA_MONITOR=primary tools/dev.sh` for
the primary one); closing the joiner closes both. The halves come from `--left` / `--right`, plus
`--secondary`, which also work on their own. Wayland doesn't let apps place their windows,
so the script runs the game on X11 (XWayland) there. It shows the game's debug logs from both
players (unless `RUST_LOG` is set), each line marked `[host]` or `[join]`, and keeps each
player's log in `logs/host.log` and `logs/join.log` until the next run. Only the host plays sound:
the joiner runs with `--mute`.

## Controls

Keyboard or gamepad. These are the defaults: change them with **Controls** in the main menu.

- **Pong:** `W` / `↑` / left stick up, `S` / `↓` / left stick down.
- **Racing:** `W` / `↑` / right trigger accelerate, `S` / `↓` / left trigger brake and reverse,
  `A` / `←` and `D` / `→` / left stick steer. `F3` draws the track's road edges over the art.

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
shape set in `src/minigames/racing/config.rs` (`TRACK_*` constants), as must the copy of those
constants in the script; press `F3` in a race to check.

## Sound

Sounds are 16-bit WAV files next to each minigame's art in `assets/minigames/<minigame>/`, plus
`assets/minigames/minigame_won.wav` for the jingle when any minigame is won. To replace one,
overwrite the WAV with the same name. `racing/engine.wav` loops, and the game plays it faster the
faster your car goes, so it should be a short, steady hum that loops without a click.

The placeholders come from `python3 tools/make_sounds.py`, which needs nothing outside Python's
standard library.

**Sound** in the main menu sets three volumes: master, effects and music. They change sounds
already playing too, and are saved with the other settings. In code, play every sound with
`sound::play_sound` (or spawn it with a `sound::Sound`) so it follows them.

There's no music yet, but the game is ready for it: put an Ogg Vorbis file (`.ogg`) under
`assets/`, then queue `PlayMusic(assets.load("music/theme.ogg"))` from `src/sound.rs` where it
should start. It loops until `StopMusic` or the next `PlayMusic`, at the music volume.

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
