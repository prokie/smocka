//! Two-player Pong: paddles and the ball move on every peer, and goals are scored on every peer.

use bevy::prelude::*;
use lightyear::prelude::PeerId;
use smocka::minigames::Minigame;
use smocka::minigames::settings::PublishedSettings;
use smocka::minigames::two_player_pong::ball::{Ball, BallPosition};
use smocka::minigames::two_player_pong::paddles::{Paddle, PaddlePosition};
use smocka::minigames::two_player_pong::score::PongScore;
use smocka::minigames::two_player_pong::{GOALS_TO_WIN_SETTING, PongRules};

use std::time::Duration;

use bevy::prelude::Command;
use smocka::network::JoinSession;
use smocka::session::SessionPhase;

use crate::common::{
    TestApp, connected_pair, pair_in_pong, update_for, update_until, update_until_within,
};

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

/// Where this app's ball is, if it has one yet.
fn ball_position(app: &mut TestApp) -> Option<Vec2> {
    app.0
        .world_mut()
        .query_filtered::<&BallPosition, With<Ball>>()
        .iter(app.0.world())
        .next()
        .map(|position| position.0)
}

/// This app's score, if it has one yet.
fn score(app: &mut TestApp) -> Option<PongScore> {
    app.0
        .world_mut()
        .query::<&PongScore>()
        .iter(app.0.world())
        .next()
        .copied()
}

fn hold(app: &mut TestApp, key: KeyCode) {
    app.0
        .world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}

/// Connect a host and a joiner, start Pong, and wait until both see both paddles. Also returns
/// the host's address.
fn pong_pair() -> (TestApp, TestApp, std::net::SocketAddr) {
    let (mut host, mut client, addr) = pair_in_pong();
    wait_for_paddles(&mut host, &mut client);
    (host, client, addr)
}

/// Wait until both apps see both paddles.
fn wait_for_paddles(host: &mut TestApp, client: &mut TestApp) {
    update_until(&mut [host, client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_y(app, hosts).is_some() && paddle_y(app, joiners).is_some())
    });
}

#[test]
fn neither_paddles_nor_the_ball_move_until_the_countdown_is_over() {
    let (mut host, mut client, _) = connected_pair();
    host.0.world_mut().insert_resource(PongRules {
        countdown: Duration::from_secs(2),
    });
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);
    host.start_minigame(Minigame::TwoPlayerPong);
    wait_for_paddles(&mut host, &mut client);

    // The joiner holds up, but nothing moves until the start. That's long enough for their input
    // to reach the host; their own, predicted paddle would move at once.
    hold(&mut client, KeyCode::KeyW);
    update_for(&mut [&mut host, &mut client], Duration::from_millis(800));
    for app in [&mut host, &mut client] {
        assert_eq!(paddle_y(app, joiners), Some(0.0));
        assert_eq!(ball_position(app), Some(Vec2::ZERO));
    }

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            paddle_y(app, joiners).is_some_and(|y| y > 0.0)
                && ball_position(app).is_some_and(|ball| ball != Vec2::ZERO)
        })
    });
}

#[test]
fn joiner_moves_their_paddle_on_every_peer() {
    let (mut host, mut client, _) = pong_pair();

    hold(&mut client, KeyCode::KeyW);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_y(app, joiners).is_some_and(|y| y > 10.0))
    });
    assert_eq!(paddle_y(&mut host, hosts), Some(0.0));
}

#[test]
fn joiner_moves_their_paddle_with_a_gamepad_stick_on_every_peer() {
    let (mut host, mut client, _) = pong_pair();
    // Pushed down: the default gamepad control for moving down.
    let mut gamepad = Gamepad::default();
    gamepad.analog_mut().set(GamepadAxis::LeftStickY, -0.9);
    client.0.world_mut().spawn(gamepad);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_y(app, joiners).is_some_and(|y| y < -10.0))
    });
}

#[test]
fn host_moves_their_paddle_on_every_peer() {
    let (mut host, mut client, _) = pong_pair();

    hold(&mut host, KeyCode::KeyS);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_y(app, hosts).is_some_and(|y| y < -10.0))
    });
    assert_eq!(paddle_y(&mut host, joiners), Some(0.0));
}

#[test]
fn the_ball_moves_on_every_peer() {
    let (mut host, mut client, _) = pong_pair();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| ball_position(app).is_some_and(|position| position != Vec2::ZERO))
    });

    // Only the host spawns the ball; the client just receives it.
    for app in [&mut host, &mut client] {
        let balls = app
            .0
            .world_mut()
            .query_filtered::<(), With<Ball>>()
            .iter(app.0.world())
            .count();
        assert_eq!(balls, 1);
    }
}

