//! Shootout: every player gets a gunner on its own spawn point, on every peer, walks and aims
//! it, and the shootout is abandoned when the other players leave.

use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use lightyear::prelude::{Interpolated, PeerId, Predicted};
use smocka::camera::GameplayCamera;
use smocka::config::display::{VIEW_HEIGHT, VIEW_WIDTH};
use smocka::minigames::Minigame;
use smocka::minigames::outcome::MinigameResult;
use smocka::minigames::shootout::config::{HITS_TO_KNOCK_OUT, SPAWNS};
use smocka::minigames::shootout::gunner::{Gunner, GunnerState, Health, normalize_angle};
use smocka::minigames::shootout::shots::PaintBall;
use smocka::party::PartyRules;
use smocka::session::SessionPhase;

use crate::common::{TestApp, connected_pair, update_for, update_until};

/// Each gunner this app can see: its player, its state, and whether it's predicted here
/// (rather than interpolated).
fn gunners(app: &mut TestApp) -> Vec<(PeerId, GunnerState, bool)> {
    app.0
        .world_mut()
        .query::<(&Gunner, &GunnerState, Has<Predicted>, Has<Interpolated>)>()
        .iter(app.0.world())
        .map(|(gunner, state, predicted, _)| (gunner.owner, *state, predicted))
        .collect()
}

/// The host's own player has a local peer id; the joiner's comes from netcode.
fn hosts(owner: PeerId) -> bool {
    matches!(owner, PeerId::Local(_))
}

fn joiners(owner: PeerId) -> bool {
    matches!(owner, PeerId::Netcode(_))
}

/// Where the gunner whose player matches `is_owner` is on this app, if it has it.
fn position(app: &mut TestApp, is_owner: impl Fn(PeerId) -> bool) -> Option<Vec2> {
    gunners(app)
        .into_iter()
        .find(|(owner, _, _)| is_owner(*owner))
        .map(|(_, state, _)| state.position)
}

/// Which way the gunner whose player matches `is_owner` aims on this app, if it has it.
fn aim(app: &mut TestApp, is_owner: impl Fn(PeerId) -> bool) -> Option<f32> {
    gunners(app)
        .into_iter()
        .find(|(owner, _, _)| is_owner(*owner))
        .map(|(_, state, _)| state.aim)
}

fn close_angle(a: f32, b: f32) -> bool {
    normalize_angle(a - b).abs() < 0.05
}

fn hold(app: &mut TestApp, key: KeyCode) {
    app.0
        .world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}

fn hold_mouse(app: &mut TestApp, button: MouseButton) {
    app.0
        .world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(button);
}

fn balls_from(app: &mut TestApp, is_shooter: impl Fn(PeerId) -> bool) -> Vec<PaintBall> {
    let world = app.0.world_mut();
    world
        .query::<&PaintBall>()
        .iter(world)
        .filter(|ball| is_shooter(ball.shooter))
        .copied()
        .collect()
}

fn health(app: &mut TestApp, is_owner: impl Fn(PeerId) -> bool) -> Option<u32> {
    let world = app.0.world_mut();
    world
        .query::<(&Gunner, &Health)>()
        .iter(world)
        .find(|(gunner, _)| is_owner(gunner.owner))
        .map(|(_, health)| health.remaining)
}

/// Give a headless app a 640×360 primary window and matching gameplay camera so mouse cursor
/// positions map directly onto the arena's world coordinates.
fn spawn_window_and_camera(app: &mut TestApp) -> Entity {
    let mut projection = OrthographicProjection::default_2d();
    projection.update(VIEW_WIDTH, VIEW_HEIGHT);
    let camera = Camera {
        computed: ComputedCameraValues {
            clip_from_view: projection.get_clip_from_view(),
            target_info: Some(RenderTargetInfo {
                physical_size: UVec2::new(VIEW_WIDTH as u32, VIEW_HEIGHT as u32),
                scale_factor: 1.0,
            }),
            ..default()
        },
        ..default()
    };
    let world = app.0.world_mut();
    world.spawn((GameplayCamera, camera, GlobalTransform::IDENTITY));
    world.spawn((Window::default(), PrimaryWindow)).id()
}

