# Smocka

A multiplayer party game in the style of *Pummel Party*, built with [Bevy](https://bevyengine.org/) and [Lightyear](https://github.com/cBournhonesque/lightyear). Players gather in a shared session and play a series of short, self-contained minigames against each other.

> **Status:** early development. A main menu hosts or joins a session. In the lobby the host
> picks a minigame once at least two players are there. Racing can be picked but isn't playable
> yet. The first minigame is two-player Pong: both
> players move their paddles, the ball bounces off the top and bottom walls and the paddles, and
> a ball that gets past a paddle is a goal for the other player. The first to 5 goals wins; the
> results are shown, then everyone goes back to the lobby.

## Requirements

- Rust (edition 2024)
- Bevy `0.20.1` and Lightyear `0.31` (pinned in `Cargo.toml`)
- Bevy's native dependencies for your platform (see the
  [Bevy setup guide](https://bevyengine.org/learn/quick-start/getting-started/setup/))

## Running

```sh
cargo run
```

This opens the main menu. One player picks **Host game**. The others type the host's address
(for example `192.168.1.10`, or `192.168.1.10:5000`) and press **Join**, or `Enter` in the
field. The field starts as `127.0.0.1:5000`, a host on the same machine. The host listens on port
5000 on every network interface (`HOST_ADDR` in `src/config.rs`), so players on the same network can join if
the firewall allows UDP on that port.

Everyone who joins waits in the lobby, which lists the players. When at least two are there, the
host picks a minigame: **Pong** or **Racing**. In Pong the two lowest-numbered players get the
paddles, and anyone else watches.
Press `Esc` in a session (or close the window) to leave. When a player leaves, everyone else sees
a short "Player N left." notice. If the host leaves, everyone goes back to the menu.

To skip the menu while developing:

```sh
cargo run -- host                # host straight away
cargo run -- join                # join a host on this machine
cargo run -- join 192.168.1.10   # join another machine (port 5000 unless given)
```

## Architecture

Every process runs both the Lightyear client and server plugins. Whether it acts as host or
client is decided by queuing one of these commands (the menu does it for you):

- `HostSession` starts a UDP server and a local client linked directly to it.
- `JoinSession` connects to a host over UDP using netcode authentication. Each join uses a new
  local port, so a host never mistakes a returning player for their old connection. If the
  netcode client can't be set up, it returns an error and changes nothing but `SessionEndReason`,
  so the app stays in the menu and shows why.
- `LeaveSession` goes back to the menu. Entering the menu disconnects the client and, on the
  host, stops the server, which disconnects everyone else.

The host is authoritative: it spawns and replicates game state to all clients.

```
src/
├── main.rs          Binary: picks host or join from CLI args
├── lib.rs           SmockaPlugin, which wires up the plugins below
├── app_state.rs     AppState: main menu, connecting or in a session (local to each app)
├── config.rs        Network, display (640×360 view) and minigame constants
├── camera/          Main 2D camera
├── network/         Lightyear setup and the HostSession / JoinSession commands
│   ├── client/      Client-side config (input delay)
│   ├── server/      Server-side handling of new client links
│   └── transport.rs HostSession / JoinSession / LeaveSession
├── lobby/           Player: one replicated entity per connected client
├── menu/            Main menu, connecting screen, in-session hints and notices (UI)
├── session/         SessionPhase state: the host decides it, every peer follows
└── minigames/       One module per minigame
    ├── two_player_pong/
    ├── racing/      Not playable yet
    └── four_player_pong/
assets/
└── minigames/       Sprites per minigame (PNG)
```

### Plugins

`SmockaPlugin` adds, in order:

1. `NetworkPlugin` - Lightyear client and server plugins, tick rate (128 Hz) and replication
   send interval.
2. `AppStatePlugin` - the local `AppState` (see below).
3. `LobbyPlugin` - the host spawns a replicated `Player` for each client that connects (numbered
   in joining order, so the host is Player 1) and despawns it when the client disconnects. Every
   peer still in the session then gets a `PlayerLeft` event, which the menu shows as a notice.
4. `SessionPlugin` - the `SessionPhase` state (lobby, a minigame, or results). See below.
5. `MinigamesPlugin` - registers each minigame's replicated components and inputs. The
   `Minigame` enum lists the minigames the lobby offers and the `SessionPhase` each is played in.

Replicated components must be registered in the **same order on every peer**, so keep this plugin order intact. `CameraPlugin`, `MinigameVisualsPlugin` (sprites) and `MenuPlugin` (UI) are added separately by the binary so tests can run headless.

### App state

`AppState` is local to each app and never replicated: `MainMenu`, `Connecting` or `InSession`.
The network drives it. `HostSession` and `JoinSession` go to `Connecting`, and the app's client
connecting goes to `InSession`. The client disconnecting, or `LeaveSession`, goes back to
`MainMenu`. If that wasn't the player's choice (the host left, the connection was lost, or nobody
answered), `SessionEndReason` says why and the menu shows it.

### Session phase

`SessionPhase` (`Lobby`, a minigame such as `TwoPlayerPong` or `Racing`, or `Results`) is a Bevy sub-state of
`AppState::InSession`: it only exists while in a session, and leaving the session exits it like
any other state change. Bevy states can't be replicated, so the host keeps its decision in a
replicated `CurrentPhase` component, and every peer (host included) copies that into its local
`SessionPhase`.

Only the host changes the phase:

```
Lobby ──StartMinigame(game)──▶ game.phase() ──MinigameEnded::Won──▶ Results ──5 s──▶ Lobby
                                    └──────────MinigameEnded::Abandoned─────────────▶ Lobby
```

- `StartMinigame(game)` (one of the host's start buttons) leaves the lobby for that `Minigame`'s
  phase once at least `MIN_PLAYERS` (2) are connected.
- A minigame ends itself by triggering `MinigameEnded` on the host: `Won(MinigameResult)` shows the
  replicated result during `Results`, and `Abandoned` (for example, a player left) goes straight
  back to the lobby.
- `Results` returns to the lobby after `RESULTS_DURATION`.

Minigame code only uses ordinary state tools, the same way on every peer:

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

1. Add a variant for it to `SessionPhase` (and to `is_minigame`) and to `Minigame` (with its
   `phase`, its `name` for the lobby button, and `Minigame::ALL`). Create a module under
   `src/minigames/` with its own plugin. Spawn, gate and despawn its entities with that state
   (see *Session phase*).
2. Register its replicated components (`app.component::<T>().replicate()`, plus `.predict()` /
   `.add_linear_interpolation()` where needed) and its input type.
3. On the host, trigger `MinigameEnded::Won` when someone wins and `MinigameEnded::Abandoned` if
   the game can't continue (for example, a player it needs has left).
4. Add the plugin to `MinigamesPlugin`.
5. Put sprites in a separate visuals plugin, add it to `MinigameVisualsPlugin`, and put the art in
   `assets/minigames/<name>/`.

See `two_player_pong` for an example with predicted, interpolated paddles and native Lightyear
input. Its `move_paddles` system shows the pattern for input-driven movement: it runs in
`FixedUpdate`, reads `ActionState<PongInput>`, and only changes `PaddlePosition`. That way the
host runs it for every paddle, each client runs it for its own predicted paddle, and Lightyear
can re-run it during rollback.

Pong's game logic runs in a fixed order each tick, set by the `PongSystems` system set: paddles
move first, then the ball (so it collides with this tick's paddles), then scoring. The host
alone moves the ball and keeps the score: when the ball leaves through the left or right edge, it
adds a goal to the replicated `PongScore` and serves the ball again from the centre, towards the
player who conceded. The first to `PongRules::goals_to_win` (5 by default, `GOALS_TO_WIN`) wins;
the winning goal isn't re-served, and no more goals count once the game is won.
If a paddle's player leaves, the game is abandoned. Each paddle hit makes the ball faster (`BALL_HIT_SPEEDUP`, up to
`BALL_MAX_SPEED` in `src/config.rs`), and every serve starts at the serve speed again.

Where the ball hits a paddle decides where it goes, so players can aim: the centre of the paddle
sends it straight back across, and the ends send it off at up to `BALL_MAX_BOUNCE_ANGLE` (37°)
up or down.

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
