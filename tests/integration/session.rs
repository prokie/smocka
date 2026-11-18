//! The host decides the session phase, every peer follows it, and Pong's entities are removed
//! when the session leaves Pong.

use bevy::prelude::*;
use smocka::minigames::two_player_pong::ball::Ball;
use smocka::minigames::two_player_pong::paddles::Paddle;
use smocka::session::SessionPhase;

use crate::common::{TestApp, connected_pair, free_addr, update_until};

/// Number of paddles and balls this app can see.
fn pong_entities(app: &mut TestApp) -> usize {
    let world = app.0.world_mut();
    let paddles = world
        .query_filtered::<(), With<Paddle>>()
        .iter(world)
        .count();
    let balls = world.query_filtered::<(), With<Ball>>().iter(world).count();
    paddles + balls
}

#[test]
fn a_lone_host_waits_in_the_lobby() {
    let mut host = TestApp::host(free_addr());
    update_until(&mut [&mut host], |apps| apps[0].players() == 1);

    for _ in 0..10 {
        host.0.update();
    }
    assert_eq!(host.phase(), Some(SessionPhase::Lobby));
    assert_eq!(pong_entities(&mut host), 0);
}

#[test]
fn both_peers_enter_pong_when_a_second_player_joins() {
    let (mut host, mut client, _) = connected_pair();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter()
            .all(|app| app.phase() == Some(SessionPhase::TwoPlayerPong))
    });
}

#[test]
fn leaving_ends_pong_and_removes_its_entities_on_both_peers() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| pong_entities(app) == 3)
    });

    client.disconnect();

    // The host goes back to the lobby; the joiner leaves the session altogether.
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].phase() == Some(SessionPhase::Lobby)
            && apps[1].phase().is_none()
            && apps.iter_mut().all(|app| pong_entities(app) == 0)
    });
}
