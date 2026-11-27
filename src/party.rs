//! Parties: a set number of rounds, each a minigame the game picks, with points adding up to one
//! winner at the end.
//!
//! The host starts a party from the lobby with [`StartParty`]. Each round goes: a screen shows
//! the round and the minigame picked for it ([`SessionPhase::NextRound`]), the minigame, its results,
//! the standings, then the next round. After the last round, [`SessionPhase::PartyOver`] shows who
//! won, and everyone goes back to the lobby. A minigame that can't go on (a player left) is played
//! again with a new pick; with fewer than two players left, the party ends.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{server::Started, *};
use rand::Rng;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::config::party::{MAX_ROUNDS, MIN_ROUNDS, NEXT_ROUND_DURATION, ROUNDS, WINNER_DURATION};
use crate::config::session::MIN_PLAYERS;
use crate::minigames::Minigame;
use crate::network::is_host;
use crate::players::Player;
use crate::session::{CurrentPhase, PhaseTimer, SessionPhase, Then, can_start};
use crate::standings::{Points, standings};

/// A party being played. One at most, spawned by the host when it starts and replicated; it goes
/// when the session is back in the lobby.
#[derive(Component, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Party {
    /// How many rounds the party has.
    pub rounds: u32,
    /// Rounds whose minigame was played to the end. An abandoned minigame doesn't count: its
    /// round is played again.
    pub played: u32,
    /// The minigame picked for the coming round, once it has been picked.
    pub next: Option<Minigame>,
    /// The minigame of the last round played, so it isn't picked twice in a row.
    pub last: Option<Minigame>,
}

impl Party {
    /// A party of `rounds` rounds, none played yet.
    pub fn new(rounds: u32) -> Self {
        Self {
            rounds,
            played: 0,
            next: None,
            last: None,
        }
    }

    /// The round being played, or coming up next: 1 for the first.
    pub fn round(&self) -> u32 {
        self.played + 1
    }

    /// Whether every round has been played.
    pub fn is_over(&self) -> bool {
        self.played >= self.rounds
    }
}

/// Host only: how parties are played. Tests make the screens shorter.
#[derive(Resource, Debug, Clone)]
pub struct PartyRules {
    /// From [`MIN_ROUNDS`] to [`MAX_ROUNDS`]. Defaults to [`ROUNDS`]. The host picks it in the
    /// lobby.
    pub rounds: u32,
    /// How long each round's minigame is announced before it starts. Defaults to
    /// [`NEXT_ROUND_DURATION`].
    pub next_round: Duration,
    /// How long the winner is shown. Defaults to [`WINNER_DURATION`].
    pub winner: Duration,
    /// The minigames each round picks from. Defaults to all of them.
    pub games: Vec<Minigame>,
}

impl Default for PartyRules {
    fn default() -> Self {
        Self {
            rounds: ROUNDS,
            next_round: NEXT_ROUND_DURATION,
            winner: WINNER_DURATION,
            games: Minigame::ALL.to_vec(),
        }
    }
}

/// How many rounds the next party has: the host's [`PartyRules::rounds`], so joiners see it in
/// the lobby. One per session, spawned by the host and replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartyRounds(pub u32);

/// Command: change how many rounds the next party has by `.0`, keeping it from [`MIN_ROUNDS`] to
/// [`MAX_ROUNDS`]. Only means something on the host; the lobby only offers it there.
#[derive(Debug, Clone, Copy)]
pub struct ChangeRounds(pub i32);

impl Command for ChangeRounds {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Some(mut rules) = world.get_resource_mut::<PartyRules>() else {
            error!("Can't change the party's rounds: there are no party rules");
            return;
        };
        let rounds = rules
            .rounds
            .saturating_add_signed(self.0)
            .clamp(MIN_ROUNDS, MAX_ROUNDS);
        if rounds == rules.rounds {
            debug!("Party rounds stay at {rounds}: it goes from {MIN_ROUNDS} to {MAX_ROUNDS}");
            return;
        }
        info!("Party rounds {} -> {rounds}", rules.rounds);
        rules.rounds = rounds;
    }
}

/// Command: the host starts a party of [`PartyRules::rounds`] rounds from the lobby. Everyone's
/// points start again from 0. Refused unless hosting, in the lobby, with at least
/// [`MIN_PLAYERS`] players.
#[derive(Debug, Clone, Copy)]
pub struct StartParty;

