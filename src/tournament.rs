//! Knockout tournaments, for minigames with fewer seats than players (such as two-player Pong):
//! everyone plays matches until one champion is left. The [`Bracket`] is plain data; the host
//! keeps it in a replicated [`Tournament`], and runs the matches one after another with a
//! [`SessionPhase::NextMatch`] screen between. While a tournament is running, this module, not the
//! session, decides what happens when a match ends ([`MinigameEnded`]) or a player in one leaves.
//!
//! Players are seeded by points, most first, and in random order among those with the same
//! points, so a session's first tournament is drawn at random.

mod bracket;

pub use bracket::{Bracket, BracketMatch, Match, NotInMatch};

use std::cmp::Reverse;
use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::config::session::NEXT_MATCH_DURATION;
use crate::minigames::Minigame;
use crate::minigames::outcome::{MinigameEnded, MinigameResult, Participants};
use crate::network::{Hosting, is_host};
use crate::party::Party;
use crate::players::{Player, PlayerLeft};
use crate::session::{
    CurrentPhase, MoveOnAfter, SessionPhase, Then, after_abandoning, show_result,
};
use crate::standings::Points;

/// A tournament in progress. One entity, spawned by the host when a tournament starts and
/// replicated, so every peer can show who plays next. Removed back in the lobby, or as a
/// [`Party`]'s next round starts.
#[derive(Component, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Tournament {
    pub game: Minigame,
    pub bracket: Bracket,
    /// How the last match went, e.g. "Ada beat Bo (3 - 1)". Empty before the first.
    pub last_match: String,
}

/// What [`Tournament::record`] makes of a finished match.
#[derive(Debug, Clone, PartialEq)]
pub enum TournamentStep {
    /// Play this match next.
    NextMatch([PeerId; 2]),
    /// That was the final: the tournament's result, everyone placed.
    Over(MinigameResult),
}

impl Tournament {
    /// A tournament of `game` between `seeded`, best seed first. `None` with fewer than two.
    pub fn new(game: Minigame, seeded: Vec<Player>) -> Option<Self> {
        Some(Self {
            game,
            bracket: Bracket::new(seeded)?,
            last_match: String::new(),
        })
    }

    /// Record the match that ended with `result`: its winner goes through. Then any coming match
    /// with a player who isn't `present` (they left the session) is a walkover.
    pub fn record(
        &mut self,
        result: &MinigameResult,
        present: &[PeerId],
    ) -> Result<TournamentStep, String> {
        let Some((_, players)) = self.bracket.next_match() else {
            return Err("the tournament is already over".into());
        };
        let winner = result
            .winners()
            .next()
            .ok_or("the match has no winner")?
            .player;
        let loser = players
            .into_iter()
            .find(|player| *player != winner)
            .unwrap_or(winner);
        let mut note = format!("{} beat {}", self.name(winner), self.name(loser));
        if !result.summary.is_empty() {
            note.push_str(&format!(" ({})", result.summary));
        }
        self.bracket
            .record_winner(winner)
            .map_err(|_| format!("{winner:?} won a match they weren't in"))?;
        let notes: Vec<String> = std::iter::once(note)
            .chain(self.walk_over_absent(present))
            .collect();
        self.last_match = notes.join(" · ");
        self.step(present, notes.last().map_or("", String::as_str))
    }

    /// Players who aren't `present` (they left the session) lose the coming match by walkover,
    /// and so on until a match has both its players. Then what comes next.
    pub fn walk_over(&mut self, present: &[PeerId]) -> Result<TournamentStep, String> {
        let notes = self.walk_over_absent(present);
        if !notes.is_empty() {
            self.last_match = notes.join(" · ");
        }
        let latest = notes
            .last()
            .cloned()
            .unwrap_or_else(|| self.last_match.clone());
        self.step(present, &latest)
    }

