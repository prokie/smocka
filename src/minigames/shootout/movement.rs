//! Walking and aiming: gunners move at a fixed speed in eight directions, slide along the border
//! wall and cover rather than going through, and turn toward their input's aim.

use bevy::prelude::*;
use lightyear::prelude::input::native::ActionState;
use lightyear::prelude::*;

use super::config::{ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH, COVER, GUNNER_RADIUS, GUNNER_SPEED};
use super::gunner::GunnerState;
use super::shootout_input::ShootoutInput;
use super::{PHASE, ShootoutSystems};

/// Walks the gunners each tick.
pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            walk_gunners
                .in_set(ShootoutSystems::Walk)
                .run_if(in_state(PHASE)),
        );
    }
}

/// Runs on the host for every gunner, and on each player's machine for their own (the one it
/// predicts). Everyone else's is interpolated there, so it's left alone. A gunner with no input
/// yet stands still. Lightyear re-runs it during rollback, so it only reads the input and the
/// state.
fn walk_gunners(
    time: Res<Time>,
    mut gunners: Query<
        (&mut GunnerState, Option<&ActionState<ShootoutInput>>),
        Without<Interpolated>,
    >,
) {
    for (mut state, input) in &mut gunners {
        let input = input.map(|input| input.0).unwrap_or_default();
        let next = walk(*state, input, time.delta_secs());
        state.set_if_neq(next);
    }
}

/// The gunner's state after walking and aiming for `dt` seconds with `input`. Diagonals are no
/// faster than straight lines. A pure function, so Lightyear can re-run it during rollback.
pub fn walk(state: GunnerState, input: ShootoutInput, dt: f32) -> GunnerState {
    let step = input.walk().normalize_or_zero() * GUNNER_SPEED * dt;
    let position = if step == Vec2::ZERO {
        state.position
    } else {
        keep_out_of_walls(state.position + step)
    };
    let aim = input.aim_angle().unwrap_or(state.aim);
    GunnerState { position, aim }
}

/// The nearest place to `position` where a gunner fits: inside the border wall, and out of every
/// cover block. Pushing straight out of what it walked into lets it slide along the wall instead
/// of stopping.
pub fn keep_out_of_walls(position: Vec2) -> Vec2 {
    let mut position = position.clamp(
        Vec2::new(
            -ARENA_HALF_WIDTH + GUNNER_RADIUS,
            -ARENA_HALF_HEIGHT + GUNNER_RADIUS,
        ),
        Vec2::new(
            ARENA_HALF_WIDTH - GUNNER_RADIUS,
            ARENA_HALF_HEIGHT - GUNNER_RADIUS,
        ),
    );
    for block in COVER {
        position = out_of_block(position, block);
    }
    position
}