impl Command for StartParty {
    type Out = ();

    fn apply(self, world: &mut World) {
        if !world.run_system_cached(is_host).unwrap_or(false) {
            warn!("Only the host can start a party");
            return;
        }
        let players = world.query::<&Player>().iter(world).len();
        if !can_start(players) {
            warn!("A party needs at least {MIN_PLAYERS} players, have {players}");
            return;
        }
        let Ok(mut phase) = world.query::<&mut CurrentPhase>().single_mut(world) else {
            return;
        };
        if phase.0 != SessionPhase::Lobby {
            warn!("Can't start a party outside the lobby");
            return;
        }
        phase.0 = SessionPhase::NextRound;
        for mut points in world.query::<&mut Points>().iter_mut(world) {
            *points = Points::default();
        }
        let rounds = world.resource::<PartyRules>().rounds;
        info!("Starting a party of {rounds} rounds for {players} players");
        world.spawn((
            Name::from("Party"),
            Party::new(rounds),
            DespawnOnEnter(SessionPhase::Lobby),
            DespawnOnEnter(AppState::MainMenu),
            Replicate::to_clients(NetworkTarget::All),
        ));
    }
}

/// One of `games` for a round with `players` players: any that can be started with that many,
/// but not `last` again unless it's the only one. `None` if none can.
pub fn pick_minigame(
    games: &[Minigame],
    players: usize,
    last: Option<Minigame>,
    rng: &mut impl Rng,
) -> Option<Minigame> {
    let playable: Vec<Minigame> = games
        .iter()
        .copied()
        .filter(|game| players >= game.info().min_players)
        .collect();
    let fresh: Vec<Minigame> = playable
        .iter()
        .copied()
        .filter(|game| Some(*game) != last)
        .collect();
    let choices = if fresh.is_empty() { &playable } else { &fresh };
    choices.choose(rng).copied()
}

/// The players with the most points: one, or several if they tie. Empty with no players.
pub fn party_winners(players: impl IntoIterator<Item = (Player, Points)>) -> Vec<Player> {
    standings(players)
        .into_iter()
        .filter(|standing| standing.place == 1)
        .map(|standing| standing.player)
        .collect()
}

/// "Ada wins the party!", or "Ada and Bo win the party!" on a tie.
pub fn winners_line(winners: &[Player]) -> String {
    let names: Vec<&str> = winners.iter().map(Player::name).collect();
    match names.as_slice() {
        [] => "Nobody wins the party.".into(),
        [one] => format!("{one} wins the party!"),
        [rest @ .., last] => format!("{} and {last} win the party!", rest.join(", ")),
    }
}

/// Must be added after [`SessionPlugin`](crate::session::SessionPlugin) and
/// [`StandingsPlugin`](crate::standings::StandingsPlugin).
pub struct PartyPlugin;

impl Plugin for PartyPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Party>().replicate();
        app.component::<PartyRounds>().replicate();
        app.init_resource::<PartyRules>();
        app.add_observer(spawn_party_rounds);
        app.add_systems(Update, publish_party_rounds.run_if(is_host));
        app.add_systems(
            OnEnter(SessionPhase::NextRound),
            start_round.run_if(is_host),
        );
        app.add_systems(OnEnter(SessionPhase::Results), count_round.run_if(is_host));
        app.add_systems(OnEnter(SessionPhase::PartyOver), end_party.run_if(is_host));
    }
}

