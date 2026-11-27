//! Racing's panning camera: follows your own car smoothly with velocity look-ahead so upcoming
//! corners come into view in time, clamped to the track's edges so the void outside never shows.
//! Spectators follow the race leader the same way.

use bevy::prelude::*;
use lightyear::prelude::input::native::InputMarker;

use super::PHASE;
use super::RacingInput;
use super::car::{Car, CarState};
use super::progress::{RaceProgress, running_order};
use super::track::TrackId;
use super::visuals::sync_car_transform;
use crate::camera::gameplay_camera_2d;
use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

/// How far ahead (in seconds of current velocity) the camera looks.
const LOOK_AHEAD_TIME: f32 = 0.35;
/// Furthest the camera looks ahead of the car, in world units, so boosted cars stay well in view.
const MAX_LOOK_AHEAD: f32 = 90.0;
/// Exponential follow rate (per second): higher tracks tighter, lower glides softer.
const FOLLOW_RATE: f32 = 7.0;

/// Racing's gameplay camera and its snapping state.
#[derive(Component, Default)]
pub struct RacingCamera {
    initialized: bool,
    snapped_to_own: bool,
}

/// Spawns and updates the panning racing camera. Added by
/// [`RacingVisualsPlugin`](super::RacingVisualsPlugin).
pub struct RacingCameraPlugin;

impl Plugin for RacingCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(PHASE), spawn_camera);
        app.add_systems(
            PostUpdate,
            follow_car
                .after(sync_car_transform)
                .before(TransformSystems::Propagate)
                .run_if(in_state(PHASE)),
        );
    }
}

fn spawn_camera(mut commands: Commands) {
    debug!("Spawning the racing camera");
    commands.spawn((
        RacingCamera::default(),
        gameplay_camera_2d(PHASE, VIEW_WIDTH, VIEW_HEIGHT),
    ));
}

/// Where the camera wants to aim for a car at `position` moving at `velocity`: slightly ahead of
/// the car so upcoming bends are visible sooner.
pub fn camera_target(position: Vec2, velocity: Vec2) -> Vec2 {
    position + (velocity * LOOK_AHEAD_TIME).clamp_length_max(MAX_LOOK_AHEAD)
}

/// `target` clamped so a viewport of `view_size` stays inside a track of `track_size` (centred on
/// `(0, 0)`). If the viewport is wider or taller than the track on an axis, it centres on `0.0`.
pub fn clamp_camera(target: Vec2, track_size: Vec2, view_size: Vec2) -> Vec2 {
    let limit = ((track_size - view_size) / 2.0).max(Vec2::ZERO);
    target.clamp(-limit, limit)
}

/// Move `current` smoothly toward `target` over `dt` seconds with frame-rate-independent damping.
pub fn step_camera(current: Vec2, target: Vec2, dt: f32) -> Vec2 {
    let alpha = 1.0 - (-FOLLOW_RATE * dt.max(0.0)).exp();
    current.lerp(target, alpha)
}

/// Each car's state, progress, smoothed transform, and whether it is this player's own.
type FocusCar = (
    &'static CarState,
    &'static RaceProgress,
    &'static Transform,
    Has<InputMarker<RacingInput>>,
);

