//! The host decides the session phase and every peer follows it: the lobby until the host starts,
//! the minigame, its results, the standings, and back to the lobby.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::PeerId;
use smocka::config::session::{RESULTS_DURATION, STANDINGS_DURATION};
use smocka::minigames::Minigame;
use smocka::minigames::two_player_pong::ball::Ball;
use smocka::minigames::two_player_pong::paddles::Paddle;
use smocka::minigames::two_player_pong::score::PongRules;
use smocka::players::Player;
use smocka::session::{MinigameResult, SessionPhase};
use smocka::standings::Points;

use crate::common::{
    TestApp, connected_pair, free_addr, pair_in_pong, update_until, update_until_within,
};

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

/// Every player's points as this app sees them, host first: `(total, gained)`.
fn points(app: &mut TestApp) -> Vec<(u32, u32)> {
    let world = app.0.world_mut();
    let mut players: Vec<(u32, Points)> = world
        .query::<(&Player, &Points)>()
        .iter(world)
        .map(|(player, points)| (player.number, *points))
        .collect();
    players.sort_by_key(|(number, _)| *number);
    players
        .into_iter()
        .map(|(_, points)| (points.total, points.gained))
        .collect()
}

/// The result this app is showing, if any.
fn result(app: &mut TestApp) -> Option<MinigameResult> {
    let world = app.0.world_mut();
    world.query::<&MinigameResult>().iter(world).next().cloned()
}

fn wait_a_bit(apps: &mut [&mut TestApp]) {
    for _ in 0..20 {
        for app in apps.iter_mut() {
            app.0.update();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_lone_host_cannot_start() {
    let mut host = TestApp::host(free_addr());
    update_until(&mut [&mut host], |apps| apps[0].players() == 1);

    host.start_minigame(Minigame::TwoPlayerPong);
    wait_a_bit(&mut [&mut host]);

    assert_eq!(host.phase(), Some(SessionPhase::Lobby));
    assert_eq!(pong_entities(&mut host), 0);
}

#[test]
fn players_wait_in_the_lobby_until_the_host_starts() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| app.players() == 2)
    });
    wait_a_bit(&mut [&mut host, &mut client]);
    assert_eq!(host.phase(), Some(SessionPhase::Lobby));
    assert_eq!(client.phase(), Some(SessionPhase::Lobby));

    host.start_minigame(Minigame::TwoPlayerPong);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter()
            .all(|app| app.phase() == Some(SessionPhase::TwoPlayerPong))
    });
}

#[test]
fn the_host_picks_which_minigame_to_start() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| app.players() == 2)
    });

    host.start_minigame(Minigame::Racing);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter()
            .all(|app| app.phase() == Some(SessionPhase::Racing))
    });
    assert_eq!(pong_entities(&mut host), 0);
}

#[test]
fn a_joiner_cannot_start() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.players() == 2 && app.phase() == Some(SessionPhase::Lobby))
    });

    client.start_minigame(Minigame::TwoPlayerPong);
    wait_a_bit(&mut [&mut host, &mut client]);

    assert_eq!(host.phase(), Some(SessionPhase::Lobby));
    assert_eq!(client.phase(), Some(SessionPhase::Lobby));
}

#[test]
fn winning_shows_the_result_then_the_standings_then_returns_to_the_lobby() {
    let (mut host, mut client, _) = connected_pair();
    host.0
        .world_mut()
        .insert_resource(PongRules { goals_to_win: 1 });
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);
    host.start_minigame(Minigame::TwoPlayerPong);

    // Nobody moves, so the first serve flies past whichever paddle it goes to: a 1–0 win for the
    // host (left) or the joiner (right), at random.
    update_until_within(
        &mut [&mut host, &mut client],
        Duration::from_secs(10),
        |apps| {
            apps.iter_mut().all(|app| {
                app.phase() == Some(SessionPhase::Results)
                    && result(app).is_some()
                    && pong_entities(app) == 0
            })
        },
    );
    let host_won = matches!(
        result(&mut host).unwrap().placements[0].player,
        PeerId::Local(_)
    );
    for app in [&mut host, &mut client] {
        let result = result(app).unwrap();
        let [winner, loser] = &result.placements[..] else {
            panic!("both players are placed: {result:?}");
        };
        assert_eq!(
            matches!(winner.player, PeerId::Local(_)),
            host_won,
            "every peer agrees who won: {result:?}"
        );
        assert_eq!((winner.place, loser.place), (1, 2));
        assert_eq!(result.summary, if host_won { "1 - 0" } else { "0 - 1" });
    }

    // Beating one player is worth one point.
    update_until_within(
        &mut [&mut host, &mut client],
        RESULTS_DURATION + Duration::from_secs(5),
        |apps| {
            apps.iter_mut()
                .all(|app| app.phase() == Some(SessionPhase::Standings))
        },
    );
    let expected = if host_won {
        [(1, 1), (0, 0)]
    } else {
        [(0, 0), (1, 1)]
    };
    for app in [&mut host, &mut client] {
        assert_eq!(points(app), expected);
    }

    update_until_within(
        &mut [&mut host, &mut client],
        STANDINGS_DURATION + Duration::from_secs(5),
        |apps| {
            apps.iter_mut()
                .all(|app| app.phase() == Some(SessionPhase::Lobby) && result(app).is_none())
        },
    );
}

#[test]
fn a_player_leaving_mid_game_returns_everyone_to_the_lobby() {
    let (mut host, mut client, _) = pair_in_pong();
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
