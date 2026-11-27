//! Shootout's arena, sizes and speeds. Sizes are in world units, speeds in world units per second,
//! and angles in radians.
//!
//! The arena's art (`arena.png`, from `tools/make_art.py`) is drawn from a copy of these: keep
//! them in step.

use bevy::math::{Rect, Vec2};

use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

/// Thickness of the wall around the edge of the arena.
pub const BORDER: f32 = 8.0;
/// Half the size of the floor inside the border wall.
pub const ARENA_HALF_WIDTH: f32 = VIEW_WIDTH / 2.0 - BORDER;
pub const ARENA_HALF_HEIGHT: f32 = VIEW_HEIGHT / 2.0 - BORDER;

/// A player's figure is a circle this big.
pub const GUNNER_RADIUS: f32 = 8.0;

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
    use super::{ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH, COVER, GUNNER_RADIUS, SPAWNS};

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
}
