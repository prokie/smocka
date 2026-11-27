//! Two-player Pong's sizes, speeds and rules. Sizes are in world units, speeds in world units per
//! second, and angles in radians.

use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

/// Thickness of the wall drawn along the top and bottom of `arena.png`, in world units.
pub const WALL_THICKNESS: f32 = 4.0;

/// Horizontal distance from the arena centre to each paddle, in world units.
pub const PADDLE_X_OFFSET: f32 = 296.0;
/// Paddle width in world units. Draw `paddle.png` with the same proportions.
pub const PADDLE_WIDTH: f32 = 8.0;
/// Paddle height in world units. Draw `paddle.png` with the same proportions.
pub const PADDLE_HEIGHT: f32 = 48.0;
/// Paddle speed in world units per second.
pub const PADDLE_SPEED: f32 = 300.0;
/// Highest `y` a paddle centre may reach (the lowest is the negative of this).
/// Keeps the paddle inside the walls at the top and bottom of the arena.
pub const PADDLE_MAX_Y: f32 = VIEW_HEIGHT / 2.0 - WALL_THICKNESS - PADDLE_HEIGHT / 2.0;

/// Ball width and height in world units. Draw `ball.png` square.
pub const BALL_SIZE: f32 = 8.0;
/// Ball speed when it is served, in world units per second.
pub const BALL_SERVE_SPEED: f32 = 180.0;
/// The flattest angle a serve can take, in radians away from straight across.
pub const BALL_SERVE_MIN_ANGLE: f32 = 15.0_f32.to_radians();
/// The steepest angle a serve can take, in radians away from straight across.
///
/// Each serve picks an angle between [`BALL_SERVE_MIN_ANGLE`] and this, up or down at
/// random. Keep it at most [`BALL_MAX_BOUNCE_ANGLE`], so a serve is never steeper than a
/// return can be.
pub const BALL_SERVE_MAX_ANGLE: f32 = 35.0_f32.to_radians();
/// Each paddle hit multiplies the ball's speed by this, up to [`BALL_MAX_SPEED`].
/// Each serve starts again at [`BALL_SERVE_SPEED`].
pub const BALL_HIT_SPEEDUP: f32 = 1.1;
/// The ball never gets faster than this, in world units per second.
///
/// Keep it below `(PADDLE_WIDTH + BALL_SIZE) * TICK_RATE` (2048 for the current values).
/// Above that, the ball can jump past a paddle in one tick without ever overlapping it.
pub const BALL_MAX_SPEED: f32 = 600.0;
/// The steepest angle the ball can leave a paddle at, in radians away from straight across.
///
/// Where the ball hits a paddle decides its angle: the centre sends it straight across,
/// and the very ends send it off at this angle, up or down. Keep it well below 90°, or the
/// ball can crawl almost vertically across the arena.
pub const BALL_MAX_BOUNCE_ANGLE: f32 = 37.0_f32.to_radians();
/// Highest `y` the ball centre may reach before it bounces (the lowest is the negative).
pub const BALL_MAX_Y: f32 = VIEW_HEIGHT / 2.0 - WALL_THICKNESS - BALL_SIZE / 2.0;
/// How far left or right of the centre the ball centre must get to score a goal.
/// At this distance the ball is fully off screen.
pub const GOAL_X: f32 = VIEW_WIDTH / 2.0 + BALL_SIZE / 2.0;
/// Goals a player needs to win, unless the host picks another number in the lobby.
pub const GOALS_TO_WIN: u32 = 5;
/// The fewest goals to win the host can pick.
pub const MIN_GOALS_TO_WIN: u32 = 1;
/// The most goals to win the host can pick.
pub const MAX_GOALS_TO_WIN: u32 = 10;
/// Font size of the score at the top of the arena, in world units.
pub const SCORE_FONT_SIZE: f32 = 32.0;
