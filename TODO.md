# TODO

## Racing minigame

- [x] **1. Pick the minigame** — `Minigame` enum, one lobby button per game, `SessionPhase::Racing`.
- [x] **2. Drivable cars** — `Car` / `CarState` (predicted + interpolated), `RacingInput`, `drive()`.
- [x] **3. Track** — oval in `track.rs`, `surface_at()`, grass slowdown, grid, `track.png`, `F3` gizmos.
- [x] **4. Checkpoints and laps** — `CHECKPOINTS` / `checkpoint_at()` in `track.rs`, `RaceProgress` counted
      by the host in `progress.rs`, "Lap 2/3" HUD, checkpoints in the `F3` gizmos.
- [x] **5. Start and finish** — `RaceStatus` countdown / racing / finishing and the host's
      `RaceDirector` in `race.rs`, places on `RaceProgress`, `FINISH_GRACE`, results with the full
      order, abandon when every car's player has left.
- [x] **6. Polish** — live race positions ("2nd / 4") from `running_order` in `progress.rs`,
      handling tuned, flat 1080p art from `tools/make_art.py`, skid marks in `skid_marks.rs`.
      Sounds moved to *Sound* under *Later*.

Known limitation: with prediction, the joiner's car can jump forward a little when the race starts.
Acceptable for now (see *Later*).

## Netcode and engine

APIs below were checked against the Lightyear 0.31 / Bevy 0.20.1 sources in `~/.cargo/registry`.

- [x] **Don't mark unchanged state as changed** (quick win). Replication sends components that
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
- [x] **Art style: stylized flat HD at 1080p.** Textures at `ART_SCALE` (3) pixels per world
      unit with linear filtering, every sprite sized in world units with `custom_size`, Pong's
      score drawn 3× bigger and scaled down so it stays sharp. Placeholders redrawn by
      `tools/make_art.py`. No mipmaps needed at 3×.
  - [ ] If 4K textures are ever wanted: downscaling them more than 2× shimmers without mipmaps.
        Bevy 0.20's asset processor can make them (`compressed_image_saver` in `bevy_image`, with
        a `generate_mipmaps` setting, writing compressed KTX2). It needs the `asset_processor`
        and `compressed_image_saver` features (and a C++ compiler) to build, and `ktx2` to load.
  - [ ] Simple shapes could skip image files: `Mesh2d` with a `ColorMaterial` gives crisp,
        resizable shapes, with edges smoothed by the camera's MSAA (on by default, 4 samples).

## Playing with friends

- [x] **Downloadable builds**: `.github/workflows/release.yml` builds Linux, Windows and macOS
      (Apple Silicon) zips on a `v*` tag and publishes a GitHub Release; `ci.yml` runs fmt,
      clippy, tests and docs on every push. The README has *Download and play* and *Making a
      release*. Not yet run on GitHub: check the first tagged release.
  - [ ] Linux builds come from `ubuntu-latest`, so they need a recent glibc. If a friend's distro
        is too old, build on an older Ubuntu image.
  - [ ] Intel Macs: add an `x86_64-apple-darwin` build if a friend needs it.
- [x] **Refuse other versions of the game**: `PROTOCOL_ID` is a hash of the crate version, the
      main menu shows the version, and a timed-out join suggests checking it.
- [ ] **Playing over the internet without port forwarding.** Today the host forwards UDP port
      5000 (documented in the README). Alternatives: a relay server, or Steam networking.
      Lightyear 0.31 ships `lightyear_steam` and `lightyear_p2p` crates (not looked into yet).

## Later

- [ ] **Gamepads and rebindable controls**: replace the hard-coded `KeyCode`s in
      `pong_input.rs` and `racing_input.rs` with `bevy_enhanced_input`. Lightyear supports it
      through its `lightyear_inputs_bei` crate. Matters more with every minigame added.
- [ ] **Find games on the local network**: the host announces itself (UDP broadcast or mDNS),
      and the menu lists games to click instead of typing an IP address.
- [ ] **Sound**, for every minigame at once: add Bevy's `audio` feature back to `Cargo.toml`.
      Its built-in audio is enough for Pong's bounces and goals and racing's countdown beeps and
      finish. Racing's engine could follow speed with `AudioSink::set_speed`. For separate
      music/effects volumes or fades, look at `bevy_kira_audio` (check it supports Bevy 0.20
      first).
- [ ] **Car-to-car bumping**: the hard prediction problem. Do it on its own.
- [ ] **Start on an agreed tick**, so predicted cars don't jump at the start.
- [ ] **More interesting tracks**: describe the track as data (a centre line plus width) instead of
      an oval.
- [x] **Placements in results**: `MinigameResult { placements, summary }`, best first, with
      shared places for ties and a note such as "DNF". Racing places every car (finishers, then
      the rest by progress); Pong places its two players. The results screen lists them.
- [x] **Points across games**: `Points` on each player (`src/standings.rs`), one point per player
      beaten, ties sharing. A `SessionPhase::Standings` screen follows each result.
- [x] **Player limits per minigame**: `Minigame::min_players` / `max_players` (Pong 2, racing up
      to `GRID_SLOTS`). The lobby enables each start button on its own minimum.
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
  - Steps:
    - [x] 1. The session picks the players: the host sets the replicated `Participants` when it
          starts a minigame (`pick_participants`), and Pong and racing seat exactly those.
    - [x] 2. Bracket logic as plain data: `src/tournament.rs` (`Bracket`: seeding, byes,
          `record_winner`, `placements`, `describe`), unit tested.
    - [ ] 3. Run a tournament when a minigame has fewer seats than players: matches one after
          another with a short "next match" phase between, each match's `MinigameEnded::Won`
          recorded in the bracket instead of ending the minigame, and the bracket's placements as
          the result. Decide the seeding (random, or by points).
    - [ ] 4. Watching: a "Watching: Ada vs Bo" label for players not in the match, and the bracket
          on the next-match screen (replicate the `Bracket`).
    - [ ] 5. Walkovers: a player who leaves mid-tournament loses their match; abandon only with
          fewer than two players left.
- [ ] **Spectating when knocked out** (or waiting for your next match). With matches one after
      another, it's the normal view plus a "Watching: Player 2 vs Player 4" label and the
      bracket. With matches at the same time, spectators pick which match to watch (a key to
      cycle through them), so they need to receive every match.
- [ ] **The board/hub** that picks the next minigame.

## Loose ends

- [x] Art generator in the repo: `python3 tools/make_art.py` redraws every placeholder. Keep its
      constants in step with the ones in `src/config.rs`.
- [x] Delete the empty `src/minigames/four_player_pong` module.
- [x] README: mention `RUST_LOG=info,smocka=debug cargo run` for the `debug!` logs.
- [ ] Bevy's built-in font only covers ASCII, so names like "Åsa" show boxes for the letters it
      lacks. Ship a font with wider coverage in `assets/` and use it for all UI text.
