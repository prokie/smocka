//! Two-player Pong: holding a key moves your paddle on every peer.

use bevy::prelude::*;
use lightyear::prelude::PeerId;
use smocka::minigames::two_player_pong::paddles::{Paddle, PaddlePosition};

use crate::common::{TestApp, connected_pair, update_until};

/// `y` of the paddle whose owner matches `is_owner`, if this app has it yet.
fn paddle_y(app: &mut TestApp, is_owner: impl Fn(PeerId) -> bool) -> Option<f32> {
    app.0
        .world_mut()
        .query::<(&Paddle, &PaddlePosition)>()
        .iter(app.0.world())
        .find(|(paddle, _)| is_owner(paddle.owner))
        .map(|(_, position)| position.0.y)
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

/// Connect a host and a joiner and wait until both see both paddles.
fn pong_pair() -> (TestApp, TestApp) {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_y(app, hosts).is_some() && paddle_y(app, joiners).is_some())
    });
    (host, client)
}

#[test]
fn joiner_moves_their_paddle_on_every_peer() {
    let (mut host, mut client) = pong_pair();

    hold(&mut client, KeyCode::KeyW);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_y(app, joiners).is_some_and(|y| y > 10.0))
    });
    assert_eq!(paddle_y(&mut host, hosts), Some(0.0));
}

#[test]
fn host_moves_their_paddle_on_every_peer() {
    let (mut host, mut client) = pong_pair();

    hold(&mut host, KeyCode::KeyS);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_y(app, hosts).is_some_and(|y| y < -10.0))
    });
    assert_eq!(paddle_y(&mut host, joiners), Some(0.0));
}
