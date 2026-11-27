//! Pong Royale's sizes, speeds and rules. Sizes are in world units, speeds in world units per
//! second, and angles in radians.
//!
//! The arena is a square in the middle of the screen, with a goal on each side and a block in each
//! corner. `tools/make_art.py` draws `arena.png` from a copy of these: keep them in step.

use crate::config::display::VIEW_HEIGHT;
use crate::config::network::TICK_RATE;

/// From the centre of the arena to each goal line.
pub const ARENA_HALF: f32 = 172.0;
/// Each corner of the arena is blocked by a square this big, so the ball can't slip out between
/// two goals.
pub const CORNER_SIZE: f32 = 40.0;
/// Half the width of each goal: the open stretch between two corner blocks.
pub const GOAL_HALF_WIDTH: f32 = ARENA_HALF - CORNER_SIZE;
/// How thick the wall is that closes a side with nobody (left) to defend it. It stands just inside
/// the goal line.
pub const WALL_THICKNESS: f32 = 6.0;

/// From the centre of the arena to each paddle's centre.
pub const PADDLE_DISTANCE: f32 = ARENA_HALF - 14.0;
/// Paddle length (along its side) and thickness. Pong's `paddle.png` has these proportions.
pub const PADDLE_LENGTH: f32 = 48.0;
pub const PADDLE_THICKNESS: f32 = 8.0;
pub const PADDLE_SPEED: f32 = 300.0;
/// The furthest a paddle's centre goes along its side, either way: far enough for its end to
/// reach the corner block, so it can cover the whole goal.
pub const PADDLE_MAX_ALONG: f32 = GOAL_HALF_WIDTH - PADDLE_LENGTH / 2.0;

/// Ball width and height. Pong's `ball.png` is drawn square.
pub const BALL_SIZE: f32 = 8.0;
/// Ball speed when it's served from the centre.
pub const BALL_SERVE_SPEED: f32 = 160.0;
/// A serve goes towards a player's goal, between these angles off straight at it, to either side.
pub const BALL_SERVE_MIN_ANGLE: f32 = 10.0_f32.to_radians();
pub const BALL_SERVE_MAX_ANGLE: f32 = 30.0_f32.to_radians();
/// Each paddle hit multiplies the ball's speed by this, up to [`BALL_MAX_SPEED`].
pub const BALL_HIT_SPEEDUP: f32 = 1.08;
/// The ball never gets faster than this. Keep it below `(PADDLE_THICKNESS + BALL_SIZE) *
/// TICK_RATE`, or it can jump through a paddle in one tick: `step_ball` checks.
pub const BALL_MAX_SPEED: f32 = 480.0;
/// The steepest angle the ball leaves a paddle at, off straight back across the arena. Hitting the
/// ball with a paddle's end sends it towards a neighbour's goal.
pub const BALL_MAX_BOUNCE_ANGLE: f32 = 45.0_f32.to_radians();
/// How far out from the centre the ball's centre must get, past a side's goal line, to cost that
/// side a life. At this distance the ball is fully over the line.
pub const GOAL_DISTANCE: f32 = ARENA_HALF + BALL_SIZE / 2.0;

/// Lives each player starts with, unless the host picks another number in the lobby.
pub const LIVES: u32 = 3;
pub const MIN_LIVES: u32 = 1;
pub const MAX_LIVES: u32 = 9;

// The arena fits on the screen, and the ball can't skip through a paddle in one tick.
const _: () = assert!(ARENA_HALF + WALL_THICKNESS < VIEW_HEIGHT / 2.0);
const _: () = assert!(BALL_MAX_SPEED < (PADDLE_THICKNESS + BALL_SIZE) * TICK_RATE as f32);
