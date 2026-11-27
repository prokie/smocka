//! Shootout's arena, sizes and speeds. Sizes are in world units, speeds in world units per second,
//! and angles in radians.
//!
//! The arena's art (`arena.png`, from `tools/make_art.py`) is drawn from a copy of these: keep
//! them in step.

use std::time::Duration;

use bevy::math::{Rect, Vec2};

use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};
use crate::config::network::TICK_RATE;

/// Thickness of the wall around the edge of the arena.
pub const BORDER: f32 = 8.0;
/// Half the size of the floor inside the border wall.
pub const ARENA_HALF_WIDTH: f32 = VIEW_WIDTH / 2.0 - BORDER;
pub const ARENA_HALF_HEIGHT: f32 = VIEW_HEIGHT / 2.0 - BORDER;

/// A player's figure is a circle this big.
pub const GUNNER_RADIUS: f32 = 8.0;
/// How fast a gunner walks, in world units per second: across the arena in about four seconds.
pub const GUNNER_SPEED: f32 = 140.0;

/// Distance from the gunner's centre to the tip of its barrel (matches `gunner.png`).
pub const MUZZLE_OFFSET: f32 = GUNNER_RADIUS + 5.0;
/// A paint ball is a circle this big.
pub const PAINT_BALL_RADIUS: f32 = 3.0;
/// How fast a paint ball flies, in world units per second: across the arena in about a second.
pub const PAINT_BALL_SPEED: f32 = 600.0;
/// How many times a paint ball ricochets off walls and cover before splatting.
pub const PAINT_BALL_BOUNCES: u8 = 1;
/// Minimum time between two shots from the same gunner, in seconds.
pub const FIRE_COOLDOWN_SECONDS: f32 = 0.35;
/// [`FIRE_COOLDOWN_SECONDS`] in fixed ticks.
pub const FIRE_COOLDOWN_TICKS: u32 = (FIRE_COOLDOWN_SECONDS * TICK_RATE as f32) as u32;
/// How many hits knock a gunner out by default, and the limits the host can pick between.
pub const HITS_TO_KNOCK_OUT: u32 = 3;
pub const MIN_HITS_TO_KNOCK_OUT: u32 = 1;
pub const MAX_HITS_TO_KNOCK_OUT: u32 = 10;

/// A centre pickup is a circle this big.
pub const PICKUP_RADIUS: f32 = 8.0;
/// How long into a round the first pickup appears, and how long after a pickup is collected
/// before the next one appears.
pub const PICKUP_FIRST_DELAY: Duration = Duration::from_secs(5);
pub const PICKUP_INTERVAL: Duration = Duration::from_secs(8);
/// How long timed weapon buffs (Spread, Rapid Fire) last.
pub const BUFF_DURATION_SECONDS: f32 = 6.0;
/// [`BUFF_DURATION_SECONDS`] in fixed ticks.
pub const BUFF_DURATION_TICKS: u32 = (BUFF_DURATION_SECONDS * TICK_RATE as f32) as u32;
/// Half the fan angle between side paint balls when firing with the Spread buff, in radians.
pub const SPREAD_ANGLE: f32 = 0.17;
/// Fire cooldown in fixed ticks while the Rapid Fire buff is active.
pub const RAPID_FIRE_COOLDOWN_TICKS: u32 = FIRE_COOLDOWN_TICKS / 2;
/// Symmetric spots around the centre cover where pickups can appear.
pub const PICKUP_SPOTS: [Vec2; 4] = [
    Vec2::new(0.0, 56.0),
    Vec2::new(0.0, -56.0),
    Vec2::new(-72.0, 0.0),
    Vec2::new(72.0, 0.0),
];

