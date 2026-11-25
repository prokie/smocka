//! Racing: every player gets a car, and each player's driving shows up on every peer.

use bevy::prelude::*;
use lightyear::prelude::PeerId;
use smocka::minigames::Minigame;
use smocka::minigames::racing::car::{Car, CarState};
use smocka::minigames::racing::track::{Surface, surface_at};
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
