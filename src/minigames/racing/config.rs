//! Car handling, the track, the starting grid and the race's rules. Speeds are in world units per
//! second, accelerations in world units per second², and angles in radians.

use std::time::Duration;

use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

/// Car length (nose to tail) in world units. Draw `car.png` with the same proportions.
pub const CAR_LENGTH: f32 = 16.0;
/// Car width in world units. Draw `car.png` with the same proportions.
pub const CAR_WIDTH: f32 = 10.0;

/// Top speed going forwards.
pub const MAX_SPEED: f32 = 200.0;
/// Top speed in reverse.
pub const MAX_REVERSE_SPEED: f32 = 70.0;
/// How fast the throttle speeds the car up.
pub const ACCELERATION: f32 = 250.0;
/// How fast holding brake (or throttle while rolling backwards) slows the car down.
pub const BRAKE_DECELERATION: f32 = 500.0;
/// How fast brake speeds the car up backwards once it has stopped.
pub const REVERSE_ACCELERATION: f32 = 150.0;
/// How fast the car slows down with neither throttle nor brake held.
pub const COAST_DECELERATION: f32 = 150.0;

/// How fast the car turns with steering held, once it is going at least
/// [`FULL_STEER_SPEED`]. Slower cars turn proportionally slower, and a car standing still
/// can't turn at all.
pub const TURN_RATE: f32 = 3.5;
/// The speed (either way) at which steering reaches its full [`TURN_RATE`].
pub const FULL_STEER_SPEED: f32 = 60.0;

/// The furthest a car centre may get from the middle of the screen, sideways and up or
/// down. Keeps the whole car on screen whichever way it points.
pub const CAR_MAX_X: f32 = VIEW_WIDTH / 2.0 - CAR_LENGTH / 2.0;
/// See [`CAR_MAX_X`].
pub const CAR_MAX_Y: f32 = VIEW_HEIGHT / 2.0 - CAR_LENGTH / 2.0;
/// A car pushing against the edge of the screen goes no faster than this, so it can still
/// turn away from it.
pub const EDGE_MAX_SPEED: f32 = 40.0;

/// How bouncy bumps between cars are, from 0 to 1. At 0, cars that hit each other move on
/// together; at 1 they bounce apart as fast as they came together.
pub const BUMP_RESTITUTION: f32 = 0.4;
/// How fast a car shoved sideways by a bump (or past its top speed) stops sliding.
pub const SLIDE_DECELERATION: f32 = 600.0;

/// Top speed either way on grass.
pub const GRASS_MAX_SPEED: f32 = 80.0;
/// How fast a car going faster than [`GRASS_MAX_SPEED`] slows down on grass. Keep it above
/// [`ACCELERATION`], or throttle would beat it.
pub const GRASS_DECELERATION: f32 = 400.0;

/// Cars the starting grid has room for (and `track.png` draws), so the most players a race
/// can have.
pub const GRID_SLOTS: usize = 8;
/// Distance between rows of the starting grid, and from the start line to the front row.
pub const GRID_ROW_SPACING: f32 = 24.0;
/// Distance of each of the grid's two lanes from the middle of the road.
pub const GRID_LANE_OFFSET: f32 = 14.0;

/// Laps in a race, unless the host picks another number in the lobby.
pub const LAPS: u32 = 3;
/// The fewest laps the host can pick.
pub const MIN_LAPS: u32 = 1;
/// The most laps the host can pick.
pub const MAX_LAPS: u32 = 10;
/// How long the cars wait on the grid before the start, unless
/// [`RaceRules`](super::race::RaceRules) says otherwise.
pub const COUNTDOWN: Duration = Duration::from_secs(3);
/// How long the race goes on after the first car finishes, so one slow player can't hold
/// everyone up. Unless [`RaceRules`](super::race::RaceRules) says otherwise.
pub const FINISH_GRACE: Duration = Duration::from_secs(10);
/// How far each checkpoint reaches along the track. A car must be seen inside it on some
/// tick, so keep it well above how far a car moves in one tick (`MAX_SPEED / TICK_RATE`);
/// `track.rs` checks this when it compiles.
pub const CHECKPOINT_DEPTH: f32 = 16.0;
/// How far each checkpoint reaches past both edges of the road, so a car running a little
/// wide still counts. Keep it small enough that cutting across the grass inside a hairpin
/// misses the checkpoint at its apex.
pub const CHECKPOINT_MARGIN: f32 = 16.0;
