//! Shootout's panning camera: follows this player's gunner smoothly with subtle aim look-ahead so
//! downrange cover and incoming shots come into view cleanly, clamped inside the arena's outer
//! walls so the void outside never shows. Spectators and knocked-out players follow a surviving
//! gunner the same way.

use std::cmp::Reverse;

use bevy::prelude::*;
use lightyear::prelude::input::native::InputMarker;

use super::SyncGunnerTransform;
use crate::camera::gameplay_camera_2d;
use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};
use crate::minigames::shootout::config::{ARENA_HEIGHT, ARENA_WIDTH};
use crate::minigames::shootout::gunner::{Gunner, GunnerState, Health};
use crate::minigames::shootout::{PHASE, ShootoutInput};

/// How far downrange along the gunner's aim direction the camera shifts, in world units.
const AIM_LOOK_AHEAD: f32 = 36.0;
/// Exponential follow rate (per second): crisp enough for aiming while gliding smoothly.
const FOLLOW_RATE: f32 = 8.0;

/// Shootout's gameplay camera and its first-frame snapping state.
#[derive(Component, Default)]
pub struct ShootoutCamera {
    initialized: bool,
    snapped_to_own: bool,
}

/// Spawns and pans the shootout gameplay camera.
pub(super) struct ShootoutCameraPlugin;

impl Plugin for ShootoutCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(PHASE), spawn_camera);
        app.add_systems(
            PostUpdate,
            follow_gunner
                .after(SyncGunnerTransform)
                .before(TransformSystems::Propagate)
                .run_if(in_state(PHASE)),
        );
    }
}

fn spawn_camera(mut commands: Commands) {
    debug!("Spawning the shootout camera");
    commands.spawn((
        ShootoutCamera::default(),
        gameplay_camera_2d(PHASE, VIEW_WIDTH, VIEW_HEIGHT),
    ));
}

/// Where the camera wants to aim for a gunner at `position` aiming along `aim`.
pub fn camera_target(position: Vec2, aim: f32) -> Vec2 {
    position + Vec2::from_angle(aim) * AIM_LOOK_AHEAD
}

/// `target` clamped so a viewport of `view_size` stays inside an arena of `arena_size` (centred
/// on `(0, 0)`). If the viewport is larger than the arena on an axis, it centres on `0.0`.
pub fn clamp_camera(target: Vec2, arena_size: Vec2, view_size: Vec2) -> Vec2 {
    let limit = ((arena_size - view_size) / 2.0).max(Vec2::ZERO);
    target.clamp(-limit, limit)
}

/// Move `current` smoothly toward `target` over `dt` seconds with frame-rate-independent damping.
pub fn step_camera(current: Vec2, target: Vec2, dt: f32) -> Vec2 {
    let alpha = 1.0 - (-FOLLOW_RATE * dt.max(0.0)).exp();
    current.lerp(target, alpha)
}

type FocusGunner = (
    &'static Gunner,
    &'static GunnerState,
    &'static Health,
    &'static Transform,
    Has<InputMarker<ShootoutInput>>,
);

