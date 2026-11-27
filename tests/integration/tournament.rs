//! Knockout tournaments: Pong with more than two players plays match after match until one
//! champion is left, and the bracket's placements are the minigame's result.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use smocka::minigames::Minigame;
use smocka::minigames::outcome::{MinigameResult, Participants};
use smocka::minigames::two_player_pong::paddles::Paddle;
use smocka::minigames::two_player_pong::score::PongRules;
use smocka::players::Player;
use smocka::session::SessionPhase;
use smocka::standings::Points;
use smocka::tournament::{Tournament, TournamentRules};

use crate::common::{TestApp, connected_pair, update_until, update_until_within};

/// A host and two joiners in the lobby, with no wait before each match.
fn trio() -> (TestApp, TestApp, TestApp) {
    let (mut host, mut first, addr) = connected_pair();
    let mut second = TestApp::join(addr);
    host.0.world_mut().insert_resource(TournamentRules {
        next_match: Duration::ZERO,
    });
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| app.players() == 3)
    });
    (host, first, second)
}

fn has_tournament(app: &mut TestApp) -> bool {
    let world = app.0.world_mut();
    world.query::<&Tournament>().iter(world).next().is_some()
}

fn paddles(app: &mut TestApp) -> usize {
    let world = app.0.world_mut();
    world
        .query_filtered::<(), With<Paddle>>()
        .iter(world)
        .count()
}

fn result(app: &mut TestApp) -> Option<MinigameResult> {
    let world = app.0.world_mut();
    world.query::<&MinigameResult>().iter(world).next().cloned()
}

/// Everyone's points total as this app sees them, lowest first.
fn totals(app: &mut TestApp) -> Vec<u32> {
    let world = app.0.world_mut();
    let mut totals: Vec<u32> = world
        .query::<&Points>()
        .iter(world)
        .map(|points| points.total)
        .collect();
    totals.sort();
    totals
}

/// The player numbers in the current match, as the host sees them.
fn playing(host: &mut TestApp) -> Vec<u32> {
    let world = host.0.world_mut();
    let participants = world
        .query::<&Participants>()
        .single(world)
        .unwrap()
        .clone();
    let players: Vec<Player> = world.query::<&Player>().iter(world).cloned().collect();
    participants
        .0
        .iter()
        .filter_map(|id| players.iter().find(|player| player.id == *id))
        .map(|player| player.number)
        .collect()
}

#[test]
fn three_players_play_a_tournament_to_one_result() {
    let (mut host, mut first, mut second) = trio();
    host.0
        .world_mut()
        .insert_resource(PongRules { goals_to_win: 1 });
    host.start_minigame(Minigame::TwoPlayerPong);

    // Nobody moves, so each match ends 1-0 to whoever the serve goes away from: two matches,
    // then the result.
    update_until_within(
        &mut [&mut host, &mut first, &mut second],
        Duration::from_secs(30),
        |apps| {
            apps.iter_mut().all(|app| {
                app.phase() == Some(SessionPhase::Results)
                    && result(app).is_some_and(|result| result.placements.len() == 3)
            })
        },
    );
    for app in [&mut host, &mut first, &mut second] {
        let result = result(app).unwrap();
        let places: Vec<u32> = result.placements.iter().map(|p| p.place).collect();
        assert_eq!(places, [1, 2, 3], "{result:?}");
        assert!(result.summary.starts_with("Final: "), "{result:?}");
        // The champion beat two players, the finalist one.
        assert_eq!(totals(app), [0, 1, 2]);
    }
}

/// Start a tournament, wait until its first match is on, and pick the joiner to leave: one of
/// the two playing (at least one of them is a joiner). Returns `(leaving, staying)`.
fn leave_mid_match(host: &mut TestApp, first: TestApp, second: TestApp) -> (TestApp, TestApp) {
    let (mut first, mut second) = (first, second);
    host.start_minigame(Minigame::TwoPlayerPong);
    update_until(&mut [host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| {
            app.phase() == Some(SessionPhase::Playing(Minigame::TwoPlayerPong))
                && paddles(app) == 2
                && has_tournament(app)
        })
    });
    let playing = playing(host);
    if playing.contains(&first.own_player_number().unwrap()) {
        (first, second)
    } else {
        (second, first)
    }
}

#[test]
fn a_player_leaving_mid_match_loses_it_by_walkover() {
    let (mut host, first, second) = trio();
    let (mut leaving, mut staying) = leave_mid_match(&mut host, first, second);
    let left = leaving.own_player_number().unwrap();
    leaving.disconnect();
    // Once the host has seen them go (and so walked them over), make the final quick. Any
    // sooner, and the semi-final could end with them winning it.
    update_until(&mut [&mut host, &mut staying], |apps| {
        apps[0].players() == 2
    });
    host.0
        .world_mut()
        .insert_resource(PongRules { goals_to_win: 1 });

    // With three players the first match is a semi-final, so the one who left comes 3rd.
    update_until_within(
        &mut [&mut host, &mut staying],
        Duration::from_secs(30),
        |apps| {
            apps.iter_mut().all(|app| {
                app.phase() == Some(SessionPhase::Results)
                    && result(app).is_some_and(|result| result.placements.len() == 3)
            })
        },
    );
    for app in [&mut host, &mut staying] {
        let result = result(app).unwrap();
        let shown: Vec<(u32, &str)> = result
            .placements
            .iter()
            .map(|placement| (placement.place, placement.note.as_str()))
            .collect();
        assert_eq!(shown, [(1, ""), (2, ""), (3, "left")], "{result:?}");
        assert_eq!(result.placements[2].name, format!("Player {left}"));
    }
}

#[test]
fn a_walkover_on_the_next_match_screen_restarts_the_countdown() {
    let (mut host, mut first, mut second) = trio();
    let countdown = Duration::from_millis(1500);
    host.0.world_mut().insert_resource(TournamentRules {
        next_match: countdown,
    });
    host.start_minigame(Minigame::TwoPlayerPong);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| has_tournament(app))
    });
    let started = Instant::now();

    // Near the end of the countdown, a joiner in the first match leaves. With three players,
    // the one who stays meets the player who had a bye.
    let playing = playing(&mut host);
    let mut leaving = if playing.contains(&first.own_player_number().unwrap()) {
        first
    } else {
        second
    };
    update_until(&mut [&mut host, &mut leaving], |_| {
        started.elapsed() >= Duration::from_millis(1000)
    });
    leaving.disconnect();
    update_until(&mut [&mut host, &mut leaving], |apps| {
        apps[0].players() == 2
    });
    assert_eq!(host.phase(), Some(SessionPhase::NextMatch));

    // The old countdown would have run out half a second after the walkover at most. The new
    // pair gets the whole countdown.
    let walked_over = Instant::now();
    update_until(&mut [&mut host], |_| {
        walked_over.elapsed() >= Duration::from_millis(900)
    });
    assert_eq!(host.phase(), Some(SessionPhase::NextMatch));
    update_until(&mut [&mut host], |apps| {
        apps[0].phase() == Some(SessionPhase::Playing(Minigame::TwoPlayerPong))
    });
}

#[test]
fn a_tournament_with_fewer_than_two_players_left_is_abandoned() {
    let (mut host, first, second) = trio();
    let (mut leaving, mut also_leaving) = leave_mid_match(&mut host, first, second);
    leaving.disconnect();
    also_leaving.disconnect();

    update_until(&mut [&mut host], |apps| {
        apps[0].phase() == Some(SessionPhase::Lobby)
            && paddles(apps[0]) == 0
            && !has_tournament(apps[0])
    });
    assert!(result(&mut host).is_none());
}