/// `position` moved just far enough that a gunner there doesn't overlap `block`.
fn out_of_block(position: Vec2, block: Rect) -> Vec2 {
    let nearest = position.clamp(block.min, block.max);
    let gap = position - nearest;
    if gap != Vec2::ZERO {
        let distance = gap.length();
        return if distance < GUNNER_RADIUS {
            nearest + gap / distance * GUNNER_RADIUS
        } else {
            position
        };
    }
    // The centre is inside the block (it only gets there by moving more than its radius in a
    // tick, so not at walking speed): out through the nearest side.
    let to_sides = [
        (
            position.x - block.min.x,
            Vec2::new(block.min.x - GUNNER_RADIUS, position.y),
        ),
        (
            block.max.x - position.x,
            Vec2::new(block.max.x + GUNNER_RADIUS, position.y),
        ),
        (
            position.y - block.min.y,
            Vec2::new(position.x, block.min.y - GUNNER_RADIUS),
        ),
        (
            block.max.y - position.y,
            Vec2::new(position.x, block.max.y + GUNNER_RADIUS),
        ),
    ];
    to_sides
        .into_iter()
        .min_by(|(a, _), (b, _)| a.total_cmp(b))
        .map_or(position, |(_, outside)| outside)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use bevy::math::Vec2;

    use super::{keep_out_of_walls, walk};
    use crate::minigames::shootout::config::{
        ARENA_HALF_WIDTH, COVER, GUNNER_RADIUS, GUNNER_SPEED,
    };
    use crate::minigames::shootout::gunner::GunnerState;
    use crate::minigames::shootout::shootout_input::ShootoutInput;

    const DT: f32 = 1.0 / 128.0;

    fn input(walk_x: i8, walk_y: i8) -> ShootoutInput {
        ShootoutInput {
            walk_x,
            walk_y,
            aim: None,
            fire: false,
        }
    }

    fn at(x: f32, y: f32) -> GunnerState {
        GunnerState {
            position: Vec2::new(x, y),
            aim: 0.0,
        }
    }

    fn close(a: Vec2, b: Vec2) -> bool {
        a.distance(b) < 1e-3
    }

    #[test]
    fn walks_at_its_speed_and_no_faster_diagonally() {
        let start = at(100.0, 100.0);
        let right = walk(start, input(1, 0), 1.0);
        assert!(close(
            right.position,
            Vec2::new(100.0 + GUNNER_SPEED, 100.0)
        ));
        let diagonal = walk(start, input(1, 1), DT);
        let distance = diagonal.position.distance(start.position);
        assert!((distance - GUNNER_SPEED * DT).abs() < 1e-3, "{distance}");
    }

    #[test]
    fn stands_still_without_input() {
        let start = at(-30.0, 120.0);
        assert_eq!(walk(start, ShootoutInput::default(), DT), start);
    }

    #[test]
    fn turns_to_aim_even_while_standing_still() {
        let start = at(50.0, 100.0);
        let turned = walk(
            start,
            ShootoutInput {
                aim: Some(FRAC_PI_2),
                ..ShootoutInput::default()
            },
            DT,
        );
        assert_eq!(turned.position, start.position);
        assert!((turned.aim - FRAC_PI_2).abs() < 1e-5, "{}", turned.aim);
    }

    #[test]
    fn keeps_its_aim_when_input_has_none_or_nan() {
        let start = GunnerState {
            position: Vec2::new(50.0, 100.0),
            aim: FRAC_PI_2,
        };
        assert_eq!(walk(start, ShootoutInput::default(), DT), start);
        assert_eq!(
            walk(
                start,
                ShootoutInput {
                    aim: Some(f32::NAN),
                    ..ShootoutInput::default()
                },
                DT,
            ),
            start
        );
    }

    #[test]
    fn stops_at_the_border_wall() {
        let edge = ARENA_HALF_WIDTH - GUNNER_RADIUS;
        let state = walk(at(edge - 1.0, 120.0), input(1, 0), 1.0);
        assert_eq!(state.position, Vec2::new(edge, 120.0));
    }

    #[test]
    fn slides_along_cover_it_walks_into_at_an_angle() {
        // Walking up and right into the bottom of the middle block.
        let block = COVER[0];
        let below = Vec2::new(0.0, block.min.y - GUNNER_RADIUS);
        let mut state = at(below.x, below.y);
        // Not far enough to reach its corner.
        for _ in 0..20 {
            state = walk(state, input(1, 1), DT);
        }
        assert!(
            (state.position.y - below.y).abs() < 1e-3,
            "{}",
            state.position
        );
        assert!(state.position.x > 10.0, "{}", state.position);
    }

    #[test]
    fn never_ends_up_inside_cover() {
        for block in COVER {
            for point in [
                block.center(),
                block.min,
                block.max,
                block.center() + Vec2::X,
            ] {
                let kept = keep_out_of_walls(point);
                let nearest = kept.clamp(block.min, block.max);
                assert!(
                    kept.distance(nearest) >= GUNNER_RADIUS - 1e-3,
                    "{point} -> {kept} overlaps {block:?}"
                );
            }
        }
    }
}
