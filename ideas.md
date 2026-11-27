# Ideas

Ways to make the minigames and the party more fun. Not planned work: pick from here, then move
what you pick into `TODO.md` or just build it.

**Cheap** marks ideas that fit the netcode as it is: the host already decides that part alone, or
every peer can work it out the same way from what it already has.

## Top picks

- [ ] **Spectators bet on Pong matches.** Pong is played as a knockout tournament, so most
      players spend it watching. Let them pick a winner before each match on the next-match
      screen, and give a point for a correct call. Watching becomes playing.
- [ ] **Impact feedback.** No minigame has screen shake, hit-pause or particles yet. A small shake
      on knockouts, hard bumps and fast Pong hits, a 2–3 frame freeze on a knockout, and sparks
      when cars collide. All local to each peer, nothing sent. **Cheap.**
- [ ] **Dodge roll in Shootout.** A short dash with a cooldown turns "trade shots" into "read and
      dodge". Paint balls are already slow enough to dodge.

## Pong

- [ ] **Spin**: a paddle that's moving when it hits the ball curves the ball's path. Rewards skill
      and varies rallies. **Cheap**: only the host simulates the ball.
- [ ] **Rally heat**: the ball changes colour or leaves a trail as it speeds up
      (`BALL_HIT_SPEEDUP`), so a long rally visibly gets tense.
- [ ] **Serve control**: the player who conceded (or scored) holds the ball briefly and picks the
      angle, instead of a random serve.
- [ ] **Chaos events** as a lobby setting: multi-ball after a long rally, a paddle that shrinks
      after each goal it scores (a built-in comeback), or a moving obstacle in the middle.
- [ ] **Match point**: a "MATCH POINT" banner and a zoomed-in camera for the deciding goal.

## Racing

- [ ] **Boost pads and slipstream**: pads on the track, and a speed boost for driving close
      behind another car. Slipstream lets the losers catch up, and makes bumping matter.
      **Cheap**: the track is the same fixed data on every peer, and drivers already predict
      every car.
- [ ] **Drift boost**: a long drift charges a short burst of speed. The skid-mark code already
      works out how much a car is sliding.
- [ ] **Hazards**: oil slicks dropped from the car, or cones you can knock into others.
- [x] **New track shapes**: the oval gets old fastest. A figure-eight with a crossing adds crashes
      for free. (Done: the track is now data, see `minigames/racing/track.rs`.)
- [x] **A track picker** in the lobby, now that a track is data: the old oval as a quick,
      flat-out track, the figure-eight as a technical one. (Done: the Track setting.)
- [ ] **Placement drama**: "FINAL LAP!" and a camera pan across the cars at the finish.

## Shootout

- [ ] **More pickups**: bouncing shots that ricochet off cover once, a big slow paint ball that
      pushes players back, speed boots, or a short invisibility.
- [ ] **Knockback** on hit, so shots push players into the closing sudden-death border. Ties two
      existing systems together.
- [ ] **Destructible cover** that wears down as paint hits it, so the arena changes during a
      round.
- [ ] **Knocked-out players stay involved**: they float as ghosts and can drop a slow paint ball,
      or pick where the next pickup appears. Being knocked out first is the least fun part of a
      free-for-all.

## Across the party

- [ ] **Final round worth double points**, announced on the next-round screen, so the party isn't
      decided before the end.
- [ ] **Comeback help**: last place starts at the front of the racing grid, or gets an extra hit
      point in Shootout.
- [ ] **Reactions**: keys 1–4 show a small emoji over your player and on spectators' screens.
      Cheap to send, and in a party game the joking around matters as much as the game.
- [ ] **Awards on the party-over screen**: "Most bumps", "Sharpshooter", "Longest rally". The host
      can count these as the games happen.

## New minigames that reuse what's built

- [ ] **Sumo**: racing's car bump physics on a round platform that shrinks. Last car on it wins.
- [ ] **Four-player Pong**: a paddle on each side of the screen, everyone plays at once. A
      free-for-all, so no tournament wait.
- [ ] **King of the Hill**: Shootout's movement with no guns, just shoving. Points for time spent
      in a zone that moves.
- [ ] **Hot Potato**: tag games are cheap to network, and the "who has it" moments are a lot of
      fun.