#[test]
fn a_missed_ball_scores_on_every_peer() {
    let (mut host, mut client, _) = pong_pair();

    // Nobody moves, so the first serve flies past whichever paddle it goes to (random).
    update_until_within(
        &mut [&mut host, &mut client],
        Duration::from_secs(10),
        |apps| {
            let first_goal = score(apps[0]).filter(|score| score.left + score.right == 1);
            first_goal.is_some() && score(apps[1]) == first_goal
        },
    );
}

#[test]
fn a_joiner_who_rejoins_can_still_move_their_paddle() {
    let (mut host, mut client, addr) = pong_pair();

    client.leave();
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 1);
    JoinSession { addr }.apply(client.0.world_mut()).unwrap();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| app.players() == 2)
    });
    host.start_minigame(Minigame::TwoPlayerPong);
    wait_for_paddles(&mut host, &mut client);

    hold(&mut client, KeyCode::KeyW);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| paddle_y(app, joiners).is_some_and(|y| y > 10.0))
    });
}

#[test]
fn the_host_picks_how_many_goals_win_and_joiners_see_it() {
    let (mut host, mut client, _) = connected_pair();
    host.set_setting(GOALS_TO_WIN_SETTING, 2);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].players() == 2 && goals_to_win(apps[1]) == Some(2)
    });
    host.start_minigame(Minigame::TwoPlayerPong);

    // Nobody moves, so every serve flies past the paddle it goes to. After a goal, the ball is
    // served to the side that let it in, so the same side scores again.
    update_until_within(
        &mut [&mut host, &mut client],
        Duration::from_secs(10),
        |apps| score(apps[1]).is_some_and(|score| score.left + score.right == 1),
    );
    assert_eq!(
        host.phase(),
        Some(SessionPhase::Playing(Minigame::TwoPlayerPong)),
        "one goal doesn't win when two are needed"
    );
    update_until_within(
        &mut [&mut host, &mut client],
        Duration::from_secs(10),
        |apps| {
            apps.iter_mut()
                .all(|app| app.phase() == Some(SessionPhase::Results))
        },
    );
}

/// How many goals this app has been told win Pong, if it knows yet.
fn goals_to_win(app: &mut TestApp) -> Option<u32> {
    app.0
        .world_mut()
        .query::<&PublishedSettings>()
        .iter(app.0.world())
        .next()
        .map(|settings| settings.get(GOALS_TO_WIN_SETTING))
}

#[test]
fn players_pick_paddle_shapes_on_the_setup_screen_and_play_with_them_in_pong() {
    use smocka::minigames::pong_paddle::{PaddleShape, PickPaddleShape, PlayerPaddleShape};
    use smocka::players::Player;
    use smocka::session::OpenMinigameSetup;

    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| app.players() == 2)
    });

    // Host opens the pre-game setup screen; both peers enter Setup(TwoPlayerPong).
    OpenMinigameSetup(Minigame::TwoPlayerPong).apply(host.0.world_mut());
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.phase() == Some(SessionPhase::Setup(Minigame::TwoPlayerPong)))
    });

    // Host picks Wedge and joiner picks Split; both peers see both picks replicate.
    PickPaddleShape(PaddleShape::Wedge).apply(host.0.world_mut());
    PickPaddleShape(PaddleShape::Split).apply(client.0.world_mut());
    let picked = |app: &mut TestApp, is_owner: fn(PeerId) -> bool| -> Option<PaddleShape> {
        let world = app.0.world_mut();
        world
            .query::<(&Player, &PlayerPaddleShape)>()
            .iter(world)
            .find(|(player, _)| is_owner(player.id))
            .map(|(_, shape)| shape.0)
    };
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            picked(app, hosts) == Some(PaddleShape::Wedge)
                && picked(app, joiners) == Some(PaddleShape::Split)
        })
    });

    // Host starts Pong from the setup screen; spawned paddles keep each player's chosen shape.
    host.start_minigame(Minigame::TwoPlayerPong);
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
            app.phase() == Some(SessionPhase::Playing(Minigame::TwoPlayerPong))
                && paddle_shape(app, hosts) == Some(PaddleShape::Wedge)
                && paddle_shape(app, joiners) == Some(PaddleShape::Split)
        })
    });
}
