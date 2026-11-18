# Smocka

A multiplayer party game in the style of *Pummel Party*, built with [Bevy](https://bevyengine.org/) and [Lightyear](https://github.com/cBournhonesque/lightyear). Players gather in a shared session and play a series of short, self-contained minigames against each other.

> **Status:** early development. Networking and the lobby work. In the first minigame
> (two-player Pong), both players can move their paddles and the ball bounces off the walls and paddles
> (there is no scoring yet: the side edges bounce too).

## Requirements

- Rust (edition 2024)
- Bevy `0.20.1` and Lightyear `0.31` (pinned in `Cargo.toml`)
- Bevy's native dependencies for your platform (see the
  [Bevy setup guide](https://bevyengine.org/learn/quick-start/getting-started/setup/))

## Running

One player hosts, the others join. Both currently use `127.0.0.1:5000` (`SERVER_ADDR` in `src/config.rs`).

```sh
# Host a session (runs a server and a local client in the same process)
cargo run

# Join the session from another terminal
cargo run -- join
```

## Architecture

Every process runs both the Lightyear client and server plugins. Whether it acts as host or client is decided at startup by queuing one of two commands:

- `HostSession` starts a UDP server and a local client linked directly to it.
- `JoinSession` connects to a host over UDP using netcode authentication.

The host is authoritative: it spawns and replicates game state to all clients.

```
src/
├── main.rs          Binary: picks host or join from CLI args
├── lib.rs           SmockaPlugin, which wires up the plugins below
├── config.rs        Network, display (640×360 view) and minigame constants
├── camera/          Main 2D camera
├── network/         Lightyear setup and the HostSession / JoinSession commands
│   ├── client/      Client-side config (input delay)
│   ├── server/      Server-side handling of new client links
│   └── transport.rs HostSession / JoinSession
├── lobby/           Player: one replicated entity per connected client
├── session/         SessionPhase state: the host decides it, every peer follows
└── minigames/       One module per minigame
    ├── two_player_pong/
    └── four_player_pong/
assets/
└── minigames/       Sprites per minigame (PNG)
```

### Plugins

`SmockaPlugin` adds, in order:

1. `NetworkPlugin` - Lightyear client and server plugins, tick rate (128 Hz) and replication
   send interval.
2. `LobbyPlugin` - the host spawns a replicated `Player` for each client that connects and
   despawns it when the client disconnects.
3. `SessionPlugin` - the `SessionPhase` state (lobby or a minigame). See below.
4. `MinigamesPlugin` - registers each minigame's replicated components and inputs.

Replicated components must be registered in the **same order on every peer**, so keep this plugin order intact. `CameraPlugin` and `MinigameVisualsPlugin` (sprites) are added separately by the binary so tests can run headless.

### Session phase

`SessionPhase` is a normal Bevy state (`Lobby` or `TwoPlayerPong`). Bevy states can't be
replicated, so the host keeps its decision in a replicated `CurrentPhase` component, and every
peer (host included) copies that into its local `SessionPhase`. If a client loses its connection,
it goes back to `Lobby`.

For now the host plays Pong whenever two or more players are connected, and goes back to the
lobby when fewer are. Minigame code only uses ordinary state tools, the same way on every peer:

- `OnEnter(SessionPhase::TwoPlayerPong)` to spawn things (add `.run_if(is_host)` for replicated
  game entities, which only the host spawns).
- `.run_if(in_state(SessionPhase::TwoPlayerPong))` on systems that should only run during the game.
- `DespawnOnExit(SessionPhase::TwoPlayerPong)` on everything spawned, so it's removed when the
  game ends. On clients, replicated entities disappear when the host despawns them.

### Graphics

The game uses pixel art on a **640×360 virtual screen**: one world unit is one art pixel, and
the camera scales the view to the window. 640×360 scales by whole numbers to 720p, 1080p, 1440p
and 4K. Images use nearest-neighbour filtering so pixels stay sharp.

Art lives in `assets/minigames/<minigame>/`. The current files are flat placeholders. To replace one,
overwrite the PNG with the same name; no code changes are needed. Draw player-coloured parts
(such as paddles) in white or grey, because the game tints them per player.

| File (`assets/minigames/two_player_pong/`) | Size | Used |
|---|---|---|
| `arena.png` | 640×360 | Background |
| `paddle.png` | 8×48 | Paddles, tinted per side |
| `ball.png` | 8×8 | Ball |

### Adding a minigame

1. Add a variant for it to `SessionPhase`, and create a module under `src/minigames/` with its
   own plugin. Spawn, gate and despawn its entities with that state (see *Session phase*).
2. Register its replicated components (`app.component::<T>().replicate()`, plus `.predict()` /
   `.add_linear_interpolation()` where needed) and its input type.
3. Add the plugin to `MinigamesPlugin`.
4. Put sprites in a separate visuals plugin, add it to `MinigameVisualsPlugin`, and put the art in
   `assets/minigames/<name>/`.

See `two_player_pong` for an example with predicted, interpolated paddles and native Lightyear
input. Its `move_paddles` system shows the pattern for input-driven movement: it runs in
`FixedUpdate`, reads `ActionState<PongInput>`, and only changes `PaddlePosition`. That way the
host runs it for every paddle, each client runs it for its own predicted paddle, and Lightyear
can re-run it during rollback.

### Controls (Pong)

`W` / `↑` moves your paddle up, `S` / `↓` moves it down.

## Development

Run these before considering a change done:

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

Integration tests live in `tests/integration/` and are compiled as one crate so Bevy and Lightyear are linked only once. They run real host and client apps headlessly over localhost UDP. Add new test suites as modules in `tests/integration/main.rs`, not as new files directly under `tests/`.