/// Connect a host and a joiner, start a shootout, and wait until both see both gunners.
fn shootout_pair() -> (TestApp, TestApp) {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);
    host.start_minigame(Minigame::Shootout);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            app.phase() == Some(SessionPhase::Playing(Minigame::Shootout))
                && gunners(app).len() == 2
        })
    });
    (host, client)
}

#[test]
fn every_player_gets_a_gunner_on_its_own_spawn_point_on_every_peer() {
    let (mut host, mut client) = shootout_pair();
    for app in [&mut host, &mut client] {
        let gunners = gunners(app);
        let positions: Vec<Vec2> = gunners.iter().map(|(_, state, _)| state.position).collect();
        assert!(
            positions.iter().all(|position| SPAWNS.contains(position)),
            "{positions:?}"
        );
        assert_ne!(positions[0], positions[1]);
    }
    // The joiner predicts their own gunner and interpolates the host's.
    for (owner, _, predicted) in gunners(&mut client) {
        assert_eq!(predicted, joiners(owner), "{owner:?}");
    }
    // The host holds both as they really are: it would skip interpolated ones when walking them.
    let world = host.0.world_mut();
    let interpolated = world
        .query_filtered::<(), (With<Gunner>, With<Interpolated>)>()
        .iter(world)
        .count();
    assert_eq!(interpolated, 0);
}

#[test]
fn the_shootout_is_abandoned_when_the_other_player_leaves() {
    let (mut host, mut client) = shootout_pair();
    client.leave();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].phase() == Some(SessionPhase::Lobby) && gunners(apps[0]).is_empty()
    });
}

#[test]
fn the_joiner_walks_their_gunner_on_every_peer() {
    let (mut host, mut client) = shootout_pair();
    let start = position(&mut host, joiners).unwrap();
    // The joiner starts in the bottom right corner: walk up and left, into the arena.
    hold(&mut client, KeyCode::KeyW);
    hold(&mut client, KeyCode::KeyA);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            position(app, joiners)
                .is_some_and(|now| now.x < start.x - 20.0 && now.y > start.y + 20.0)
        })
    });
    // Nobody touched the host's gunner.
    assert!(SPAWNS.contains(&position(&mut host, hosts).unwrap()));
}

#[test]
fn the_host_walks_their_gunner_on_every_peer() {
    let (mut host, mut client) = shootout_pair();
    let start = position(&mut host, hosts).unwrap();
    hold(&mut host, KeyCode::ArrowRight);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| position(app, hosts).is_some_and(|now| now.x > start.x + 30.0))
    });
}

#[test]
fn the_joiner_aims_with_the_right_stick_on_every_peer() {
    let (mut host, mut client) = shootout_pair();
    let mut gamepad = Gamepad::default();
    gamepad.analog_mut().set(GamepadAxis::RightStickX, 0.0);
    gamepad.analog_mut().set(GamepadAxis::RightStickY, 1.0);
    let gamepad_entity = client.0.world_mut().spawn(gamepad).id();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| aim(app, joiners).is_some_and(|now| close_angle(now, FRAC_PI_2)))
    });

    // Releasing the stick keeps the gunner aimed up.
    {
        let mut gamepad = client
            .0
            .world_mut()
            .get_mut::<Gamepad>(gamepad_entity)
            .unwrap();
        gamepad.analog_mut().set(GamepadAxis::RightStickX, 0.0);
        gamepad.analog_mut().set(GamepadAxis::RightStickY, 0.0);
    }
    update_for(&mut [&mut host, &mut client], Duration::from_millis(100));
    for app in [&mut host, &mut client] {
        let now = aim(app, joiners).unwrap();
        assert!(close_angle(now, FRAC_PI_2), "{now}");
    }
}

#[test]
fn the_host_aims_with_the_mouse_on_every_peer() {
    let (mut host, mut client) = shootout_pair();
    let start = position(&mut host, hosts).unwrap();
    let window = spawn_window_and_camera(&mut host);

    // Top-middle of the 640×360 window maps to world (0, 180).
    let target = Vec2::new(0.0, VIEW_HEIGHT / 2.0);
    let expected = (target - start).to_angle();
    host.0
        .world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(VIEW_WIDTH / 2.0, 0.0)));

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| aim(app, hosts).is_some_and(|now| close_angle(now, expected)))
    });

    // Moving the cursor to the bottom-left corner maps to world (-320, -180).
    let second_target = Vec2::new(-VIEW_WIDTH / 2.0, -VIEW_HEIGHT / 2.0);
    let second_expected = (second_target - start).to_angle();
    host.0
        .world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(0.0, VIEW_HEIGHT)));

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| aim(app, hosts).is_some_and(|now| close_angle(now, second_expected)))
    });
}

