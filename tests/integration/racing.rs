//! Racing: every player gets a car, each player's driving shows up on every peer, and the host
//! counts laps.

use bevy::prelude::*;
use lightyear::prelude::PeerId;
use smocka::minigames::Minigame;
use smocka::minigames::racing::car::{Car, CarState};
use smocka::minigames::racing::progress::RaceProgress;
use smocka::minigames::racing::track::{CHECKPOINTS, Surface, surface_at};
use smocka::session::SessionPhase;

use crate::common::{TestApp, connected_pair, update_until};

/// The state of the car whose driver matches `is_driver`, if this app has it yet.
fn car(app: &mut TestApp, is_driver: impl Fn(PeerId) -> bool) -> Option<CarState> {
    app.0
        .world_mut()
        .query::<(&Car, &CarState)>()
        .iter(app.0.world())
        .find(|(car, _)| is_driver(car.owner))
        .map(|(_, state)| *state)
}

/// The progress of the car whose driver matches `is_driver`, if this app has it yet.
fn progress(app: &mut TestApp, is_driver: impl Fn(PeerId) -> bool) -> Option<RaceProgress> {
    app.0
        .world_mut()
        .query::<(&Car, &RaceProgress)>()
        .iter(app.0.world())
        .find(|(car, _)| is_driver(car.owner))
        .map(|(_, progress)| *progress)
}

/// Host only: park the car whose driver matches `is_driver` at `position`.
fn teleport(app: &mut TestApp, is_driver: impl Fn(PeerId) -> bool, position: Vec2) {
    let world = app.0.world_mut();
    let mut cars = world.query::<(&Car, &mut CarState)>();
    for (car, mut state) in cars.iter_mut(world) {
        if is_driver(car.owner) {
            *state = CarState::parked(position, state.heading);
        }
    }
}

/// How many cars this app can see.
fn cars(app: &mut TestApp) -> usize {
    app.0
        .world_mut()
        .query_filtered::<(), With<Car>>()
        .iter(app.0.world())
        .count()
}

/// The host's own player has a local peer id; the joiner's comes from netcode.
fn hosts(owner: PeerId) -> bool {
    matches!(owner, PeerId::Local(_))
}

fn joiners(owner: PeerId) -> bool {
    matches!(owner, PeerId::Netcode(_))
}

fn hold(app: &mut TestApp, key: KeyCode) {
    app.0
        .world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}

/// Connect a host and a joiner, start a race, and wait until both see both cars.
fn race_pair() -> (TestApp, TestApp) {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);
    host.start_minigame(Minigame::Racing);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            app.phase() == Some(SessionPhase::Racing)
                && car(app, hosts).is_some()
                && car(app, joiners).is_some()
        })
    });
    (host, client)
}

#[test]
fn every_player_gets_one_car_on_every_peer() {
    let (mut host, mut client) = race_pair();
    assert_eq!(cars(&mut host), 2);
    assert_eq!(cars(&mut client), 2);
}

#[test]
fn cars_start_on_the_road() {
    let (mut host, mut client) = race_pair();
    for app in [&mut host, &mut client] {
        for driver in [hosts, joiners] {
            let position = car(app, driver).unwrap().position;
            assert_eq!(surface_at(position), Surface::Road, "{position}");
        }
    }
}

#[test]
fn joiner_drives_their_car_on_every_peer() {
    let (mut host, mut client) = race_pair();
    let start = car(&mut host, joiners).unwrap().position;

    hold(&mut client, KeyCode::KeyW);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| car(app, joiners).is_some_and(|car| car.position.x > start.x + 20.0))
    });
    // Nobody touched the host's car.
    assert_eq!(car(&mut host, hosts).unwrap().speed, 0.0);
}

#[test]
fn host_steers_their_car_on_every_peer() {
    let (mut host, mut client) = race_pair();

    hold(&mut host, KeyCode::KeyW);
    hold(&mut host, KeyCode::KeyA);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| car(app, hosts).is_some_and(|car| car.heading > 0.5))
    });
}

#[test]
fn cars_are_removed_when_the_race_ends() {
    let (mut host, mut client) = race_pair();

    host.leave();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| cars(app) == 0)
    });
}

#[test]
fn every_car_starts_with_no_laps_on_every_peer() {
    let (mut host, mut client) = race_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            progress(app, hosts) == Some(RaceProgress::on_grid())
                && progress(app, joiners) == Some(RaceProgress::on_grid())
        })
    });
}

#[test]
fn the_host_counts_a_lap_and_every_peer_sees_it() {
    let (mut host, mut client) = race_pair();

    // Take the host's car round the checkpoints in order, ending on the line.
    for checkpoint in [1, 2, 3, 0] {
        teleport(&mut host, hosts, CHECKPOINTS[checkpoint].center());
        update_until(&mut [&mut host, &mut client], |apps| {
            progress(apps[0], hosts).is_some_and(|progress| {
                progress.next_checkpoint == (checkpoint + 1) % CHECKPOINTS.len()
            })
        });
    }

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| progress(app, hosts).is_some_and(|progress| progress.laps == 1))
    });
    // The joiner's car stayed on the grid.
    assert_eq!(progress(&mut host, joiners), Some(RaceProgress::on_grid()));
}