    /// Decide every coming match a player isn't `present` for: the other goes through, or if
    /// both left, the first (who then loses their next match the same way). One note per match.
    fn walk_over_absent(&mut self, present: &[PeerId]) -> Vec<String> {
        let mut notes = Vec::new();
        while let Some((_, [first, second])) = self.bracket.next_match() {
            let (first_name, second_name) = (self.name(first), self.name(second));
            let (winner, note) = match (present.contains(&first), present.contains(&second)) {
                (true, true) => break,
                (true, false) => (
                    first,
                    format!("{second_name} left, {first_name} goes through"),
                ),
                (false, true) => (
                    second,
                    format!("{first_name} left, {second_name} goes through"),
                ),
                (false, false) => (first, format!("{first_name} and {second_name} both left")),
            };
            self.bracket
                .record_winner(winner)
                .expect("the winner is one of the match's players");
            notes.push(note);
        }
        notes
    }

    /// The next match, or the result: everyone placed, those no longer `present` marked "left",
    /// with `latest` (how the final went) as the summary.
    fn step(&self, present: &[PeerId], latest: &str) -> Result<TournamentStep, String> {
        if let Some((_, next)) = self.bracket.next_match() {
            return Ok(TournamentStep::NextMatch(next));
        }
        let mut placements = self
            .bracket
            .placements()
            .ok_or("no match left, but no champion")?;
        for placement in &mut placements {
            if !present.contains(&placement.player) {
                placement.note = "left".into();
            }
        }
        Ok(TournamentStep::Over(MinigameResult {
            placements,
            summary: format!("Final: {latest}"),
        }))
    }

    fn name(&self, id: PeerId) -> String {
        self.bracket
            .player(id)
            .map_or("?", Player::name)
            .to_string()
    }
}

/// Host only: how tournaments run.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TournamentRules {
    /// How long the screen before each match stays up. Defaults to [`NEXT_MATCH_DURATION`].
    pub next_match: Duration,
}

impl Default for TournamentRules {
    fn default() -> Self {
        Self {
            next_match: NEXT_MATCH_DURATION,
        }
    }
}

/// Registers [`Tournament`] for replication, and runs tournaments on the host. Must be added
/// after [`SessionPlugin`](crate::session::SessionPlugin).
pub struct TournamentPlugin;

impl Plugin for TournamentPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Tournament>().replicate();
        app.init_resource::<TournamentRules>();
        app.add_observer(end_match);
        app.add_observer(walk_over_leavers);
        app.add_systems(
            OnEnter(SessionPhase::NextMatch),
            start_next_match_timer.run_if(is_host),
        );
        app.add_systems(
            OnEnter(SessionPhase::NextRound),
            end_last_rounds_tournament.run_if(is_host),
        );
    }
}

/// Whether `game` is played as a tournament with `players` players: it seats two, and there are
/// more. (Minigames that seat more than two just seat the first players.)
pub fn needs_tournament(game: Minigame, players: usize) -> bool {
    game.info().max_players == 2 && players > 2
}

/// `players` in seed order: most points first, and in random order among equal points.
pub fn seed(mut players: Vec<(Player, Points)>) -> Vec<Player> {
    players.shuffle(&mut rand::rng());
    // A stable sort keeps the shuffled order among equal points.
    players.sort_by_key(|(_, points)| Reverse(points.total));
    players.into_iter().map(|(player, _)| player).collect()
}