/// How long into a round sudden death starts closing the safe zone, and how long it takes to
/// shrink to its minimum size.
pub const SUDDEN_DEATH_DELAY: Duration = Duration::from_secs(25);
pub const SUDDEN_DEATH_SHRINK: Duration = Duration::from_secs(20);
/// Minimum half-size of the sudden-death safe zone, leaving corridors all around the centre cover.
pub const SAFE_ZONE_MIN_HALF_WIDTH: f32 = 76.0;
pub const SAFE_ZONE_MIN_HALF_HEIGHT: f32 = 56.0;
/// How often a gunner standing outside the sudden-death safe zone takes a hit.
pub const ZONE_DAMAGE_INTERVAL: Duration = Duration::from_secs(1);

/// A block `width` by `height`, centred on (`x`, `y`).
const fn block(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        min: Vec2::new(x - width / 2.0, y - height / 2.0),
        max: Vec2::new(x + width / 2.0, y + height / 2.0),
    }
}

/// The blocks inside the arena to take cover behind: one in the middle, four bars around it and
/// one in front of each side. Symmetric, so no spawn is better than another.
pub const COVER: [Rect; 7] = [
    block(0.0, 0.0, 48.0, 48.0),
    block(-170.0, 80.0, 90.0, 16.0),
    block(170.0, 80.0, 90.0, 16.0),
    block(-170.0, -80.0, 90.0, 16.0),
    block(170.0, -80.0, 90.0, 16.0),
    block(-260.0, 0.0, 16.0, 70.0),
    block(260.0, 0.0, 16.0, 70.0),
];

/// Where players start, in the order they're handed out: opposite corners first, so two players
/// start as far apart as they can.
pub const SPAWNS: [Vec2; 8] = [
    Vec2::new(-280.0, 140.0),
    Vec2::new(280.0, -140.0),
    Vec2::new(280.0, 140.0),
    Vec2::new(-280.0, -140.0),
    Vec2::new(0.0, 140.0),
    Vec2::new(0.0, -140.0),
    Vec2::new(-200.0, 0.0),
    Vec2::new(200.0, 0.0),
];

#[cfg(test)]
mod tests {
    use super::{
        ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH, COVER, GUNNER_RADIUS, PICKUP_RADIUS, PICKUP_SPOTS,
        SAFE_ZONE_MIN_HALF_HEIGHT, SAFE_ZONE_MIN_HALF_WIDTH, SPAWNS,
    };

    #[test]
    fn every_spawn_is_on_open_floor_with_room_for_a_player() {
        for spawn in SPAWNS {
            assert!(
                spawn.x.abs() + GUNNER_RADIUS <= ARENA_HALF_WIDTH
                    && spawn.y.abs() + GUNNER_RADIUS <= ARENA_HALF_HEIGHT,
                "{spawn} is outside the arena"
            );
            for block in COVER {
                let gap = block.inflate(GUNNER_RADIUS);
                assert!(!gap.contains(spawn), "{spawn} overlaps {block:?}");
            }
        }
    }

    #[test]
    fn spawns_are_all_different() {
        for (i, a) in SPAWNS.iter().enumerate() {
            for b in &SPAWNS[i + 1..] {
                assert!(a.distance(*b) > 4.0 * GUNNER_RADIUS, "{a} and {b}");
            }
        }
    }

    #[test]
    fn every_pickup_spot_is_on_open_floor_with_room() {
        let clearance = PICKUP_RADIUS + GUNNER_RADIUS;
        for spot in PICKUP_SPOTS {
            assert!(
                spot.x.abs() + clearance <= ARENA_HALF_WIDTH
                    && spot.y.abs() + clearance <= ARENA_HALF_HEIGHT,
                "{spot} is too close to the border wall"
            );
            for block in COVER {
                let gap = block.inflate(clearance);
                assert!(!gap.contains(spot), "{spot} is too close to {block:?}");
            }
        }
    }

    #[test]
    fn minimum_safe_zone_leaves_corridors_around_centre_cover() {
        let centre = COVER[0];
        let min_corridor = 2.0 * GUNNER_RADIUS;
        assert!(SAFE_ZONE_MIN_HALF_WIDTH - centre.half_size().x >= min_corridor);
        assert!(SAFE_ZONE_MIN_HALF_HEIGHT - centre.half_size().y >= min_corridor);
    }
}
