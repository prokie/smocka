# TODO

## Netcode and engine

APIs below were checked against the Lightyear 0.31 / Bevy 0.20.1 sources in `~/.cargo/registry`.

- [ ] **Smooth rendering between ticks, and blend out rollback snaps.** The `sync_*_transform`
      systems copy the latest fixed-tick state into `Transform`. At 128 Hz on a 60/144/165 Hz
      screen that can stutter a little. Calling `.add_correction()` on the predicted
      `PaddlePosition` and `CarState` eases mispredictions out over a few frames instead of
      snapping, and adds `FrameInterpolationPlugin` by itself (`lightyear_prediction`
      `registry.rs`). Entities opt in to frame interpolation with the `FrameInterpolate` marker.
      `add_correction` needs the component to implement `Default` and `Diffable`, which
      `CarState` and `PaddlePosition` don't yet. Insert `SkipFrameInterpolation` on intentional
      teleports, such as Pong's ball going back to the centre after a goal, so it doesn't smear
      across the screen. Matters more for racing since bumping came in: every driver predicts
      every car from rebroadcast inputs, which arrive late, so other players' cars snap a little
      each time their driver changes input.
- [ ] **Test under bad network conditions.** Everything is tested over clean localhost.
      `Link::with_conditioner(LinkConditionerConfig { .. })` (`lightyear_link`) adds latency,
      jitter and packet loss. Add a CLI flag (e.g. `--lag`) for manual play, and one
      integration test with a conditioned link. Racing needs it most: other players' cars are
      predicted from their rebroadcast inputs, and are wrong for longer the higher the latency.
- [ ] **Reconsider `TICK_RATE` (128 Hz) and `SEND_INTERVAL` (64 Hz)** once frame interpolation is
      in. 64 Hz ticks and a 20–32 Hz send rate would roughly halve CPU and bandwidth. Measure
      first. A lower tick rate also tightens three limits that are written in terms of it: the
      ball's tunnelling limit (`BALL_MAX_SPEED`, see its doc in `src/config.rs`), how thick racing
      checkpoints must be, and how fast cars can drive at each other without passing through
      (checked when `bumps.rs` compiles). A lower send rate makes other players' paddles, and cars
      for spectators, show slightly later.
- [ ] **Lightyear's debug tools, when chasing a desync**: `lightyear_tools` has a structured
      tracing layer and a debug UI covering sync, input buffering, rollbacks and correction.
      Check which `lightyear` feature enables it.
- **Art** (flat HD at 1080p, textures at `ART_SCALE` = 3 pixels per world unit, no mipmaps):
  - [ ] If 4K textures are ever wanted: downscaling them more than 2× shimmers without mipmaps.
        Bevy 0.20's asset processor can make them (`compressed_image_saver` in `bevy_image`, with
        a `generate_mipmaps` setting, writing compressed KTX2). It needs the `asset_processor`
        and `compressed_image_saver` features (and a C++ compiler) to build, and `ktx2` to load.
  - [ ] Simple shapes could skip image files: `Mesh2d` with a `ColorMaterial` gives crisp,
        resizable shapes, with edges smoothed by the camera's MSAA (on by default, 4 samples).

## Playing with friends
- [ ] Intel Macs: add an `x86_64-apple-darwin` build if a friend needs it.
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

- [ ] **Racing doesn't tell you when you miss a checkpoint.** `RaceProgress::advance`
      (`progress.rs`) only counts the next checkpoint in order, so a car that cuts across the
      grass past one gets no laps until it drives back to it. The only sign is a host-side
      `debug!` in `count_laps`. The player just sees the lap counter not go up. Show it: a
      "Missed checkpoint!" message when your car enters a checkpoint out of order, and mark the
      checkpoint it needs next (or an arrow towards it). `RaceProgress` is replicated, so each
      client already knows its own car's `next_checkpoint` and can check its own `CarState`
      against `checkpoint_at`, with no new message from the host.
- [ ] **Skid marks depend on the frame rate.** `leave_skid_marks` runs in `Update` and divides
      each change in `CarState` by the frame's `delta_secs`. Cars driven in `FixedUpdate` (all
      of them on the host and on a driver's machine) only change on 128 Hz ticks. On a 144–240 Hz
      screen, a frame with a tick sees 7.8 ms of driving in a 4–7 ms frame, so cornering and
      braking look up to ~1.9× harder and marks start too easily. At 60 Hz, 2 or 3 ticks per
      frame make it swing between 0.9× and 1.4×. Moving the system to `FixedUpdate` would break
      interpolated cars (every car, for a spectator), which change every frame. Divide by
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
- [ ] **Start on an agreed tick**, so predicted cars don't jump at the start. It also fixes a
      rollback problem: `drive_cars` reads `RaceStatus`, which isn't predicted. A rollback that
      replays ticks from before the start sees the current `Racing` and lets the car move early.
      Comparing the tick being simulated with the start tick fixes both.
- [ ] **More interesting tracks**: describe the track as data (a centre line plus width) instead of
      an oval.
- [ ] **Tournament matches at the same time.** Today matches run one after another, with
      everyone else watching ("Watching: …" and the bracket on the next-match screen). That's slow
      with many players: 8 players is 7 matches. Running them at once needs a match id on every
      entity, because Pong assumes one of everything (`Single<&mut PongScore>`, one ball, and
      `abandon_without_both_players` counts every paddle). Lightyear's rooms (`RoomPlugin` /
      `RoomId`, in `lightyear_replication`'s visibility module) can limit which matches each
      client receives. Spectators then pick which match to watch (a key to cycle through them),
      so they need to receive every match.