/// Host only: when the session starts, spawn the [`PartyRounds`] everyone sees.
fn spawn_party_rounds(_trigger: On<Add<Started>>, rules: Res<PartyRules>, mut commands: Commands) {
    commands.spawn((
        Name::from("PartyRounds"),
        PartyRounds(rules.rounds),
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

/// Host only: keep [`PartyRounds`] up to date with the host's pick.
fn publish_party_rounds(rules: Res<PartyRules>, mut rounds: Single<&mut PartyRounds>) {
    rounds.set_if_neq(PartyRounds(rules.rounds));
}

/// Host only: as a round starts, pick its minigame and start it after a moment. With fewer
/// than two players left, end the party instead.
fn start_round(
    rules: Res<PartyRules>,
    party: Option<Single<&mut Party>>,
    players: Query<&Player>,
    mut phase: Single<&mut CurrentPhase>,
    mut commands: Commands,
) {
    let Some(mut party) = party.map(Single::into_inner) else {
        error!("Starting a party round, but there's no party");
        phase.0 = SessionPhase::Lobby;
        return;
    };
    let count = players.iter().len();
    if !can_start(count) {
        info!(
            "Ending the party after {} of {} rounds: {count} player(s) left",
            party.played, party.rounds
        );
        phase.0 = SessionPhase::Lobby;
        return;
    }
    let Some(game) = pick_minigame(&rules.games, count, party.last, &mut rand::rng()) else {
        error!("No minigame can be played by {count} players: ending the party");
        phase.0 = SessionPhase::Lobby;
        return;
    };
    info!(
        "Party round {} of {}: {game:?} for {count} players",
        party.round(),
        party.rounds
    );
    party.next = Some(game);
    commands.insert_resource(PhaseTimer {
        timer: Timer::new(rules.next_round, TimerMode::Once),
        during: SessionPhase::NextRound,
        then: Then::Minigame(game),
    });
}

/// Host only: a minigame was played to the end, so its round counts.
fn count_round(party: Option<Single<&mut Party>>) {
    let Some(mut party) = party.map(Single::into_inner) else {
        return;
    };
    party.played += 1;
    party.last = party.next.take();
    info!("Party round {} of {} played", party.played, party.rounds);
}

/// Host only: announce the winner, then go back to the lobby after a while.
fn end_party(rules: Res<PartyRules>, players: Query<(&Player, &Points)>, mut commands: Commands) {
    let winners = party_winners(
        players
            .iter()
            .map(|(player, points)| (player.clone(), *points)),
    );
    let points = players
        .iter()
        .map(|(_, points)| points.total)
        .max()
        .unwrap_or(0);
    info!("Party over: {} ({points} points)", winners_line(&winners));
    commands.insert_resource(PhaseTimer {
        timer: Timer::new(rules.winner, TimerMode::Once),
        during: SessionPhase::PartyOver,
        then: Then::Phase(SessionPhase::Lobby),
    });
}

#[cfg(test)]
mod tests {
    use super::{Party, party_winners, pick_minigame, winners_line};
    use crate::minigames::Minigame;
    use crate::players::Player;
    use crate::standings::Points;
    use lightyear::prelude::PeerId;

    fn player(number: u32, name: &str) -> Player {
        Player {
            id: PeerId::Netcode(number.into()),
            number,
            name: name.into(),
            color: 0,
        }
    }

    #[test]
    fn never_picks_the_same_minigame_twice_in_a_row() {
        let mut rng = rand::rng();
        for last in Minigame::ALL {
            for _ in 0..50 {
                assert_ne!(
                    pick_minigame(&Minigame::ALL, 4, Some(last), &mut rng),
                    Some(last)
                );
            }
        }
    }

    #[test]
    fn picks_every_minigame_sometimes() {
        let mut rng = rand::rng();
        for game in Minigame::ALL {
            assert!(
                (0..200).any(|_| pick_minigame(&Minigame::ALL, 2, None, &mut rng) == Some(game)),
                "{game:?} never picked"
            );
        }
    }

    #[test]
    fn picks_nothing_for_one_player() {
        assert_eq!(
            pick_minigame(&Minigame::ALL, 1, None, &mut rand::rng()),
            None
        );
    }

    #[test]
    fn repeats_the_only_minigame_it_has() {
        let only = [Minigame::Racing];
        let picked = pick_minigame(&only, 3, Some(Minigame::Racing), &mut rand::rng());
        assert_eq!(picked, Some(Minigame::Racing));
    }

    #[test]
    fn counts_rounds() {
        let mut party = Party::new(2);
        assert_eq!((party.round(), party.is_over()), (1, false));
        party.played = 1;
        assert_eq!((party.round(), party.is_over()), (2, false));
        party.played = 2;
        assert!(party.is_over());
    }

    #[test]
    fn the_most_points_win_and_ties_share() {
        let with = |number, name, total| (player(number, name), Points { total, gained: 0 });
        let winners = party_winners([with(1, "Ada", 3), with(2, "Bo", 5), with(3, "Cy", 1)]);
        assert_eq!(winners_line(&winners), "Bo wins the party!");
        let winners = party_winners([with(1, "Ada", 5), with(2, "Bo", 5), with(3, "Cy", 5)]);
        assert_eq!(winners_line(&winners), "Ada, Bo and Cy win the party!");
    }
}
