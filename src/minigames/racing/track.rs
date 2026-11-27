//! The track's shape, which is the same constant data on every peer, so it is never replicated.
//! `assets/minigames/racing/track.png` is drawn to match it.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use super::config::{
    CAR_LENGTH, CHECKPOINT_DEPTH, GRASS_MAX_SPEED, GRID_LANE_OFFSET, GRID_ROW_SPACING, MAX_SPEED,
    START_LINE_X, TRACK_HALF_LENGTH, TRACK_INNER_RADIUS, TRACK_OUTER_RADIUS,
};
use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};
use crate::config::network::TICK_RATE;

// The whole road, plus room for a car on it, must fit on the screen.
const _: () = assert!(TRACK_HALF_LENGTH + TRACK_OUTER_RADIUS + CAR_LENGTH < VIEW_WIDTH / 2.0);
const _: () = assert!(TRACK_OUTER_RADIUS < VIEW_HEIGHT / 2.0);
const _: () = assert!(TRACK_INNER_RADIUS < TRACK_OUTER_RADIUS);
// A car at top speed must land inside each checkpoint on at least one tick, with room to spare.
const _: () = assert!(2.0 * MAX_SPEED as f64 / TICK_RATE < CHECKPOINT_DEPTH as f64);

/// What a car is driving on, which limits its speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Road,
    Grass,
}

impl Surface {
    /// Top speed either way on this surface.
    pub fn max_speed(self) -> f32 {
        match self {
            Surface::Road => MAX_SPEED,
            Surface::Grass => GRASS_MAX_SPEED,
        }
    }
}

/// `y` of the middle of the road along the bottom straight, where the grid and start line are.
pub const BOTTOM_STRAIGHT_Y: f32 = -(TRACK_INNER_RADIUS + TRACK_OUTER_RADIUS) / 2.0;

/// Cars start facing right, since the race goes anticlockwise.
pub const START_HEADING: f32 = 0.0;

/// The surface at `position`.
pub fn surface_at(position: Vec2) -> Surface {
    let distance = distance_from_centre_line(position);
    if (TRACK_INNER_RADIUS..=TRACK_OUTER_RADIUS).contains(&distance) {
        Surface::Road
    } else {
        Surface::Grass
    }
}

/// Distance from `position` to the nearest point of the centre line (see
/// [`TRACK_HALF_LENGTH`]).
fn distance_from_centre_line(position: Vec2) -> f32 {
    let nearest = Vec2::new(position.x.clamp(-TRACK_HALF_LENGTH, TRACK_HALF_LENGTH), 0.0);
    position.distance(nearest)
}

/// Where car `slot` (from 0) starts: two staggered lanes behind the start line, with the front
/// car on the inside lane.
pub fn grid_position(slot: usize) -> Vec2 {
    let row = (slot / 2) as f32;
    let outside = slot % 2 == 1;
    let x = START_LINE_X
        - GRID_ROW_SPACING * (row + 1.0)
        - if outside { GRID_ROW_SPACING / 2.0 } else { 0.0 };
    let lane = if outside { -1.0 } else { 1.0 };
    Vec2::new(x, BOTTOM_STRAIGHT_Y + lane * GRID_LANE_OFFSET)
}

/// The checkpoints a car must reach in order to complete a lap, anticlockwise: the start/finish
/// line, the right bend, the top straight and the left bend. Checkpoint 0 is the line itself
/// (just past it), so reaching it after the others completes a lap.
///
/// Each one reaches from the inner edge of the road out to the edge of the screen, so a car on
/// the outside grass still counts, but one cutting across the infield misses it.
pub const CHECKPOINTS: [Rect; 4] = [
    Rect {
        min: Vec2::new(START_LINE_X, -VIEW_HEIGHT / 2.0),
        max: Vec2::new(START_LINE_X + CHECKPOINT_DEPTH, -TRACK_INNER_RADIUS),
    },
    Rect {
        min: Vec2::new(
            TRACK_HALF_LENGTH + TRACK_INNER_RADIUS,
            -CHECKPOINT_DEPTH / 2.0,
        ),
        max: Vec2::new(VIEW_WIDTH / 2.0, CHECKPOINT_DEPTH / 2.0),
    },
    Rect {
        min: Vec2::new(-CHECKPOINT_DEPTH / 2.0, TRACK_INNER_RADIUS),
        max: Vec2::new(CHECKPOINT_DEPTH / 2.0, VIEW_HEIGHT / 2.0),
    },
    Rect {
        min: Vec2::new(-VIEW_WIDTH / 2.0, -CHECKPOINT_DEPTH / 2.0),
        max: Vec2::new(
            -TRACK_HALF_LENGTH - TRACK_INNER_RADIUS,
            CHECKPOINT_DEPTH / 2.0,
        ),
    },
];

/// The checkpoint `position` is in, if any.
pub fn checkpoint_at(position: Vec2) -> Option<usize> {
    CHECKPOINTS
        .iter()
        .position(|checkpoint| checkpoint.contains(position))
}

