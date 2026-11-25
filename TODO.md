# TODO

## Racing minigame

- [x] **1. Pick the minigame** — `Minigame` enum, one lobby button per game, `SessionPhase::Racing`.
- [x] **2. Drivable cars** — `Car` / `CarState` (predicted + interpolated), `RacingInput`, `drive()`.
- [x] **3. Track** — oval in `track.rs`, `surface_at()`, grass slowdown, grid, `track.png`, `F3` gizmos.

### 4. Checkpoints and laps (host)
Goal: the game knows which lap each car is on.

- [ ] `track.rs`: an ordered list of checkpoints across the road (e.g. start line, right bend, top
      straight, left bend), with checkpoint 0 as the start/finish line, plus
      `checkpoint_at(pos) -> Option<usize>`.
- [ ] `LAPS` (3) in `config::minigames::racing`.
- [ ] `RaceProgress { next_checkpoint, laps }` on each car: replicated, **not** predicted.
- [ ] Host system in a new `RacingSystems::Checkpoints` set, after `Drive`: reaching
      `next_checkpoint` moves on to the next one; reaching 0 after all the others counts a lap.
- [ ] HUD: "Lap 2/3" for your own car.
- [ ] `debug!` each lap: who, and which lap.
- [ ] Show the checkpoints in the `F3` gizmos.
- [ ] Tests: unit tests that driving the checkpoints in order counts a lap, and skipping one or
      going backwards doesn't.

Watch out for:
- Cars start **behind** the start line, so crossing it the first time must not count as a lap.
- Checkpoints must be thicker than a car moves in one tick (`MAX_SPEED / TICK_RATE` ≈ 1.6
  units). 16 or more is safe.

### 5. Start and finish
Goal: a whole race from countdown to results.

- [ ] Countdown: a host-owned, replicated `RaceStatus` (`Countdown` → `Racing`). `drive_cars`
      ignores throttle until `Racing`, and the HUD shows 3-2-1-GO.
- [ ] Finishing: completing `LAPS` gives the car the next place. Log each finish with `info!`.
- [ ] End when every car has finished, or `FINISH_GRACE` (~10 s) after the first finish. Then
      trigger `MinigameEnded::Won` with 1st place as winner and the full order in `summary`.
      Cars that didn't finish go last, ordered by progress.
- [ ] Players leaving: their car disappears; keep racing. Trigger `Abandoned` only when no cars
      are left, with an `info!` saying why at the trigger site (see CLAUDE.md).
- [ ] Integration test: the host moves a car to just before the line on its last lap, and both
      peers reach `Results` with the right winner.

Known limitation: with prediction, the joiner's car can jump forward a little when the race starts.
Acceptable for now (see *Later*).

### 6. Polish
- [ ] HUD: live race positions (1st, 2nd…).
- [ ] Tune the handling in `config::minigames::racing` until driving feels good.
- [ ] Better art: car, track, kerbs. Optionally skid marks and sounds.
- [ ] README: update the status line and controls.

## Netcode and engine

APIs below were checked against the Lightyear 0.31 / Bevy 0.20.1 sources in `~/.cargo/registry`.

- [ ] **Don't mark unchanged state as changed** (quick win). Replication sends components that
      Bevy change detection marks as changed. `drive_cars` writes `*state = drive(...)` and
      `move_paddles` writes `position.0.y` every tick, even for a parked car or an idle paddle.
      Use `set_if_neq` instead (as `style_buttons` in `src/menu.rs` already does).
- [ ] **Smooth rendering between ticks, and blend out rollback snaps.** The `sync_*_transform`
      systems copy the latest fixed-tick state into `Transform`. At 128 Hz on a 60/144/165 Hz
      screen that can stutter a little. Calling `.add_correction()` on the predicted
      `PaddlePosition` and `CarState` eases mispredictions out over a few frames instead of
      snapping, and adds `FrameInterpolationPlugin` by itself (`lightyear_prediction`
      `registry.rs`). Entities opt in to frame interpolation with the `FrameInterpolate` marker.
      `add_correction` needs the component to implement `Default` and `Diffable`, which
      `CarState` and `PaddlePosition` don't yet. Insert `SkipFrameInterpolation` on intentional
      teleports, such as Pong's ball going back to the centre after a goal, so it doesn't smear
      across the screen.
- [ ] **Test under bad network conditions.** Everything is tested over clean localhost.
      `Link::with_conditioner(LinkConditionerConfig { .. })` (`lightyear_link`) adds latency,
      jitter and packet loss. Add a CLI flag (e.g. `--lag`) for manual play, and one
      integration test with a conditioned link.
- [ ] **Reconsider `TICK_RATE` (128 Hz) and `SEND_INTERVAL` (64 Hz)** once frame interpolation is
      in. 64 Hz ticks and a 20–32 Hz send rate would roughly halve CPU and bandwidth. Measure
      first. A lower tick rate also tightens two limits that are written in terms of it: the ball's
      tunnelling limit (`BALL_MAX_SPEED`, see its doc in `src/config.rs`) and how thick racing
      checkpoints must be. A lower send rate makes other players' cars and paddles show slightly
      later.
- [ ] **Lightyear's debug tools, when chasing a desync**: `lightyear_tools` has a structured
      tracing layer and a debug UI covering sync, input buffering, rollbacks and correction.
      Check which `lightyear` feature enables it.
- [ ] **Compile only the 2D parts of Bevy**: `default-features = false, features = ["2d", "ui",
      "file_watcher"]` (add `"audio"` back with sound) skips the 3D renderer and cuts clean build
      time and binary size. Keep `file_watcher`: it's what makes edited PNGs reload in a running
      game.
- [ ] **Crisp pixel art at any window size**: the camera's `ScalingMode::AutoMin` scales by
      fractional amounts in windowed mode, so pixels come out uneven and rotating cars shimmer.
      Either snap the zoom to whole multiples of 640×360, or render the game to a 640×360 texture
      and scale that up (needs a second camera; menus can stay at full resolution).

## Later

- [ ] **Gamepads and rebindable controls**: replace the hard-coded `KeyCode`s in
      `pong_input.rs` and `racing_input.rs` with `bevy_enhanced_input`. Lightyear supports it
      through its `lightyear_inputs_bei` crate. Matters more with every minigame added.
- [ ] **Find games on the local network**: the host announces itself (UDP broadcast or mDNS),
      and the menu lists games to click instead of typing an IP address.
- [ ] **Sound**: Bevy's built-in `audio` feature is enough for bounces, goals and countdown beeps.
      For engine pitch that follows speed, or separate music/effects volumes, look at
      `bevy_kira_audio` (check it supports Bevy 0.20 first).
- [ ] **Car-to-car bumping**: the hard prediction problem. Do it on its own.
- [ ] **Start on an agreed tick**, so predicted cars don't jump at the start.
- [ ] **More interesting tracks**: describe the track as data (a centre line plus width) instead of
      an oval.
- [ ] **Placements in results**: turn `MinigameResult { winner, summary }` into a full finishing
      order.
- [ ] **Points across games**: a score per `Player`, and a standings screen after each result.
- [ ] **Player limits per minigame**: min and max players on `Minigame`. Pong needs exactly 2.
- [ ] **The board/hub** that picks the next minigame.

## Loose ends

- [x] Track-art generator in the repo: `python3 tools/make_track.py` redraws `track.png`. Keep its
      constants in step with the `TRACK_*` / `GRID_*` ones in `src/config.rs`.
- [x] Delete the empty `src/minigames/four_player_pong` module.
- [ ] README: mention `RUST_LOG=info,smocka=debug cargo run` for the `debug!` logs.
