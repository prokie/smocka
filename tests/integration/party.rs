//! Parties: the host starts one from the lobby, the game picks each round's minigame, points add
//! up over the rounds, and the winner is shown before everyone goes back to the lobby.

use std::time::Duration;

use bevy::prelude::*;
use smocka::config::party::ROUNDS;
use smocka::minigames::Minigame;
use smocka::minigames::outcome::{MinigameEnded, MinigameResult, Participants, Placement};
use smocka::party::{ChangeRounds, Party, PartyRounds, PartyRules, StartParty};
use smocka::players::Player;
use smocka::session::{SessionPhase, SessionRules};
use smocka::standings::Points;
use smocka::tournament::{Tournament, TournamentRules};

use crate::common::{TestApp, connected_pair, update_until};

/// Make every screen between minigames instant, except the next round's and the winner's, which
/// stay up for `next_round` and `winner`. Parties have `rounds` rounds of `games`.
fn quick_party(
    host: &mut TestApp,
    rounds: u32,
    games: &[Minigame],
    next_round: Duration,
    winner: Duration,
) {
    let world = host.0.world_mut();
    world.insert_resource(SessionRules {
        results: Duration::ZERO,
        standings: Duration::ZERO,
    });
    world.insert_resource(PartyRules {
        rounds,
        next_round,
        winner,
        games: games.to_vec(),
    });
    world.insert_resource(TournamentRules {
        next_match: Duration::ZERO,
    });
}

/// A host and a joiner in the lobby, both seeing both players.
fn pair() -> (TestApp, TestApp) {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| app.players() == 2)
    });
    (host, client)
}

fn party(app: &mut TestApp) -> Option<Party> {
    let world = app.0.world_mut();
    world.query::<&Party>().iter(world).next().cloned()
}

fn tournaments(app: &mut TestApp) -> usize {
    let world = app.0.world_mut();
    world.query::<&Tournament>().iter(world).count()
}

/// Whether the host is playing a minigame (a match, in a tournament) in round `round`.
fn playing_round(app: &mut TestApp, round: u32) -> bool {
    app.phase().is_some_and(SessionPhase::is_minigame)
        && party(app).is_some_and(|party| party.round() == round)
}

/// Host only: end the current minigame (or match) with its participants placed in seat order.
fn win(host: &mut TestApp) {
    let world = host.0.world_mut();
    let seats = world
        .query::<&Participants>()
        .single(world)
        .unwrap()
        .0
        .clone();
    let players: Vec<Player> = world.query::<&Player>().iter(world).cloned().collect();
    let placements = seats
        .iter()
        .zip(1..)
        .map(|(id, place)| {
            let player = players.iter().find(|player| player.id == *id).unwrap();
            Placement::new(player, place)
        })
        .collect();
    world.trigger(MinigameEnded::Won(MinigameResult {
        placements,
        summary: String::new(),
    }));
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

#[test]
fn a_party_plays_its_rounds_then_shows_the_winner() {
    let (mut host, mut client) = pair();
    quick_party(
        &mut host,
        2,
        &Minigame::ALL,
        Duration::ZERO,
        Duration::from_millis(300),
    );
    // Points from before the party don't count.
    for mut points in host
        .0
        .world_mut()
        .query::<&mut Points>()
        .iter_mut(host.0.world_mut())
    {
        points.total = 7;
    }

    StartParty.apply(host.0.world_mut());
    for round in 1..=2 {
        update_until(&mut [&mut host, &mut client], |apps| {
            playing_round(apps[0], round)
        });
        win(&mut host);
    }

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.phase() == Some(SessionPhase::PartyOver))
    });
    assert_eq!(party(&mut client).map(|party| party.played), Some(2));
    // The winner of each two-player round beats one player.
    assert_eq!(totals(&mut client).iter().sum::<u32>(), 2);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.phase() == Some(SessionPhase::Lobby) && party(app).is_none())
    });
}

