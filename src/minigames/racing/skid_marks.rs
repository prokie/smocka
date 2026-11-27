//! Skid marks: dark strips behind a car's rear wheels when it corners or brakes hard, or slides
//! after a bump, fading away after a few seconds. Local to each peer and worked out from the car states it sees, so nothing
//! extra is sent over the network.

use bevy::prelude::*;

use super::PHASE;
use super::car::{Car, CarState};
use super::track::{Surface, TrackId};

/// Cornering this hard (speed × turn rate, world units/s²) leaves marks. At full steering that's
/// from about two thirds of top speed.
const CORNERING: f32 = 450.0;
/// Slowing down this fast (world units/s²) leaves marks: hard braking, not coasting.
const BRAKING: f32 = 350.0;
/// Slower cars never leave marks from cornering or braking.
const MIN_SPEED: f32 = 60.0;
/// Sliding this fast after a bump (see [`CarState::slide`], world units/s) leaves marks, whatever
/// the car's own speed.
const SLIDING: f32 = 40.0;
/// A car that moved further than this between two frames was snapped into place (by a rollback,
/// say): don't draw a mark across the jump.
const MAX_STEP: f32 = 12.0;

/// The rear wheels, relative to the car's centre with the car pointing right. Matches `car.png`.
const REAR_WHEELS: [Vec2; 2] = [Vec2::new(-4.1, 4.2), Vec2::new(-4.1, -4.2)];
const WIDTH: f32 = 1.6;
const COLOR: Color = Color::srgba(0.07, 0.07, 0.09, 0.45);
const FADE_SECONDS: f32 = 3.0;
/// Above the track, below the cars.
const Z: f32 = 0.5;

/// Where a car was last frame, to tell how it is moving.
#[derive(Component, Debug, Default)]
struct SkidTrail {
    last: Option<CarState>,
}

/// One strip of a skid mark, fading out.
#[derive(Component, Debug)]
struct SkidMark {
    fade: Timer,
}

/// Leaves and fades skid marks during races. Added by
/// [`RacingVisualsPlugin`](super::RacingVisualsPlugin).
pub struct SkidMarksPlugin;

impl Plugin for SkidMarksPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(track_car);
        app.add_systems(
            Update,
            (leave_skid_marks, fade_skid_marks).run_if(in_state(PHASE)),
        );
    }
}

fn track_car(trigger: On<Add<Car>>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(SkidTrail::default());
}

/// Whether a car that went from `before` to `after` in `dt` seconds is skidding: sliding after a
/// bump, or cornering or braking hard enough and not too slow.
pub fn is_skidding(before: CarState, after: CarState, dt: f32) -> bool {
    if after.slide.length() > SLIDING {
        return true;
    }
    if dt <= 0.0 || after.speed.abs() < MIN_SPEED {
        return false;
    }
    let turned = Vec2::from_angle(before.heading).angle_to(Vec2::from_angle(after.heading));
    let cornering = (turned / dt).abs() * after.speed.abs();
    let braking = (before.speed.abs() - after.speed.abs()) / dt;
    cornering > CORNERING || braking > BRAKING
}

/// Lay a strip behind each rear wheel of every car that is skidding on the race's road, from
/// where the wheel was last frame to where it is now. Nothing until the race's [`TrackId`] has
/// arrived.
fn leave_skid_marks(
    time: Res<Time>,
    track: Option<Single<&TrackId>>,
    mut cars: Query<(&CarState, &mut SkidTrail)>,
    mut commands: Commands,
) {
    let Some(track) = track.map(|id| id.track()) else {
        return;
    };
    for (state, mut trail) in &mut cars {
        let Some(before) = trail.last.replace(*state) else {
            continue;
        };
        let step = state.position.distance(before.position);
        if step > MAX_STEP
            || track.surface_at(state.position) != Surface::Road
            || !is_skidding(before, *state, time.delta_secs())
        {
            continue;
        }
        for wheel in REAR_WHEELS {
            let from = before.position + Vec2::from_angle(before.heading).rotate(wheel);
            let to = state.position + Vec2::from_angle(state.heading).rotate(wheel);
            spawn_mark(&mut commands, from, to);
        }
    }
}

