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

## Later

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
- [ ] Delete the empty `src/minigames/four_player_pong` module, or start it.
- [ ] README: mention `RUST_LOG=info,smocka=debug cargo run` for the `debug!` logs.
