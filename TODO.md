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
- [ ] **Anyone who can reach a host can join.** `PRIVATE_KEY` (`src/config.rs`) is all zeros
      and joiners mint their own connect tokens, so the key isn't a secret (it can't be while it
      ships inside an open-source client). That's fine on a LAN, but a port-forwarded host
      accepts anyone who finds the address. A crafted token gets no further than a normal join:
      `name_from_user_data` cleans the name. If it matters: a session password the host checks
      after connecting, or tokens handed out by a server (a relay could do it).

## Bugs and rough edges

- [ ] **Hosting when the port is taken crashes the game.** If something already holds port 5000
      (say, another copy of the game), Lightyear's `UdpEndpointPlugin::link` observer returns
      `AddrInUse` and Bevy's default error handler panics: "Address already in use (os error
      98)" (checked with a throwaway test). Release builds on Windows have no console, so the
      window just disappears. Bind a test socket in `HostSession::apply` first and, if that
      fails, stay in the menu with a `SessionEndReason` such as "Port 5000 is in use. Is the
      game already hosting?". Without the panic it would still be broken: the netcode server
      gets `Started` whether or not the bind worked, so the host would sit in a lobby nobody can
      join.
- [ ] **A walkover on the next-match screen doesn't restart the countdown.** When a player in
      the coming match leaves during `SessionPhase::NextMatch`, `walk_over_leavers` pairs a new
      match, but the phase stays `NextMatch`. `start_next_match_timer` doesn't run again, and
      the host's `PhaseTimer` keeps its remaining time, so near the end of the screen the new
      pair goes straight into the match. Clients' `Countdown` (`menu/next_match.rs`) keeps the
      old time too. Restart the timer in `follow_tournament` when already in `NextMatch`, and
      restart the client countdown when the next match changes.
- [ ] **A race goes on with only one car left.** `end_race` only abandons once every car is
      gone. If all the other drivers leave before anyone finishes, the last player drives the
      remaining laps alone and scores 0 points. Pong abandons as soon as a paddle's player
      leaves. Abandon when fewer than two cars are left and `director.finishers` is empty.
      Racing needs 2 players to start, so a solo race isn't otherwise possible.
- [ ] **Racing doesn't tell you when you miss a checkpoint.** `RaceProgress::advance`
      (`progress.rs`) only counts the next checkpoint in order, so a car that cuts across the
      grass past one gets no laps until it drives back to it. The only sign is a host-side
      `debug!` in `count_laps`. The player just sees the lap counter not go up. Show it: a
      "Missed checkpoint!" message when your car enters a checkpoint out of order, and mark the
      checkpoint it needs next (or an arrow towards it). `RaceProgress` is replicated, so each
      client already knows its own car's `next_checkpoint` and can check its own `CarState`
      against `checkpoint_at`, with no new message from the host.
- [ ] **A new name is lost when quitting from the main menu.** `save_profile` only runs on
      `OnExit(AppState::MainMenu)`. Clicking Quit or closing the window after typing a name
      never leaves the main menu, so the name isn't saved. Save on `AppExit` too.
- [ ] **Skid marks depend on the frame rate.** `leave_skid_marks` runs in `Update` and divides
      each change in `CarState` by the frame's `delta_secs`. Cars driven in `FixedUpdate` (all
      of them on the host, a joiner's own car) only change on 128 Hz ticks. On a 144–240 Hz
      screen, a frame with a tick sees 7.8 ms of driving in a 4–7 ms frame, so cornering and
      braking look up to ~1.9× harder and marks start too easily. At 60 Hz, 2 or 3 ticks per
      frame make it swing between 0.9× and 1.4×. Moving the system to `FixedUpdate` would break
      interpolated cars (other players' cars on a joiner), which change every frame. Divide by
      the time the change covers instead: ticks run × the fixed timestep for driven cars, the
      frame time for interpolated ones.
- [ ] **"Esc: leave" and the watching line overlap on narrow windows.** Both sit 8 px from the
      bottom: the hint on the left (`spawn_leave_hint` in `menu.rs`), `WatchingText` centred
      (`menu/watching.rs`). In Bevy's default monospace font, "Watching: Player 1 vs Player 2
      (semi-final)" is about 540 px wide. In a 640 px window (`--left`/`--right` on a 1280 px
      screen) it starts at ~50 px, on top of the hint (12–108 px). Two 16-character names
      overlap it in any window narrower than ~950 px. Move one of them up.
- [ ] **Pong's ball sinks into a paddle on fast hits.** `step_ball` turns the ball around on a
      hit but leaves it where the step ended: up to `BALL_MAX_SPEED / TICK_RATE` ≈ 4.7 units
      inside the paddle (the ball is 8 wide), for one tick. When it bounces, put it back in
      front of the face (or mirror the overshoot). Cosmetic.
- [ ] **Two players can have the same name.** `spawn_player` keeps names as sent, so two "Ada"s
      (or someone who types "Player 2") look the same on bracket cards, results and "… left."
      notices. The host could add a number to a name that's taken ("Ada 2").

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
- [ ] **Start on an agreed tick**, so predicted cars don't jump at the start. It also fixes a
      rollback problem: `drive_cars` reads `RaceStatus`, which isn't predicted. A rollback that
      replays ticks from before the start sees the current `Racing` and lets the car move early.
      Comparing the tick being simulated with the start tick fixes both.
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
    - [x] 3. Run a tournament when a two-seat minigame has more players (`needs_tournament`):
          a replicated `Tournament` (game + bracket), seeded by points with random ties, matches
          one after another with a `SessionPhase::NextMatch` screen between
          (`TournamentRules::next_match`), and the bracket's placements as the result.
    - [x] 4. Watching: "Watching: Ada vs Bo (semi-final)" along the bottom for players not in
          the match (`menu/watching.rs`), and every round of the bracket on the next-match screen
          (`Bracket::overview`).
    - [x] 5. Walkovers: a player who leaves loses their current or coming match at once
          (`walk_over_leavers` on `PlayerLeft`), a later one when it comes up
          (`Tournament::walk_over`); placed "(left)"; abandoned only with fewer than two players.
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
- [ ] Pong's `update_score_text`, `sync_paddle_transform` and `sync_ball_transform`, and racing's
      `sync_car_transform`, run in every state. That's harmless, since they find nothing to do
      outside their minigame, but every other visuals system is scoped with
      `.run_if(in_state(...))`. Scope these too.
- [x] Bevy's built-in font only covers ASCII, so names like "Åsa" showed boxes. Fira Sans
      (`assets/fonts/`, compiled in) replaces Bevy's default font in `MenuPlugin`, so every text
      uses it. Covers Latin, Greek and Cyrillic, not CJK.