/// Points around the closed loop `radius` from the centre line, for drawing the road's edges.
/// The last point repeats the first.
pub fn outline(radius: f32, points_per_bend: usize) -> Vec<Vec2> {
    let bend = |centre_x: f32, start_angle: f32| {
        (0..=points_per_bend).map(move |i| {
            let angle = start_angle + PI * i as f32 / points_per_bend as f32;
            Vec2::new(centre_x, 0.0) + Vec2::from_angle(angle) * radius
        })
    };
    let mut points: Vec<Vec2> = bend(TRACK_HALF_LENGTH, -FRAC_PI_2)
        .chain(bend(-TRACK_HALF_LENGTH, FRAC_PI_2))
        .collect();
    points.push(points[0]);
    points
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;

    use super::{
        BOTTOM_STRAIGHT_Y, CHECKPOINTS, Surface, checkpoint_at, grid_position, outline, surface_at,
    };
    use crate::minigames::racing::config::{
        CAR_LENGTH, GRID_SLOTS, START_LINE_X, TRACK_HALF_LENGTH, TRACK_INNER_RADIUS,
        TRACK_OUTER_RADIUS,
    };

    #[test]
    fn the_straights_and_bends_are_road() {
        assert_eq!(surface_at(Vec2::new(0.0, BOTTOM_STRAIGHT_Y)), Surface::Road);
        assert_eq!(
            surface_at(Vec2::new(0.0, -BOTTOM_STRAIGHT_Y)),
            Surface::Road
        );
        assert_eq!(
            surface_at(Vec2::new(TRACK_HALF_LENGTH + 130.0, 0.0)),
            Surface::Road
        );
        assert_eq!(
            surface_at(Vec2::new(-TRACK_HALF_LENGTH - 130.0, 0.0)),
            Surface::Road
        );
    }

    #[test]
    fn the_infield_and_outside_are_grass() {
        assert_eq!(surface_at(Vec2::ZERO), Surface::Grass);
        assert_eq!(
            surface_at(Vec2::new(0.0, -TRACK_INNER_RADIUS + 1.0)),
            Surface::Grass
        );
        assert_eq!(
            surface_at(Vec2::new(0.0, -TRACK_OUTER_RADIUS - 1.0)),
            Surface::Grass
        );
        // Diagonally off a corner of the stadium: further than the outer radius from the bend.
        assert_eq!(
            surface_at(Vec2::new(TRACK_HALF_LENGTH + 120.0, 120.0)),
            Surface::Grass
        );
    }

    #[test]
    fn every_grid_slot_is_on_the_bottom_straight_behind_the_line() {
        let slots: Vec<Vec2> = (0..GRID_SLOTS).map(grid_position).collect();
        for (i, slot) in slots.iter().enumerate() {
            assert_eq!(surface_at(*slot), Surface::Road, "slot {i}");
            assert!(slot.x < START_LINE_X, "slot {i}");
            assert!(slot.x > -TRACK_HALF_LENGTH, "slot {i}");
            for other in &slots[i + 1..] {
                assert!(slot.distance(*other) >= CAR_LENGTH, "slot {i} overlaps");
            }
        }
    }

    #[test]
    fn the_outline_is_a_closed_loop_at_the_given_radius() {
        let points = outline(TRACK_OUTER_RADIUS, 16);
        assert_eq!(points.first(), points.last());
        for point in points {
            let nearest = Vec2::new(point.x.clamp(-TRACK_HALF_LENGTH, TRACK_HALF_LENGTH), 0.0);
            assert!((point.distance(nearest) - TRACK_OUTER_RADIUS).abs() < 1e-3);
        }
    }

    #[test]
    fn every_checkpoint_lies_across_the_road() {
        // The middle of the road where each checkpoint is.
        let middle = (TRACK_INNER_RADIUS + TRACK_OUTER_RADIUS) / 2.0;
        let road = [
            Vec2::new(START_LINE_X + 1.0, -middle),
            Vec2::new(TRACK_HALF_LENGTH + middle, 0.0),
            Vec2::new(0.0, middle),
            Vec2::new(-TRACK_HALF_LENGTH - middle, 0.0),
        ];
        for (i, point) in road.into_iter().enumerate() {
            assert_eq!(surface_at(point), Surface::Road, "checkpoint {i}");
            assert_eq!(checkpoint_at(point), Some(i), "checkpoint {i}");
            // Both edges of the road, and the grass outside, are in it too.
            let outward = point.normalize();
            for distance in [
                TRACK_INNER_RADIUS - middle + 1.0,
                TRACK_OUTER_RADIUS - middle,
            ] {
                let edge = point + outward * distance;
                assert_eq!(checkpoint_at(edge), Some(i), "checkpoint {i} at {edge}");
            }
        }
    }

    #[test]
    fn the_infield_and_the_grid_are_in_no_checkpoint() {
        assert_eq!(checkpoint_at(Vec2::ZERO), None);
        assert_eq!(
            checkpoint_at(Vec2::new(0.0, -TRACK_INNER_RADIUS + 1.0)),
            None
        );
        for slot in 0..8 {
            assert_eq!(checkpoint_at(grid_position(slot)), None, "slot {slot}");
        }
    }

    #[test]
    fn checkpoints_dont_overlap() {
        for (i, a) in CHECKPOINTS.iter().enumerate() {
            for b in &CHECKPOINTS[i + 1..] {
                assert!(a.intersect(*b).is_empty(), "{a:?} and {b:?}");
            }
        }
    }
}
