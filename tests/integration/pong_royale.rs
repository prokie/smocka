//! Pong Royale: a paddle on every player's side, lives lost to the ball, walls where players went
//! out or left, and the last player with lives winning.

use bevy::prelude::*;
use lightyear::prelude::PeerId;
use smocka::minigames::Minigame;
use smocka::minigames::outcome::MinigameResult;
use smocka::minigames::pong_royale::LIVES_SETTING;
use smocka::minigames::pong_royale::arena::Side;
use smocka::minigames::pong_royale::config::ARENA_HALF;
use smocka::minigames::pong_royale::paddles::{Lives, Paddle, PaddlePosition};
use smocka::minigames::pong_royale::play::{BallPosition, BallVelocity, Wall};
use smocka::session::SessionPhase;

use crate::common::{TestApp, connected_pair, update_until};

/// The host's own player has a local peer id; joiners' come from netcode.
fn hosts(owner: PeerId) -> bool {
    matches!(owner, PeerId::Local(_))
}

fn joiners(owner: PeerId) -> bool {
    matches!(owner, PeerId::Netcode(_))
}

/// Each paddle this app sees: its player, side, position along it, and lives.
fn paddles(app: &mut TestApp) -> Vec<(PeerId, Side, f32, u32)> {
    app.0
        .world_mut()
        .query::<(&Paddle, &PaddlePosition, &Lives)>()
        .iter(app.0.world())
        .map(|(paddle, position, lives)| (paddle.owner, paddle.side, position.0, lives.0))
        .collect()
}

/// The paddle of the player matching `is_owner`, if this app sees it.
fn paddle_of(app: &mut TestApp, is_owner: impl Fn(PeerId) -> bool) -> Option<(Side, f32, u32)> {
    paddles(app)
        .into_iter()
        .find(|(owner, ..)| is_owner(*owner))
        .map(|(_, side, along, lives)| (side, along, lives))
}

/// The sides this app sees walled off, in [`Side::ALL`]'s order.
fn walls(app: &mut TestApp) -> Vec<Side> {
    let walled: Vec<Side> = app
        .0
        .world_mut()
        .query::<&Wall>()
        .iter(app.0.world())
        .map(|wall| wall.0)
        .collect();
    Side::ALL
        .into_iter()
        .filter(|side| walled.contains(side))
        .collect()
}

fn hold(app: &mut TestApp, key: KeyCode) {
    app.0
        .world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}

/// Host only: send the ball straight out through `side`'s goal, wide of its paddle.
fn shoot_into_goal(host: &mut TestApp, side: Side) {
    let world = host.0.world_mut();
    let (mut position, mut velocity) = world
        .query::<(&mut BallPosition, &mut BallVelocity)>()
        .single_mut(world)
        .unwrap();
    let wide_of_the_paddle = side.tangent() * 100.0;
    position.0 = side.normal() * (ARENA_HALF - 2.0) + wide_of_the_paddle;
    velocity.0 = side.normal() * 300.0;
}

/// The result this app is showing, if any.
fn result(app: &mut TestApp) -> Option<MinigameResult> {
    app.0
        .world_mut()
        .query::<&MinigameResult>()
        .iter(app.0.world())
        .next()
        .cloned()
}

/// A host and a joiner playing Pong Royale with `lives` lives each, both seeing both paddles.
fn royale_pair(lives: u32) -> (TestApp, TestApp) {
    let (mut host, mut client, _) = connected_pair();
    host.set_setting(LIVES_SETTING, lives);
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);
    host.start_minigame(Minigame::PongRoyale);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| paddles(app).len() == 2)
    });
    (host, client)
}

#[test]
fn two_players_face_each_other_and_the_other_sides_are_walled() {
    let (mut host, mut client) = royale_pair(3);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| walls(app).len() == 2)
    });
    for app in [&mut host, &mut client] {
        assert_eq!(paddle_of(app, hosts), Some((Side::Left, 0.0, 3)));
        assert_eq!(paddle_of(app, joiners), Some((Side::Right, 0.0, 3)));
        assert_eq!(walls(app), [Side::Bottom, Side::Top]);
    }
}

#[test]
fn the_joiner_moves_their_paddle_along_their_side_on_every_peer() {
    let (mut host, mut client) = royale_pair(3);
    // The joiner is on the right: up moves their paddle along it.
    hold(&mut client, KeyCode::KeyW);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_of(app, joiners).is_some_and(|(_, along, _)| along > 10.0))
    });
    assert_eq!(
        paddle_of(&mut host, hosts).map(|(_, along, _)| along),
        Some(0.0)
    );
}