/// Pan the racing camera to follow this player's car (or the race leader for spectators), smoothly
/// with velocity look-ahead and clamped inside the track.
fn follow_car(
    time: Res<Time>,
    track: Option<Single<&TrackId>>,
    cars: Query<FocusCar, With<Car>>,
    camera: Single<(&mut Transform, &Projection, &mut RacingCamera), Without<Car>>,
) {
    let Some(track) = track.map(|id| id.track()) else {
        return;
    };
    let focus = cars
        .iter()
        .find(|(_, _, _, own)| *own)
        .map(|(state, _, transform, _)| (state, transform, true))
        .or_else(|| {
            cars.iter()
                .min_by_key(|(state, progress, _, _)| {
                    running_order(track, **progress, state.position)
                })
                .map(|(state, _, transform, _)| (state, transform, false))
        });
    let Some((state, car_transform, is_own)) = focus else {
        return;
    };

    let (mut cam_transform, projection, mut follow) = camera.into_inner();
    let view_size = match projection {
        Projection::Orthographic(ortho) => {
            let area = ortho.area.size();
            if area.x > 0.0 && area.y > 0.0 {
                area
            } else {
                Vec2::new(VIEW_WIDTH, VIEW_HEIGHT)
            }
        }
        _ => Vec2::new(VIEW_WIDTH, VIEW_HEIGHT),
    };

    let desired = clamp_camera(
        camera_target(car_transform.translation.truncate(), state.velocity()),
        track.size(),
        view_size,
    );

    let next = if !follow.initialized || (is_own && !follow.snapped_to_own) {
        follow.initialized = true;
        follow.snapped_to_own = is_own;
        desired
    } else {
        let current = cam_transform.translation.truncate();
        clamp_camera(
            step_camera(current, desired, time.delta_secs()),
            track.size(),
            view_size,
        )
    };
    cam_transform.translation.x = next.x;
    cam_transform.translation.y = next.y;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use lightyear::prelude::PeerId;
    use lightyear::prelude::input::native::InputMarker;

    use super::{
        LOOK_AHEAD_TIME, MAX_LOOK_AHEAD, RacingCamera, camera_target, clamp_camera, follow_car,
        step_camera,
    };
    use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};
    use crate::minigames::racing::RacingInput;
    use crate::minigames::racing::car::{Car, CarState};
    use crate::minigames::racing::progress::RaceProgress;
    use crate::minigames::racing::track::TrackId;

    #[test]
    fn look_ahead_leads_the_car_and_caps_at_max() {
        assert_eq!(
            camera_target(Vec2::new(10.0, 20.0), Vec2::ZERO),
            Vec2::new(10.0, 20.0)
        );
        let moving = camera_target(Vec2::ZERO, Vec2::new(200.0, 0.0));
        assert!((moving.x - 200.0 * LOOK_AHEAD_TIME).abs() < 1e-3);
        let fast = camera_target(Vec2::ZERO, Vec2::new(1000.0, 0.0));
        assert!((fast.x - MAX_LOOK_AHEAD).abs() < 1e-3);
    }

    #[test]
    fn clamp_keeps_viewport_inside_track_and_centres_when_oversized() {
        let track = Vec2::new(1280.0, 720.0);
        let view = Vec2::new(640.0, 360.0);
        assert_eq!(
            clamp_camera(Vec2::new(100.0, -50.0), track, view),
            Vec2::new(100.0, -50.0)
        );
        assert_eq!(
            clamp_camera(Vec2::new(500.0, -300.0), track, view),
            Vec2::new(320.0, -180.0)
        );
        // A tall window that sees the whole track height centres vertically on 0.
        assert_eq!(
            clamp_camera(Vec2::new(100.0, 200.0), track, Vec2::new(640.0, 800.0)),
            Vec2::new(100.0, 0.0)
        );
    }

    #[test]
    fn step_camera_moves_towards_target_without_overshooting() {
        let start = Vec2::ZERO;
        let target = Vec2::new(100.0, 0.0);
        let after_tick = step_camera(start, target, 1.0 / 60.0);
        assert!(after_tick.x > 0.0 && after_tick.x < target.x);
        let after_second = step_camera(start, target, 1.0);
        assert!(after_second.x > 99.0 && after_second.x <= target.x);
    }

    fn camera_world(dt: f32) -> (World, Entity) {
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_secs_f32(dt));
        world.insert_resource(time);
        world.spawn(TrackId::FigureEight);
        let camera = world
            .spawn((
                RacingCamera::default(),
                Transform::default(),
                Projection::Orthographic(OrthographicProjection {
                    area: Rect::from_center_size(Vec2::ZERO, Vec2::new(VIEW_WIDTH, VIEW_HEIGHT)),
                    ..OrthographicProjection::default_2d()
                }),
            ))
            .id();
        (world, camera)
    }

    #[test]
    fn camera_snaps_to_own_car_on_first_frame_then_follows_smoothly() {
        let (mut world, camera) = camera_world(1.0 / 60.0);
        let start = Vec2::new(-120.0, -80.0);
        let car = world
            .spawn((
                Car {
                    owner: PeerId::Netcode(1),
                    name: "Ada".into(),
                    color: 0,
                },
                CarState::parked(start, 0.0),
                RaceProgress::on_grid(),
                InputMarker::<RacingInput>::default(),
                Transform::from_translation(start.extend(1.0)),
            ))
            .id();

        world.run_system_once(follow_car).unwrap();
        assert_eq!(
            world
                .get::<Transform>(camera)
                .unwrap()
                .translation
                .truncate(),
            start
        );

        // Move the car forward: next frame pans smoothly rather than teleporting.
        let moved = start + Vec2::new(80.0, 0.0);
        world.get_mut::<Transform>(car).unwrap().translation = moved.extend(1.0);
        world.get_mut::<CarState>(car).unwrap().speed = 200.0;
        world.run_system_once(follow_car).unwrap();
        let pos = world
            .get::<Transform>(camera)
            .unwrap()
            .translation
            .truncate();
        assert!(pos.x > start.x && pos.x < moved.x, "{pos}");
    }

    #[test]
    fn spectator_camera_follows_the_race_leader() {
        let (mut world, camera) = camera_world(1.0 / 60.0);
        let behind_pos = Vec2::new(-200.0, -80.0);
        let leader_pos = Vec2::new(150.0, 100.0);
        world.spawn((
            Car {
                owner: PeerId::Netcode(1),
                name: "Ada".into(),
                color: 0,
            },
            CarState::parked(behind_pos, 0.0),
            RaceProgress::on_grid(),
            Transform::from_translation(behind_pos.extend(1.0)),
        ));
        world.spawn((
            Car {
                owner: PeerId::Netcode(2),
                name: "Bo".into(),
                color: 1,
            },
            CarState::parked(leader_pos, 0.0),
            RaceProgress {
                laps: 1,
                ..RaceProgress::on_grid()
            },
            Transform::from_translation(leader_pos.extend(1.0)),
        ));

        world.run_system_once(follow_car).unwrap();
        assert_eq!(
            world
                .get::<Transform>(camera)
                .unwrap()
                .translation
                .truncate(),
            leader_pos
        );
    }
}