fn spawn_mark(commands: &mut Commands, from: Vec2, to: Vec2) {
    let along = to - from;
    // A little longer than the gap, so strips meet without seams.
    let length = along.length() + WIDTH / 2.0;
    commands.spawn((
        SkidMark {
            fade: Timer::from_seconds(FADE_SECONDS, TimerMode::Once),
        },
        DespawnOnExit(PHASE),
        Sprite::from_color(COLOR, Vec2::new(length, WIDTH)),
        Transform::from_translation(((from + to) / 2.0).extend(Z))
            .with_rotation(Quat::from_rotation_z(along.to_angle())),
    ));
}

fn fade_skid_marks(
    time: Res<Time>,
    mut marks: Query<(Entity, &mut SkidMark, &mut Sprite)>,
    mut commands: Commands,
) {
    for (entity, mut mark, mut sprite) in &mut marks {
        mark.fade.tick(time.delta());
        if mark.fade.is_finished() {
            commands.entity(entity).despawn();
        } else {
            sprite.color = COLOR.with_alpha(COLOR.alpha() * mark.fade.fraction_remaining());
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;

    use super::is_skidding;
    use crate::minigames::racing::car::CarState;
    use crate::minigames::racing::config::{MAX_SPEED, TURN_RATE};

    const DT: f32 = 1.0 / 60.0;

    fn car(speed: f32, heading: f32) -> CarState {
        CarState {
            speed,
            ..CarState::parked(Vec2::ZERO, heading)
        }
    }

    #[test]
    fn driving_straight_leaves_no_marks() {
        assert!(!is_skidding(car(MAX_SPEED, 0.0), car(MAX_SPEED, 0.0), DT));
    }

    #[test]
    fn full_steering_at_top_speed_leaves_marks() {
        let turned = TURN_RATE * DT;
        assert!(is_skidding(car(MAX_SPEED, 0.0), car(MAX_SPEED, turned), DT));
        // The same turn the other way, and across the ±π seam.
        assert!(is_skidding(
            car(MAX_SPEED, 0.0),
            car(MAX_SPEED, -turned),
            DT
        ));
        assert!(is_skidding(
            car(MAX_SPEED, 3.13),
            car(MAX_SPEED, 3.13 + turned - std::f32::consts::TAU),
            DT
        ));
    }

    #[test]
    fn gentle_turns_and_slow_cars_leave_no_marks() {
        let half_steering = TURN_RATE * DT / 2.0;
        assert!(!is_skidding(
            car(MAX_SPEED, 0.0),
            car(MAX_SPEED, half_steering),
            DT
        ));
        assert!(!is_skidding(car(40.0, 0.0), car(40.0, 1.0), DT));
    }

    #[test]
    fn sliding_after_a_bump_leaves_marks_even_standing_still() {
        let shoved = CarState {
            slide: Vec2::new(0.0, 100.0),
            ..car(0.0, 0.0)
        };
        assert!(is_skidding(car(0.0, 0.0), shoved, DT));
        let nudged = CarState {
            slide: Vec2::new(0.0, 10.0),
            ..car(0.0, 0.0)
        };
        assert!(!is_skidding(car(0.0, 0.0), nudged, DT));
    }

    #[test]
    fn hard_braking_leaves_marks_but_coasting_doesnt() {
        assert!(is_skidding(
            car(150.0, 0.0),
            car(150.0 - 500.0 * DT, 0.0),
            DT
        ));
        assert!(!is_skidding(
            car(150.0, 0.0),
            car(150.0 - 150.0 * DT, 0.0),
            DT
        ));
    }
}