#[test]
fn letting_the_ball_in_costs_a_life_on_every_peer() {
    let (mut host, mut client) = royale_pair(3);
    shoot_into_goal(&mut host, Side::Right);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_of(app, joiners).is_some_and(|(_, _, lives)| lives == 2))
    });
    assert_eq!(
        paddle_of(&mut host, hosts).map(|(.., lives)| lives),
        Some(3)
    );
    assert_eq!(
        host.phase(),
        Some(SessionPhase::Playing(Minigame::PongRoyale))
    );
}

#[test]
fn the_last_player_with_lives_left_wins() {
    let (mut host, mut client) = royale_pair(1);
    shoot_into_goal(&mut host, Side::Right);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.phase() == Some(SessionPhase::Results) && result(app).is_some())
    });
    for app in [&mut host, &mut client] {
        let result = result(app).unwrap();
        let places: Vec<(bool, u32)> = result
            .placements
            .iter()
            .map(|placement| (hosts(placement.player), placement.place))
            .collect();
        assert_eq!(places, [(true, 1), (false, 2)]);
    }
}

#[test]
fn a_player_going_out_is_walled_off_while_the_rest_play_on() {
    let (mut host, mut first, addr) = connected_pair();
    let mut second = TestApp::join(addr);
    host.set_setting(LIVES_SETTING, 1);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| app.players() == 3)
    });
    host.start_minigame(Minigame::PongRoyale);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| paddles(app).len() == 3)
    });

    // Three players: left, right and bottom. The top is walled off from the start.
    shoot_into_goal(&mut host, Side::Bottom);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut()
            .all(|app| paddles(app).len() == 2 && walls(app) == [Side::Bottom, Side::Top])
    });
    assert_eq!(
        host.phase(),
        Some(SessionPhase::Playing(Minigame::PongRoyale))
    );
}

#[test]
fn a_player_who_leaves_is_walled_off_while_the_rest_play_on() {
    let (mut host, mut first, addr) = connected_pair();
    let mut second = TestApp::join(addr);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| app.players() == 3)
    });
    host.start_minigame(Minigame::PongRoyale);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| paddles(app).len() == 3)
    });

    second.leave();
    update_until(&mut [&mut host, &mut first], |apps| {
        apps.iter_mut()
            .all(|app| paddles(app).len() == 2 && walls(app).len() == 2)
    });
    for app in [&mut host, &mut first] {
        assert_eq!(
            app.phase(),
            Some(SessionPhase::Playing(Minigame::PongRoyale))
        );
    }
}

#[test]
fn the_game_is_abandoned_when_everyone_else_leaves() {
    let (mut host, mut client) = royale_pair(3);
    client.leave();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].phase() == Some(SessionPhase::Lobby) && paddles(apps[0]).is_empty()
    });
}

#[test]
fn players_pick_paddle_shapes_on_the_setup_screen_for_pong_royale() {
    use smocka::minigames::pong_paddle::{PaddleShape, PickPaddleShape};
    use smocka::minigames::pong_royale::paddles::Paddle;
    use smocka::session::OpenMinigameSetup;

    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| app.players() == 2)
    });

    OpenMinigameSetup(Minigame::PongRoyale).apply(host.0.world_mut());
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.phase() == Some(SessionPhase::Setup(Minigame::PongRoyale)))
    });

    PickPaddleShape(PaddleShape::Bumper).apply(host.0.world_mut());
    PickPaddleShape(PaddleShape::Wedge).apply(client.0.world_mut());
    // Wait a moment for the joiner's request to reach the host before starting.
    update_until(&mut [&mut host, &mut client], |apps| {
        let world = apps[0].0.world_mut();
        world
            .query::<(
                &smocka::players::Player,
                &smocka::minigames::pong_paddle::PlayerPaddleShape,
            )>()
            .iter(world)
            .any(|(player, shape)| joiners(player.id) && shape.0 == PaddleShape::Wedge)
    });

    host.start_minigame(Minigame::PongRoyale);
    let paddle_shape = |app: &mut TestApp, is_owner: fn(PeerId) -> bool| -> Option<PaddleShape> {
        let world = app.0.world_mut();
        world
            .query::<&Paddle>()
            .iter(world)
            .find(|paddle| is_owner(paddle.owner))
            .map(|paddle| paddle.shape)
    };
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            app.phase() == Some(SessionPhase::Playing(Minigame::PongRoyale))
                && paddle_shape(app, hosts) == Some(PaddleShape::Bumper)
                && paddle_shape(app, joiners) == Some(PaddleShape::Wedge)
        })
    });
}
