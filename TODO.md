# TODO

## Netcode and engine

APIs below were checked against the Lightyear 0.31 / Bevy 0.20.1 sources in `~/.cargo/registry`.

- [ ] **Reconsider `TICK_RATE` (128 Hz) and `SEND_INTERVAL` (64 Hz)** now that movement is
      smoothed between ticks (`minigames/smoothing.rs`). 64 Hz ticks and a 20–32 Hz send rate would roughly halve CPU and bandwidth. Measure
      first. A lower tick rate also tightens three limits that are written in terms of it: the
      ball's tunnelling limit (`BALL_MAX_SPEED`, see its doc in
      `minigames/two_player_pong/config.rs`), how thick racing checkpoints must be, and how
      fast cars can drive at each other without passing through (checked when `bumps.rs`
      compiles). A lower send rate makes other players' paddles, and cars for spectators, show
      slightly later, and smoothing draws things a tick late (15.6 ms at 64 Hz instead of
      7.8 ms).
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
      frame time for interpolated ones. Or read the state the car is drawn at
      (`smoothed`, in `minigames/smoothing.rs`): it moves on by exactly the frame's time for
      every car, so dividing by `delta_secs` is right, and marks line up with the drawn car.

      How many marks there are, and how dark they look, depends on the frame rate too.
      `leave_skid_marks` spawns one sprite per rear wheel per frame, and `fade_skid_marks`
      rewrites every mark's colour every frame: up to ~3,500 live sprites with 4 skidding cars
      at 144 Hz, ~11,500 with 8 at 240 Hz. Each strip reaches `WIDTH / 4` past both ends, so
      neighbours overlap by 0.8 units. At 240 Hz a car at top speed moves ~0.83 units a frame,
      so almost the whole mark is drawn twice (alpha ~0.70 instead of 0.45), and slower skids
      three times. At 60 Hz only ~24% is. Lay a strip only once the wheel has moved a set
      distance (2–3 units, keeping the last point per wheel in `SkidTrail`), and cap how many
      there are.
- [ ] **The log says "Using Fira Sans for all text"** (`use_ui_font` in `menu.rs`), but the font
      has been Fira Mono since `b319578`.
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

## Code structure

From a review on 2026-11-27, so that adding a minigame, a control or a lobby setting touches
fewer shared files. None of these change what players see.

They change the order replicated types are registered in, and what's sent over the network.
That's safe, since `PROTOCOL_ID` comes from the version, but the host and joiners must run the
same build.

- [ ] **`Controls` as a map keyed by `ControlAction`**, instead of one field per action and the
      `get` / `get_mut` matches. `#[serde(rename_all = "snake_case")]` on `ControlAction` keeps
      the saved `throttle = [...]` keys. Fill missing actions from the defaults in
      `sanitized()`. The settings-file tests in `controls.rs` check both.
- [ ] **Party rounds as a setting.** Minigame settings are now generic
      (`minigames/settings.rs`), but `PartyRounds` / `ChangeRounds` (`party.rs`) still repeat
      the pattern on their own: a replicated number, a command that changes it within limits, an
      observer spawning it on `Started`, a system keeping it up to date, and a lobby picker.
      `SettingId` names a minigame's setting; letting it name the party's too would remove the
      copy and `MenuButton::ChangeRounds`.
- [ ] **Small duplications.** `mark_controlled_car` and `mark_controlled_paddle` could be one
      generic observer. `spawn_cars` and `spawn_paddles` repeat a "participants still here"
      lookup that could be a `Participants` method.
- [ ] Optional: move the key, button and stick names (~75 lines) out of `controls.rs` into
      `controls/names.rs`.

## Later

- [ ] **Racing over long distances** (only if players on different continents join). Over a
      round trip of ~150 ms or more *with* packet loss, a joiner's car can stay out of line with
      the host's. Seen with `--lag=poor` (about 200 ms, 10% lost): a joiner's car stopped 20 units
      from where the host had it, and stayed there. Steady round trips up to 170 ms with no loss
      were fine, as was `--lag=average` (about 110 ms, 2% lost), so it doesn't affect players in
      one country (typically 10–50 ms).

      It takes both: lost or late inputs make the joiner's prediction wrong, and Lightyear refuses
      to correct it, because a correction arrives about a round trip after its tick and it won't
      roll back more than `RollbackPolicy::max_rollback_ticks` (default 20, which at 128 Hz is
      156 ms; it logs "Trying to do a rollback of N ticks. The max is 20! Aborting"). Raising
      that to 64 on the joiner's `PredictionManager` (`JoinSession`, in `network/transport.rs`)
      fixed it in a test, at the cost of replaying up to 64 ticks in a frame. A lower
      `TICK_RATE` covers more time with the same 20 ticks. Check a fix with
      `a_race_holds_up_over_an_average_connection` (`tests/integration/racing.rs`) set to
      `LagPreset::Poor`.
- [ ] **Find games on the local network**: the host announces itself (UDP broadcast or mDNS),
      and the menu lists games to click instead of typing an IP address.
- [ ] **Music**: everything is ready (`PlayMusic` / `StopMusic` in `sound.rs`, the music
      volume, Ogg Vorbis support) except the tracks. Pick where each starts, e.g. a theme on
      `OnEnter(AppState::MainMenu)`. Switching tracks cuts straight over; fading between them
      needs `bevy_kira_audio` (check it supports Bevy 0.20 first) or fading `AudioSink` volumes
      by hand.
- [ ] **Volume during a game**: the Sound screen is only in the main menu, like Controls.
- [ ] **Spectators don't hear bumps** in racing. Only the host and drivers work bumps out
      (`CarsBumped` in `bumps.rs`); a spectator's cars are interpolated. Spectators could check
      `contact` between the cars as they're drawn instead.
- [ ] **Start on an agreed tick**, so predicted cars don't jump at the start. It also fixes a
      rollback problem: `drive_cars` reads `RaceStatus`, which isn't predicted. A rollback that
      replays ticks from before the start sees the current `Racing` and lets the car move early.
      Comparing the tick being simulated with the start tick fixes both.
- [ ] **Blend out rollback snaps on drivers' screens**, if playtests show they look bad. Every
      driver predicts every car from inputs the host rebroadcasts, which arrive late, so a
      rollback snaps other players' cars to the corrected spot in one frame. On a LAN that's a
      few degrees of heading when a driver's steering arrives late, and more when a bump came
      out differently. Over the internet (~100 ms) a late steer would be ~20°.
      `.add_linear_correction::<D>()` on `CarState` eases snaps out over a few frames. It needs
      `CarState: Diffable<D>` (`lightyear_replication`'s `diffable.rs`), with a heading
      difference that goes the short way round (`add_correction()` would also need `Default`).
      Correction adds Lightyear's `FrameInterpolationPlugin` and needs `FrameInterpolate` on the
      corrected cars, which then takes over from `PreviousTick` smoothing
      (`minigames/smoothing.rs`): drop `PreviousTick` for those cars, or they're blended twice.
      Only give it to predicted cars, never the host's: it writes the blend into `CarState`,
      marking it changed every frame, so the host would re-send idle cars. Pong paddles don't
      need it: only the owner predicts one, and nobody else's input moves it.
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
