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
- [x] **Compile only the 2D parts of Bevy**: `default-features = false, features = ["2d", "ui",
      "file_watcher"]` (add `"audio"` back with sound) skips the 3D renderer and cuts clean build
      time and binary size. Keep `file_watcher`: it's what makes edited PNGs reload in a running
      game.
- [ ] **Decide the art style: pixel art or HD.** Decide early: switching to HD means touching every
      sprite, and that grows with each minigame. Menus and UI text are drawn at full screen
      resolution either way.
  - **Pixel art (current).** 640×360 is exactly 1/6 of 4K (1/3 of 1080p, 1/2 of 720p), so with
    nearest filtering it's already sharp on 4K screens. Remaining rough edge, *crisp pixel art at
    any window size*: the camera's `ScalingMode::AutoMin` scales by fractional amounts in windowed
    mode, so pixels come out uneven and rotating cars shimmer. Either snap the zoom to whole
    multiples of 640×360, or render the game to a 640×360 texture and scale that up (needs a
    second camera).
  - **HD art.** Textures up to 3840×2160, scaled *down* to the player's screen. The world stays
    640×360 units, so gameplay constants don't change. About a day of code; the ongoing cost is
    making HD art for every minigame.
    - `ImagePlugin::default_nearest()` → `default_linear()` in `src/main.rs`.
    - Give every sprite a `custom_size` in world units from the existing constants (arena,
      paddle, ball, car, track), so texture size no longer matters.
    - Mipmaps: without them, scaling 4K art to 1080p or 720p shimmers, especially rotating cars.
      PNGs loaded at runtime get none. Bevy 0.20's asset processor can make them:
      `compressed_image_saver` (in `bevy_image`) has a `generate_mipmaps` setting and writes
      compressed KTX2. Needs Bevy features `asset_processor` + `compressed_image_saver` (needs a
      C++ compiler) to build, and `ktx2` to load. Exact setup not worked out yet.
    - Compression also matters for memory: a full-screen 4K texture is ~33 MB uncompressed
      (~44 MB with mipmaps); BC7 cuts that by about 4×.
    - Redraw the placeholders at 6×: a scale factor and smoothed edges in
      `tools/make_track.py`, and new car, paddle and ball sprites.
    - Check Pong's score (`Text2d`) stays sharp when scaled.
    - Update the README's *Art* section (it says one world unit is one pixel).

## Playing with friends

- [ ] **Downloadable builds from GitHub Actions.** On a version tag (e.g. `v0.1.0`), build for
      Windows, Linux and macOS and attach a zip per platform to a GitHub Release, so friends
      download from the repo's Releases page. There's no CI yet (no `.github/`).
  - Each zip holds the binary plus the `assets/` folder next to it (Bevy looks for `assets/` beside
    the executable). Alternatively, embed the art in the binary with Bevy's `embedded_asset!`.
  - The Linux runner needs Bevy's system packages: `libudev-dev`, `libwayland-dev`,
    `libxkbcommon-dev` (and `libasound2-dev` once audio is back).
  - Windows: add `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` to
    `main.rs`, so release builds don't open a console window next to the game.
  - The builds aren't signed, so Windows SmartScreen and macOS Gatekeeper will warn. Tell
    friends how to get past it ("More info → Run anyway"; right-click → Open). Build macOS for
    Apple Silicon (`aarch64-apple-darwin`), plus Intel if a friend needs it.
  - Cache the build (e.g. `Swatinem/rust-cache`): the release profile uses fat LTO, so it's slow.
  - Starting point: Bevy's community template `bevy_new_2d` has a ready-made release workflow for
    these platforms (and optional itch.io upload) to copy and adapt.
  - Also run `cargo fmt --check`, clippy and the tests on every push.
  - Add a short "Download and play" section to the README.
- [ ] **Refuse other versions of the game.** Everyone must run the same build: another build may
      register replicated components differently and break in confusing ways. `PROTOCOL_ID` in
      `src/config.rs` is always `0`; derive it from the crate version so netcode refuses joins
      from a different build. Show the version in the menu, and suggest "different version?" when
      a join times out.
- [ ] **Playing over the internet.** Joining works on a local network. Over the internet, the host
      must forward UDP port 5000 on their router, which is fine for a first try with friends. To
      avoid port forwarding: a relay server, or Steam networking. Lightyear 0.31 ships
      `lightyear_steam` and `lightyear_p2p` crates (not looked into yet).

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
      Needed by tournaments (below).
- [ ] **Tournaments for minigames with fewer seats than players.** Pong takes 2 players, so with
      more in the session, play a knockout bracket instead of leaving the rest to watch. Today
      `spawn_paddles` just picks the two lowest-numbered players.
  - Bracket: seed the players (randomly, or by points later), give byes when the count isn't a
    power of 2 (3 players: one bye), and let winners advance to a final. The tournament winner
    is the minigame's result.
  - The session decides who plays each match and tells the minigame (a list of participants), so
    minigames stop picking players themselves.
  - **Matches one after another** (do this first): one match at a time, everyone else watches,
    with a short "next match" screen in between. Simple, but slow with many players: 8 players
    is 7 matches.
  - **Matches at the same time** (later): every entity needs a match id, because Pong assumes
    one of everything (`Single<&mut PongScore>`, one ball, and `abandon_without_both_players`
    counts every paddle). Lightyear's rooms (`RoomPlugin` / `RoomId`, in `lightyear_replication`'s
    visibility module) can limit which matches each client receives.
  - Someone who leaves mid-tournament loses their match by walkover and their opponent advances.
    Only abandon if fewer than two players are left.
  - Log the bracket and each match's result with `info!`.
- [ ] **Spectating when knocked out** (or waiting for your next match). With matches one after
      another, it's the normal view plus a "Watching: Player 2 vs Player 4" label and the
      bracket. With matches at the same time, spectators pick which match to watch (a key to
      cycle through them), so they need to receive every match.
- [ ] **The board/hub** that picks the next minigame.

## Loose ends

- [x] Track-art generator in the repo: `python3 tools/make_track.py` redraws `track.png`. Keep its
      constants in step with the `TRACK_*` / `GRID_*` ones in `src/config.rs`.
- [x] Delete the empty `src/minigames/four_player_pong` module.
- [ ] README: mention `RUST_LOG=info,smocka=debug cargo run` for the `debug!` logs.