#[test]
fn firing_spawns_a_paint_ball_locally_for_the_shooter_and_on_other_peers() {
    let (mut host, mut client) = shootout_pair();

    // Joiner fires with the default left mouse button binding: spawns a cosmetic ball on the
    // joiner immediately and an authoritative ball on the host (no duplicate on the joiner).
    hold_mouse(&mut client, MouseButton::Left);
    update_until(&mut [&mut host, &mut client], |apps| {
        !balls_from(apps[0], joiners).is_empty() && !balls_from(apps[1], joiners).is_empty()
    });
    assert!(
        balls_from(&mut host, joiners)
            .iter()
            .all(|b| b.authoritative)
    );
    assert!(
        balls_from(&mut client, joiners)
            .iter()
            .all(|b| !b.authoritative)
    );
    assert_eq!(balls_from(&mut client, joiners).len(), 1);

    // Host fires with Space: joiner receives ShotFired and spawns a cosmetic ball for it.
    hold(&mut host, KeyCode::Space);
    update_until(&mut [&mut host, &mut client], |apps| {
        !balls_from(apps[0], hosts).is_empty() && !balls_from(apps[1], hosts).is_empty()
    });
    assert!(balls_from(&mut host, hosts).iter().all(|b| b.authoritative));
    assert!(
        balls_from(&mut client, hosts)
            .iter()
            .all(|b| !b.authoritative)
    );
}

#[test]
fn cover_blocks_shots_between_opposite_corners() {
    let (mut host, mut client) = shootout_pair();
    // Default spawns (-280, 140) and (280, -140) aim straight at (0, 0), through the middle cover.
    hold(&mut host, KeyCode::Space);
    update_for(&mut [&mut host, &mut client], Duration::from_millis(500));
    assert_eq!(health(&mut host, joiners), Some(HITS_TO_KNOCK_OUT));
    assert_eq!(health(&mut client, joiners), Some(HITS_TO_KNOCK_OUT));
}

#[test]
fn knocking_out_the_other_gunner_wins_the_shootout_on_every_peer() {
    let (mut host, mut client) = shootout_pair();
    // Put both gunners in the clear top lane facing each other.
    {
        let world = host.0.world_mut();
        for (gunner, mut state) in world.query::<(&Gunner, &mut GunnerState)>().iter_mut(world) {
            if hosts(gunner.owner) {
                state.position = Vec2::new(-100.0, 140.0);
                state.aim = 0.0;
            } else {
                state.position = Vec2::new(100.0, 140.0);
                state.aim = std::f32::consts::PI;
            }
        }
    }
    // Wait for the joiner to receive the repositioned gunner, then aim left with the right stick
    // and hold left click to knock out the host.
    update_until(&mut [&mut host, &mut client], |apps| {
        position(apps[1], joiners).is_some_and(|p| p.distance(Vec2::new(100.0, 140.0)) < 1.0)
    });
    let mut gamepad = Gamepad::default();
    gamepad.analog_mut().set(GamepadAxis::RightStickX, -1.0);
    gamepad.analog_mut().set(GamepadAxis::RightStickY, 0.0);
    client.0.world_mut().spawn(gamepad);
    hold_mouse(&mut client, MouseButton::Left);

    // First verify damage replicates before the knockout.
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| health(app, hosts).is_some_and(|hp| hp < HITS_TO_KNOCK_OUT))
    });

    // Keep firing until the host's gunner is knocked out and both peers reach Results.
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.phase() == Some(SessionPhase::Results))
    });
    for app in [&mut host, &mut client] {
        let world = app.0.world_mut();
        let result = world.query::<&MinigameResult>().single(world).unwrap();
        assert_eq!(result.placements.len(), 2);
        assert!(joiners(result.placements[0].player));
        assert_eq!(result.placements[0].place, 1);
        assert!(hosts(result.placements[1].player));
        assert_eq!(result.placements[1].place, 2);
    }
}

#[test]
fn parties_include_shootout_by_default() {
    assert!(PartyRules::default().games.contains(&Minigame::Shootout));
}
