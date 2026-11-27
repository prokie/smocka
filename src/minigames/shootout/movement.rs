//! Walking, aiming and gunner-to-gunner collision: gunners move at a fixed speed in eight
//! directions, slide along the border wall and cover rather than going through, turn toward their
//! input's aim, and push apart instead of overlapping each other.

use bevy::prelude::*;
use lightyear::prelude::input::native::ActionState;
use lightyear::prelude::*;

use super::config::{ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH, COVER, GUNNER_RADIUS, GUNNER_SPEED};
use super::gunner::{Gunner, GunnerState};
use super::shootout_input::ShootoutInput;
use super::{PHASE, ShootoutSystems};

/// Separation passes per tick: resolves chains of gunners pushed against each other or a wall.
const SEPARATION_PASSES: usize = 3;

/// Walks the gunners each tick and keeps them from overlapping each other.
pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (walk_gunners, separate_gunners)
                .chain()
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

/// Push overlapping gunners apart so players can't walk through each other.
///
/// On the host every gunner is simulated, so touching pairs share the push equally in a
/// deterministic order by [`Gunner::owner`]. On a joiner only this player's own gunner is
/// simulated while everyone else's is [`Interpolated`], so the local gunner is pushed out of
/// other gunners without modifying interpolated components.
fn separate_gunners(
    mut simulated: Query<(Entity, &Gunner, &mut GunnerState), Without<Interpolated>>,
    interpolated: Query<&GunnerState, With<Interpolated>>,
) {
    let mut order: Vec<(PeerId, Entity)> = simulated
        .iter()
        .map(|(entity, gunner, _)| (gunner.owner, entity))
        .collect();
    order.sort_by_key(|(owner, _)| *owner);

    let others: Vec<Vec2> = interpolated.iter().map(|state| state.position).collect();

    for _ in 0..SEPARATION_PASSES {
        for i in 0..order.len() {
            for j in (i + 1)..order.len() {
                let Ok([(_, _, mut a), (_, _, mut b)]) =
                    simulated.get_many_mut([order[i].1, order[j].1])
                else {
                    continue;
                };
                let (next_a, next_b) = separate_pair(a.position, b.position);
                if next_a != a.position {
                    a.position = next_a;
                }
                if next_b != b.position {
                    b.position = next_b;
                }
            }
        }
        if !others.is_empty() {
            for (_, _, mut state) in &mut simulated {
                let mut position = state.position;
                for &other in &others {
                    position = push_out_of_gunner(position, other);
                }
                if position != state.position {
                    state.position = position;
                }
            }
        }
    }
}

/// Two gunners' positions pushed apart equally if they overlap, each kept out of walls and cover.
pub fn separate_pair(a: Vec2, b: Vec2) -> (Vec2, Vec2) {
    let gap = b - a;
    let min_distance = 2.0 * GUNNER_RADIUS;
    let distance = gap.length();
    if distance >= min_distance {
        return (a, b);
    }
    let dir = gap.try_normalize().unwrap_or(Vec2::X);
    let half_push = dir * ((min_distance - distance) / 2.0);
    (
        keep_out_of_walls(a - half_push),
        keep_out_of_walls(b + half_push),
    )
}

/// Push a single gunner at `position` out of a gunner at `other`, keeping it out of walls and
/// cover. Used on clients so a predicted gunner doesn't walk through interpolated opponents.
pub fn push_out_of_gunner(position: Vec2, other: Vec2) -> Vec2 {
    let gap = position - other;
    let min_distance = 2.0 * GUNNER_RADIUS;
    let distance = gap.length();
    if distance >= min_distance {
        return position;
    }
    let dir = gap.try_normalize().unwrap_or(Vec2::X);
    keep_out_of_walls(other + dir * min_distance)
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

    use super::{keep_out_of_walls, push_out_of_gunner, separate_pair, walk};
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

    #[test]
    fn overlapping_gunners_are_pushed_apart_without_entering_walls() {
        let (a, b) = separate_pair(Vec2::new(0.0, 120.0), Vec2::new(4.0, 120.0));
        assert!(
            (b.distance(a) - 2.0 * GUNNER_RADIUS).abs() < 1e-3,
            "{a} {b}"
        );

        // Two gunners on the exact same spot still separate cleanly.
        let (a, b) = separate_pair(Vec2::new(0.0, 120.0), Vec2::new(0.0, 120.0));
        assert!(
            (b.distance(a) - 2.0 * GUNNER_RADIUS).abs() < 1e-3,
            "{a} {b}"
        );

        // A gunner pinned against the right border wall is never pushed past it.
        let edge = ARENA_HALF_WIDTH - GUNNER_RADIUS;
        let (left, right) = separate_pair(Vec2::new(edge - 4.0, 120.0), Vec2::new(edge, 120.0));
        assert!(right.x <= edge + 1e-3, "{right}");
        assert!(left.x < right.x);

        // Single-sided separation stops a predicted gunner at 2 * GUNNER_RADIUS.
        let pushed = push_out_of_gunner(Vec2::new(10.0, 120.0), Vec2::new(0.0, 120.0));
        assert!(
            (pushed.distance(Vec2::new(0.0, 120.0)) - 2.0 * GUNNER_RADIUS).abs() < 1e-3,
            "{pushed}"
        );
    }
}