/// Pan the shootout camera to follow this player's gunner (or a surviving gunner for spectators /
/// knocked-out players), smoothly with aim look-ahead and clamped inside the arena.
fn follow_gunner(
    time: Res<Time>,
    gunners: Query<FocusGunner>,
    camera: Single<(&mut Transform, &Projection, &mut ShootoutCamera), Without<Gunner>>,
) {
    let focus = gunners
        .iter()
        .find(|(_, _, _, _, own)| *own)
        .map(|(_, state, _, transform, _)| (state.aim, transform.translation.truncate(), true))
        .or_else(|| {
            gunners
                .iter()
                .min_by_key(|(gunner, _, health, _, _)| (Reverse(health.remaining), gunner.owner))
                .map(|(_, state, _, transform, _)| {
                    (state.aim, transform.translation.truncate(), false)
                })
        });
    let Some((aim, position, is_own)) = focus else {
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
    let arena_size = Vec2::new(ARENA_WIDTH, ARENA_HEIGHT);
    let desired = clamp_camera(camera_target(position, aim), arena_size, view_size);

    let next = if !follow.initialized || (is_own && !follow.snapped_to_own) {
        follow.initialized = true;
        follow.snapped_to_own = is_own;
        desired
    } else {
        let current = cam_transform.translation.truncate();
        clamp_camera(
            step_camera(current, desired, time.delta_secs()),
            arena_size,
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
        AIM_LOOK_AHEAD, ShootoutCamera, camera_target, clamp_camera, follow_gunner, step_camera,
    };
    use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};
    use crate::minigames::shootout::ShootoutInput;
    use crate::minigames::shootout::config::{ARENA_HEIGHT, ARENA_WIDTH};
    use crate::minigames::shootout::gunner::{Gunner, GunnerState, Health};

    #[test]
    fn aim_look_ahead_shifts_downrange_and_clamp_stays_inside_arena() {
        let target = camera_target(Vec2::new(10.0, 20.0), 0.0);
        assert!((target - Vec2::new(10.0 + AIM_LOOK_AHEAD, 20.0)).length() < 1e-4);

        let arena = Vec2::new(ARENA_WIDTH, ARENA_HEIGHT);
        let view = Vec2::new(VIEW_WIDTH, VIEW_HEIGHT);
        let max = (arena - view) / 2.0;
        assert_eq!(
            clamp_camera(Vec2::new(500.0, -500.0), arena, view),
            Vec2::new(max.x, -max.y)
        );
        assert_eq!(
            clamp_camera(
                Vec2::new(50.0, 200.0),
                arena,
                Vec2::new(VIEW_WIDTH, ARENA_HEIGHT + 100.0)
            ),
            Vec2::new(50.0, 0.0)
        );
    }

    #[test]
    fn step_camera_glides_toward_target_without_overshooting() {
        let after_frame = step_camera(Vec2::ZERO, Vec2::new(100.0, 0.0), 1.0 / 60.0);
        assert!(after_frame.x > 0.0 && after_frame.x < 100.0);
        let after_second = step_camera(Vec2::ZERO, Vec2::new(100.0, 0.0), 1.0);
        assert!(after_second.x > 99.0 && after_second.x <= 100.0);
    }

    fn camera_world(dt: f32) -> (World, Entity) {
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_secs_f32(dt));
        world.insert_resource(time);
        let camera = world
            .spawn((
                ShootoutCamera::default(),
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
    fn camera_snaps_to_own_gunner_then_pans_and_falls_back_to_survivor_on_knockout() {
        let (mut world, camera) = camera_world(1.0 / 60.0);
        let own = world
            .spawn((
                Gunner {
                    owner: PeerId::Netcode(1),
                    name: "Ada".into(),
                    color: 0,
                },
                GunnerState {
                    position: Vec2::new(-50.0, 20.0),
                    aim: 0.0,
                },
                Health::full(3),
                InputMarker::<ShootoutInput>::default(),
                Transform::from_xyz(-50.0, 20.0, 1.0),
            ))
            .id();
        world.spawn((
            Gunner {
                owner: PeerId::Netcode(2),
                name: "Bo".into(),
                color: 1,
            },
            GunnerState {
                position: Vec2::new(100.0, -30.0),
                aim: std::f32::consts::PI,
            },
            Health::full(3),
            Transform::from_xyz(100.0, -30.0, 1.0),
        ));

        // First frame snaps straight to own gunner + look-ahead.
        world.run_system_once(follow_gunner).unwrap();
        let first = world
            .get::<Transform>(camera)
            .unwrap()
            .translation
            .truncate();
        assert!((first - Vec2::new(-50.0 + AIM_LOOK_AHEAD, 20.0)).length() < 1e-3);

        // Moving own gunner pans smoothly rather than jumping.
        world.get_mut::<Transform>(own).unwrap().translation = Vec3::new(30.0, 20.0, 1.0);
        world.run_system_once(follow_gunner).unwrap();
        let second = world
            .get::<Transform>(camera)
            .unwrap()
            .translation
            .truncate();
        assert!(second.x > first.x && second.x < 30.0 + AIM_LOOK_AHEAD);

        // After own gunner is knocked out, camera pans smoothly toward the surviving gunner.
        world.despawn(own);
        world.run_system_once(follow_gunner).unwrap();
        let spectating = world
            .get::<Transform>(camera)
            .unwrap()
            .translation
            .truncate();
        assert!(spectating.y < second.y, "{spectating} vs {second}");
    }
}