#[test]
fn an_abandoned_minigame_is_played_again() {
    let (mut host, mut client) = pair();
    quick_party(
        &mut host,
        1,
        &Minigame::ALL,
        Duration::from_millis(200),
        Duration::ZERO,
    );

    StartParty.apply(host.0.world_mut());
    update_until(&mut [&mut host, &mut client], |apps| {
        playing_round(apps[0], 1)
    });
    host.0.world_mut().trigger(MinigameEnded::Abandoned);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].phase() == Some(SessionPhase::NextRound)
    });
    update_until(&mut [&mut host, &mut client], |apps| {
        playing_round(apps[0], 1)
    });
    win(&mut host);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].phase() == Some(SessionPhase::PartyOver)
            || apps[0].phase() == Some(SessionPhase::Lobby)
    });
    assert_eq!(totals(&mut host), [0, 1]);
}

#[test]
fn a_party_ends_when_fewer_than_two_players_are_left() {
    let (mut host, mut client) = pair();
    quick_party(&mut host, 3, &Minigame::ALL, Duration::ZERO, Duration::ZERO);

    StartParty.apply(host.0.world_mut());
    update_until(&mut [&mut host, &mut client], |apps| {
        playing_round(apps[0], 1) && apps[1].phase().is_some_and(SessionPhase::is_minigame)
    });
    client.leave();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].phase() == Some(SessionPhase::Lobby) && party(apps[0]).is_none()
    });
}

#[test]
fn each_round_of_tournaments_draws_a_new_bracket() {
    let (mut host, mut first, addr) = connected_pair();
    let mut second = TestApp::join(addr);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| app.players() == 3)
    });
    quick_party(
        &mut host,
        2,
        &[Minigame::TwoPlayerPong],
        Duration::ZERO,
        Duration::from_millis(300),
    );

    StartParty.apply(host.0.world_mut());
    // Three players: a semi-final and a final each round.
    for round in 1..=2 {
        for _ in 0..2 {
            update_until(&mut [&mut host, &mut first, &mut second], |apps| {
                playing_round(apps[0], round)
            });
            assert_eq!(tournaments(&mut host), 1, "round {round}");
            win(&mut host);
            update_until(&mut [&mut host, &mut first, &mut second], |apps| {
                !apps[0].phase().is_some_and(SessionPhase::is_minigame)
            });
        }
    }

    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps[0].phase() == Some(SessionPhase::PartyOver)
    });
    // Each tournament's champion beat two players, its finalist one.
    assert_eq!(totals(&mut host).iter().sum::<u32>(), 6);
}

#[test]
fn a_round_after_a_tournament_is_played_without_it() {
    let (mut host, mut first, addr) = connected_pair();
    let mut second = TestApp::join(addr);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| app.players() == 3)
    });
    quick_party(
        &mut host,
        2,
        &[Minigame::TwoPlayerPong],
        Duration::ZERO,
        Duration::from_millis(300),
    );

    StartParty.apply(host.0.world_mut());
    // Round 1, three players: a Pong semi-final and final.
    for _ in 0..2 {
        update_until(&mut [&mut host, &mut first, &mut second], |apps| {
            playing_round(apps[0], 1)
        });
        win(&mut host);
    }
    // Round 2 picks Racing, which seats everyone.
    host.0.world_mut().resource_mut::<PartyRules>().games = vec![Minigame::Racing];

    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        playing_round(apps[0], 2) && apps.iter_mut().all(|app| tournaments(app) == 0)
    });
    assert_eq!(host.phase(), Some(SessionPhase::Playing(Minigame::Racing)));
    // The race's result is the party's, not the gone tournament's.
    win(&mut host);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps[0].phase() == Some(SessionPhase::PartyOver)
    });
    // The tournament gave 2 + 1 points, the three-car race 2 + 1 + 0.
    assert_eq!(totals(&mut host).iter().sum::<u32>(), 6);
}

#[test]
fn only_the_host_starts_a_party_and_joiners_see_its_rounds() {
    let (mut host, mut client) = pair();

    StartParty.apply(client.0.world_mut());
    ChangeRounds(2).apply(host.0.world_mut());
    update_until(&mut [&mut host, &mut client], |apps| {
        let world = apps[1].0.world_mut();
        world
            .query::<&PartyRounds>()
            .iter(world)
            .next()
            .is_some_and(|rounds| rounds.0 == ROUNDS + 2)
    });
    assert!(party(&mut host).is_none());
    assert_eq!(host.phase(), Some(SessionPhase::Lobby));
}
