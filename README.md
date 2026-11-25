# Smocka

A multiplayer party game in the style of *Pummel Party*, built with [Bevy](https://bevyengine.org/) and [Lightyear](https://github.com/cBournhonesque/lightyear). Players gather in a shared session and play a series of short minigames against each other.

> **Status:** early development. Two minigames: two-player Pong (first to 5 goals) and Racing
> (drivable cars, no track or finish yet).

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

## Controls

- **Pong:** `W` / `↑` up, `S` / `↓` down.
- **Racing:** `W` / `↑` accelerate, `S` / `↓` brake and reverse, `A` / `←` and `D` / `→` steer.

## Art

Sprites live in `assets/minigames/<minigame>/` on a 640×360 virtual screen (one world unit is one
pixel). Overwrite a PNG with the same name and size to replace it. Draw player-coloured parts in
white or grey: the game tints them.

## Development

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

Integration tests run real host and client apps headlessly over localhost UDP. Add new suites as
modules in `tests/integration/main.rs`, not as new files under `tests/`.