/// Host only: start a tournament of `game` between `standings` (every player here, with their
/// points), and show its first match. If it can't be drawn, go to `abandon_to` instead.
pub(crate) fn start_tournament(
    world: &mut World,
    game: Minigame,
    standings: Vec<(Player, Points)>,
    abandon_to: SessionPhase,
) {
    let count = standings.len();
    let drawn = Tournament::new(game, seed(standings)).and_then(|tournament| {
        let (_, first) = tournament.bracket.next_match()?;
        Some((tournament, first))
    });
    let Ok((mut phase, mut participants)) = world
        .query::<(&mut CurrentPhase, &mut Participants)>()
        .single_mut(world)
    else {
        return;
    };
    let Some((tournament, first)) = drawn else {
        error!("Couldn't draw a {game:?} tournament with a first match for {count} players");
        phase.0 = abandon_to;
        return;
    };
    info!(
        "Starting a {game:?} tournament for {count} players: {}",
        tournament.bracket.describe()
    );
    participants.0 = first.to_vec();
    phase.0 = SessionPhase::NextMatch;
    world.spawn((
        Name::from("Tournament"),
        tournament,
        DespawnOnEnter(SessionPhase::Lobby),
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

/// Host only: a match ended. Its winner goes through, or if it was abandoned, a player who left
/// loses it by walkover; then the next match, or the tournament's result. The session leaves
/// [`MinigameEnded`] to this while a tournament is running.
fn end_match(
    trigger: On<MinigameEnded>,
    session: Single<(&mut CurrentPhase, &mut Participants)>,
    tournament: Option<Single<&mut Tournament>>,
    party: Option<Single<&Party>>,
    mut players: Query<(&Player, &mut Points)>,
    mut commands: Commands,
) {
    let Some(mut tournament) = tournament.map(Single::into_inner) else {
        return;
    };
    let (mut phase, mut participants) = session.into_inner();
    if !phase.0.is_minigame() {
        return;
    }
    let abandon_to = after_abandoning(party.is_some());
    let present: Vec<PeerId> = players.iter().map(|(player, _)| player.id).collect();
    let step = match trigger.event() {
        MinigameEnded::Won(result) => tournament.record(result, &present),
        MinigameEnded::Abandoned => {
            // A match usually stops because a player left: they lose it by walkover.
            if present.len() < 2 {
                info!("Abandoning the tournament: fewer than two players are left");
                phase.0 = abandon_to;
                return;
            }
            if participants.0.iter().all(|player| present.contains(player)) {
                info!(
                    "Tournament match abandoned with both its players here: ending the tournament"
                );
                phase.0 = abandon_to;
                return;
            }
            tournament.walk_over(&present)
        }
    };
    follow_tournament(
        step,
        &tournament,
        abandon_to,
        &mut phase,
        &mut participants,
        &mut players,
        &mut commands,
    );
}

/// Host only: when a player in the current or coming tournament match leaves, they lose it by
/// walkover straight away, rather than when the minigame notices. A player waiting for a later
/// match loses that one when it comes up. With fewer than two players left, the tournament ends.
fn walk_over_leavers(
    trigger: On<PlayerLeft>,
    hosting: Hosting,
    session: Single<(&mut CurrentPhase, &mut Participants)>,
    tournament: Option<Single<&mut Tournament>>,
    party: Option<Single<&Party>>,
    mut players: Query<(&Player, &mut Points)>,
    mut commands: Commands,
) {
    let (Some(tournament), true) = (tournament, hosting.is_hosting()) else {
        return;
    };
    let abandon_to = after_abandoning(party.is_some());
    let mut tournament = tournament.into_inner();
    let (mut phase, mut participants) = session.into_inner();
    let PlayerLeft(leaver) = trigger.event();
    let in_play =
        phase.0 == SessionPhase::NextMatch || phase.0 == SessionPhase::Playing(tournament.game);
    if !in_play || !participants.contains(leaver.id) {
        return;
    }
    // Their player is still being removed, so leave them out by hand.
    let present: Vec<PeerId> = players
        .iter()
        .map(|(player, _)| player.id)
        .filter(|id| *id != leaver.id)
        .collect();
    if present.len() < 2 {
        info!(
            "Abandoning the tournament: {} left, and fewer than two players are left",
            leaver.name()
        );
        phase.0 = abandon_to;
        return;
    }
    info!(
        "{} ({:?}) left mid-tournament: they lose their match by walkover",
        leaver.name(),
        leaver.id
    );
    let step = tournament.walk_over(&present);
    follow_tournament(
        step,
        &tournament,
        abandon_to,
        &mut phase,
        &mut participants,
        &mut players,
        &mut commands,
    );
}

/// Host only: play the tournament's next match, or show its result. If it can't go on, go to
/// `abandon_to`.
fn follow_tournament(
    step: Result<TournamentStep, String>,
    tournament: &Tournament,
    abandon_to: SessionPhase,
    phase: &mut CurrentPhase,
    participants: &mut Participants,
    players: &mut Query<(&Player, &mut Points)>,
    commands: &mut Commands,
) {
    match step {
        Ok(TournamentStep::NextMatch(next)) => {
            info!(
                "Tournament goes on ({}). {}",
                tournament.last_match,
                tournament.bracket.describe()
            );
            participants.0 = next.to_vec();
            if phase.0 == SessionPhase::NextMatch {
                // A walkover paired a new match on the next-match screen. `OnEnter` won't run
                // again, so give the new pair the whole countdown here.
                debug!("New match paired on the next-match screen: restarting its countdown");
                commands.run_system_cached(start_next_match_timer);
            }
            phase.0 = SessionPhase::NextMatch;
        }
        Ok(TournamentStep::Over(result)) => {
            info!("Tournament over. {}", tournament.bracket.describe());
            show_result(&result, phase, players, commands);
        }
        Err(why) => {
            error!("Abandoning the tournament: {why}");
            phase.0 = abandon_to;
        }
    }
}

/// Host only: give the players a moment to see who's up, then start the tournament's next match.
fn start_next_match_timer(
    tournament: Option<Single<&Tournament>>,
    rules: Res<TournamentRules>,
    mut phase: Single<&mut CurrentPhase>,
    mut commands: Commands,
) {
    let Some(tournament) = tournament else {
        error!("Waiting for a tournament's next match, but there's no tournament");
        phase.0 = SessionPhase::Lobby;
        return;
    };
    commands.queue(MoveOnAfter {
        during: SessionPhase::NextMatch,
        wait: rules.next_match,
        then: Then::Phase(SessionPhase::Playing(tournament.game)),
    });
}

/// Host only: in a [`Party`], the last round's tournament is still here as the next round starts,
/// since the session hasn't been back to the lobby. It's over, so remove it.
fn end_last_rounds_tournament(
    tournaments: Query<Entity, With<Tournament>>,
    mut commands: Commands,
) {
    for tournament in &tournaments {
        debug!("A party round starts: removing the last round's tournament");
        commands.entity(tournament).despawn();
    }
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::PeerId;

    use super::{Tournament, TournamentStep, seed};
    use crate::minigames::Minigame;
    use crate::minigames::outcome::{MinigameResult, Placement};
    use crate::players::Player;
    use crate::standings::Points;

    fn id(seed: u64) -> PeerId {
        PeerId::Netcode(seed)
    }

    fn player(seed: u64) -> Player {
        Player {
            id: id(seed),
            number: seed as u32,
            name: seed.to_string(),
            color: 0,
        }
    }

    /// A match result won by seed `winner` over seed `loser`.
    fn won(winner: u64, loser: u64) -> MinigameResult {
        MinigameResult {
            placements: vec![
                Placement::new(&player(winner), 1),
                Placement::new(&player(loser), 2),
            ],
            summary: "1 - 0".into(),
        }
    }

    #[test]
    fn seeds_by_points_with_ties_in_any_order() {
        let with = |number, total| (player(number), Points { total, gained: 0 });
        for _ in 0..20 {
            let seeded: Vec<u32> = seed(vec![with(1, 0), with(2, 5), with(3, 0), with(4, 2)])
                .iter()
                .map(|player| player.number)
                .collect();
            assert_eq!(seeded[..2], [2, 4]);
            assert!(seeded[2..] == [1, 3] || seeded[2..] == [3, 1], "{seeded:?}");
        }
    }

    fn tournament(players: u64) -> Tournament {
        let players = (1..=players).map(player).collect();
        Tournament::new(Minigame::TwoPlayerPong, players).unwrap()
    }

    fn step_players(step: Result<TournamentStep, String>) -> [PeerId; 2] {
        match step {
            Ok(TournamentStep::NextMatch(players)) => players,
            other => panic!("expected a next match: {other:?}"),
        }
    }

    #[test]
    fn a_player_who_left_loses_their_match_by_walkover() {
        let mut tournament = tournament(3);
        // Seed 3 leaves before playing seed 2.
        let next = step_players(tournament.walk_over(&[id(1), id(2)]));
        assert_eq!(next, [id(1), id(2)]);
        assert_eq!(tournament.last_match, "3 left, 2 goes through");
    }

    #[test]
    fn a_later_match_is_a_walkover_when_it_comes_up() {
        let mut tournament = tournament(4);
        // Seed 2 leaves while 1 v 4 is played: 2 v 3 is then a walkover.
        let present = [id(1), id(3), id(4)];
        let next = step_players(tournament.record(&won(1, 4), &present));
        assert_eq!(next, [id(1), id(3)]);
        assert_eq!(
            tournament.last_match,
            "1 beat 4 (1 - 0) · 2 left, 3 goes through"
        );
    }

    #[test]
    fn a_walkover_in_the_final_ends_the_tournament() {
        let mut tournament = tournament(3);
        let everyone = [id(1), id(2), id(3)];
        step_players(tournament.record(&won(2, 3), &everyone));
        let Ok(TournamentStep::Over(result)) = tournament.walk_over(&[id(2), id(3)]) else {
            panic!("the final was a walkover");
        };
        let order: Vec<String> = result.placements.iter().map(|p| p.describe()).collect();
        assert_eq!(order, ["1st 2", "2nd 1 (left)", "3rd 3"]);
        assert_eq!(result.summary, "Final: 1 left, 2 goes through");
    }

    #[test]
    fn when_both_left_one_goes_through_to_lose_the_next_match() {
        let mut tournament = tournament(4);
        // 2 and 3 both left while 1 v 4 was played: 2 goes through on paper, and loses the
        // final to 1 by walkover.
        let Ok(TournamentStep::Over(result)) = tournament.record(&won(1, 4), &[id(1), id(4)])
        else {
            panic!("everything after 1 v 4 was a walkover");
        };
        assert_eq!(
            tournament.last_match,
            "1 beat 4 (1 - 0) · 2 and 3 both left · 2 left, 1 goes through"
        );
        let order: Vec<String> = result.placements.iter().map(|p| p.describe()).collect();
        assert_eq!(order, ["1st 1", "2nd 2 (left)", "3rd 3 (left)", "3rd 4"]);
    }

    #[test]
    fn a_tournament_goes_match_by_match_to_a_result() {
        let mut tournament = tournament(3);
        let everyone = [id(1), id(2), id(3)];
        assert_eq!(
            tournament.record(&won(3, 2), &everyone),
            Ok(TournamentStep::NextMatch([id(1), id(3)]))
        );
        assert_eq!(tournament.last_match, "3 beat 2 (1 - 0)");
        let Ok(TournamentStep::Over(result)) = tournament.record(&won(1, 3), &everyone) else {
            panic!("the final ends it");
        };
        let order: Vec<String> = result.placements.iter().map(|p| p.describe()).collect();
        assert_eq!(order, ["1st 1", "2nd 3", "3rd 2"]);
        assert_eq!(result.summary, "Final: 1 beat 3 (1 - 0)");
    }

    #[test]
    fn a_result_for_someone_not_playing_is_refused() {
        let mut tournament = tournament(3);
        assert!(
            tournament
                .record(&won(1, 2), &[id(1), id(2), id(3)])
                .is_err()
        );
        assert_eq!(tournament.bracket.next_match(), Some((0, [id(2), id(3)])));
    }
}
