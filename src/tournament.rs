//! Knockout tournaments, for minigames with fewer seats than players (such as two-player Pong):
//! everyone plays matches until one champion is left. The [`Bracket`] is plain data; the host
//! keeps it in a replicated [`Tournament`], and the session runs the matches one after another
//! with a [`SessionPhase::NextMatch`](crate::session::SessionPhase::NextMatch) screen between.
//!
//! Players are seeded in the order given, and paired the standard way (1 v 4 and 2 v 3, …), so
//! the top two seeds can only meet in the final. If the number of players isn't a power of two,
//! the top seeds get byes into the second round: with 3 players, seed 1 waits for the winner of
//! 2 v 3. Every match but the byes is played, so `n` players play `n - 1` matches.
//!
//! Players are seeded by points, most first, and in random order among those with the same
//! points, so a session's first tournament is drawn at random.

use std::cmp::Reverse;
use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::config::session::NEXT_MATCH_DURATION;
use crate::minigames::Minigame;
use crate::players::Player;
use crate::session::{MinigameResult, Placement};
use crate::standings::Points;

/// A tournament in progress. One entity, spawned by the host when a tournament starts and
/// replicated, so every peer can show who plays next. Removed back in the lobby.
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

    /// Record the match that ended with `result`: its winner goes through.
    pub fn record(&mut self, result: &MinigameResult) -> Result<TournamentStep, String> {
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
        self.bracket
            .record_winner(winner)
            .map_err(|_| format!("{winner:?} won a match they weren't in"))?;
        let name = |id| self.bracket.player(id).map_or("?", Player::name);
        self.last_match = format!("{} beat {}", name(winner), name(loser));
        if !result.summary.is_empty() {
            self.last_match.push_str(&format!(" ({})", result.summary));
        }
        if let Some((_, next)) = self.bracket.next_match() {
            return Ok(TournamentStep::NextMatch(next));
        }
        let placements = self
            .bracket
            .placements()
            .ok_or("no match left, but no champion")?;
        Ok(TournamentStep::Over(MinigameResult {
            placements,
            summary: format!("Final: {}", self.last_match),
        }))
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

/// Registers [`Tournament`] for replication. Must be added after
/// [`SessionPlugin`](crate::session::SessionPlugin).
pub struct TournamentPlugin;

impl Plugin for TournamentPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Tournament>().replicate();
        app.init_resource::<TournamentRules>();
    }
}

/// Whether `game` is played as a tournament with `players` players: it seats two, and there are
/// more. (Minigames that seat more than two just seat the first players.)
pub fn needs_tournament(game: Minigame, players: usize) -> bool {
    game.max_players() == 2 && players > 2
}

/// `players` in seed order: most points first, and in random order among equal points.
pub fn seed(mut players: Vec<(Player, Points)>) -> Vec<Player> {
    players.shuffle(&mut rand::rng());
    // A stable sort keeps the shuffled order among equal points.
    players.sort_by_key(|(_, points)| Reverse(points.total));
    players.into_iter().map(|(player, _)| player).collect()
}

/// One pairing in a round: two players, or one with a bye, who wins it without playing.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub first: PeerId,
    /// `None` for a bye.
    pub second: Option<PeerId>,
    /// Set once the match is decided; straight away for a bye.
    pub winner: Option<PeerId>,
}

impl Match {
    fn new(first: PeerId, second: Option<PeerId>) -> Self {
        Self {
            first,
            second,
            winner: if second.is_none() { Some(first) } else { None },
        }
    }

    /// The player who lost, once it's decided. `None` for a bye.
    pub fn loser(&self) -> Option<PeerId> {
        let second = self.second?;
        match self.winner? {
            winner if winner == self.first => Some(second),
            _ => Some(self.first),
        }
    }

    fn has(&self, player: PeerId) -> bool {
        self.first == player || self.second == Some(player)
    }
}

/// A match as the bracket shows it, rounds not reached yet included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BracketMatch {
    /// `None` where it isn't decided yet who plays (or, second, for a bye).
    pub players: [Option<PeerId>; 2],
    pub winner: Option<PeerId>,
    pub bye: bool,
}

/// `record_winner` was given a player who isn't in the match being played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotInMatch(pub PeerId);

/// A knockout bracket. Rounds are added as the one before finishes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Bracket {
    /// Everyone who entered, in seed order. Kept so the bracket can name players who left.
    players: Vec<Player>,
    rounds: Vec<Vec<Match>>,
}

impl Bracket {
    /// A bracket for `seeded`, best seed first. `None` with fewer than two players.
    pub fn new(seeded: Vec<Player>) -> Option<Self> {
        if seeded.len() < 2 {
            return None;
        }
        let size = seeded.len().next_power_of_two();
        let seat = |seed: usize| seeded.get(seed - 1).map(|player| player.id);
        let first_round = seed_order(size)
            .chunks(2)
            .map(|pair| {
                // Byes only ever face a real player: they are the bottom seeds, paired with the top.
                let (first, second) = match (seat(pair[0]), seat(pair[1])) {
                    (Some(first), second) => (first, second),
                    (None, Some(second)) => (second, None),
                    (None, None) => unreachable!("a bye against a bye"),
                };
                Match::new(first, second)
            })
            .collect();
        Some(Self {
            players: seeded,
            rounds: vec![first_round],
        })
    }

    /// The rounds so far, first round first.
    pub fn rounds(&self) -> &[Vec<Match>] {
        &self.rounds
    }

    /// How many rounds the whole tournament has, the final included.
    pub fn round_count(&self) -> usize {
        self.players.len().next_power_of_two().trailing_zeros() as usize
    }

    /// "Final", "Semi-final", "Quarter-final", or "Round 1", … for round `round` (from 0).
    pub fn round_name(&self, round: usize) -> String {
        match self.round_count().saturating_sub(round + 1) {
            0 => "Final".into(),
            1 => "Semi-final".into(),
            2 => "Quarter-final".into(),
            _ => format!("Round {}", round + 1),
        }
    }

    /// Every round of the whole tournament, the final included. Rounds not reached yet are
    /// filled in with the winners so far: a semi-final can show "Ada vs ?" while the other
    /// quarter-final is still to play.
    pub fn overview(&self) -> Vec<Vec<BracketMatch>> {
        let mut overview: Vec<Vec<BracketMatch>> = Vec::new();
        for round in 0..self.round_count() {
            let matches = match self.rounds.get(round) {
                Some(pairings) => pairings
                    .iter()
                    .map(|pairing| BracketMatch {
                        players: [Some(pairing.first), pairing.second],
                        winner: pairing.winner,
                        bye: pairing.second.is_none(),
                    })
                    .collect(),
                None => overview
                    .last()
                    .expect("the first round always exists")
                    .chunks(2)
                    .map(|pair| BracketMatch {
                        players: [pair[0].winner, pair[1].winner],
                        winner: None,
                        bye: false,
                    })
                    .collect(),
            };
            overview.push(matches);
        }
        overview
    }

    /// The player with id `id`, if they entered.
    pub fn player(&self, id: PeerId) -> Option<&Player> {
        self.players.iter().find(|player| player.id == id)
    }

    /// The next match to play, as `(round, players)`. `None` once there's a champion.
    pub fn next_match(&self) -> Option<(usize, [PeerId; 2])> {
        let round = self.rounds.len() - 1;
        self.rounds[round].iter().find_map(|pairing| match pairing {
            Match {
                first,
                second: Some(second),
                winner: None,
            } => Some((round, [*first, *second])),
            _ => None,
        })
    }

    /// Record that `winner` won the [`next_match`](Self::next_match), and start the next round
    /// if that finished this one.
    pub fn record_winner(&mut self, winner: PeerId) -> Result<(), NotInMatch> {
        let round = self.rounds.last_mut().expect("a bracket has a first round");
        let pairing = round
            .iter_mut()
            .find(|pairing| pairing.winner.is_none())
            .filter(|pairing| pairing.has(winner))
            .ok_or(NotInMatch(winner))?;
        pairing.winner = Some(winner);

        let winners: Option<Vec<PeerId>> = round.iter().map(|pairing| pairing.winner).collect();
        if let Some(winners) = winners.filter(|winners| winners.len() > 1) {
            let next = winners
                .chunks(2)
                .map(|pair| Match::new(pair[0], Some(pair[1])))
                .collect();
            self.rounds.push(next);
        }
        Ok(())
    }

    /// The winner of the final, once it's played.
    pub fn champion(&self) -> Option<PeerId> {
        match self.rounds.last()?[..] {
            [
                Match {
                    winner: Some(champion),
                    ..
                },
            ] if self.rounds.len() == self.round_count() => Some(champion),
            _ => None,
        }
    }

    /// Everyone's place, once there's a champion: 1st the champion, 2nd the other finalist, then
    /// each earlier round's losers sharing the place after everyone who went further (3rd for
    /// both semi-final losers, 5th for the quarter-final losers, …). Seed order within a place.
    pub fn placements(&self) -> Option<Vec<Placement>> {
        let champion = self.champion()?;
        let mut placed: Vec<(u32, PeerId)> = vec![(1, champion)];
        for round in self.rounds.iter().rev() {
            // Everyone who got further than this round's losers: one per match in it.
            let place = round.len() as u32 + 1;
            let mut losers: Vec<PeerId> = round.iter().filter_map(Match::loser).collect();
            losers.sort_by_key(|loser| self.seed(*loser));
            placed.extend(losers.into_iter().map(|loser| (place, loser)));
        }
        Some(
            placed
                .into_iter()
                .filter_map(|(place, id)| Some(Placement::new(self.player(id)?, place)))
                .collect(),
        )
    }

    /// For the logs: every round so far, e.g. "Semi-final: Ada beat Dan, Bo v Cy · Final: …".
    pub fn describe(&self) -> String {
        let name = |id: PeerId| self.player(id).map_or("?", Player::name);
        self.rounds
            .iter()
            .enumerate()
            .map(|(round, pairings)| {
                let pairings: Vec<String> = pairings
                    .iter()
                    .map(
                        |pairing| match (pairing.second, pairing.winner, pairing.loser()) {
                            (None, _, _) => format!("{} (bye)", name(pairing.first)),
                            (Some(_), Some(winner), Some(loser)) => {
                                format!("{} beat {}", name(winner), name(loser))
                            }
                            (Some(second), _, _) => {
                                format!("{} v {}", name(pairing.first), name(second))
                            }
                        },
                    )
                    .collect();
                format!("{}: {}", self.round_name(round), pairings.join(", "))
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn seed(&self, id: PeerId) -> usize {
        self.players
            .iter()
            .position(|player| player.id == id)
            .unwrap_or(usize::MAX)
    }
}

/// The seeds in bracket order for a bracket of `size` (a power of two), so that each pair is a
/// first-round match and the best seeds meet as late as possible: `[1, 4, 2, 3]` for 4,
/// `[1, 8, 4, 5, 2, 7, 3, 6]` for 8.
fn seed_order(size: usize) -> Vec<usize> {
    let mut order = vec![1];
    while order.len() < size {
        let opponent = order.len() * 2 + 1;
        order = order
            .into_iter()
            .flat_map(|seed| [seed, opponent - seed])
            .collect();
    }
    order
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::PeerId;

    use super::{Bracket, BracketMatch, NotInMatch, Tournament, TournamentStep, seed, seed_order};
    use crate::minigames::Minigame;
    use crate::players::Player;
    use crate::session::{MinigameResult, Placement};
    use crate::standings::Points;

    fn id(seed: u64) -> PeerId {
        PeerId::Netcode(seed)
    }

    /// A bracket of `n` players named by seed: "1", "2", …
    fn bracket(n: u64) -> Bracket {
        let players = (1..=n)
            .map(|seed| Player {
                id: id(seed),
                number: seed as u32,
                name: seed.to_string(),
                color: 0,
            })
            .collect();
        Bracket::new(players).unwrap()
    }

    /// The next match, as seeds.
    fn next(bracket: &Bracket) -> Option<(usize, [u64; 2])> {
        let seed = |player: PeerId| match player {
            PeerId::Netcode(seed) => seed,
            other => panic!("{other:?}"),
        };
        bracket
            .next_match()
            .map(|(round, [a, b])| (round, [seed(a), seed(b)]))
    }

    /// Play every match, the better seed (lower number) always winning.
    fn play_out(bracket: &mut Bracket) -> usize {
        let mut played = 0;
        while let Some((_, [a, b])) = next(bracket) {
            bracket.record_winner(id(a.min(b))).unwrap();
            played += 1;
        }
        played
    }

    fn places(bracket: &Bracket) -> Vec<(u32, String)> {
        bracket
            .placements()
            .unwrap()
            .into_iter()
            .map(|placement| (placement.place, placement.name))
            .collect()
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

    #[test]
    fn a_tournament_goes_match_by_match_to_a_result() {
        let players = (1..=3).map(player).collect();
        let mut tournament = Tournament::new(Minigame::TwoPlayerPong, players).unwrap();
        assert_eq!(
            tournament.record(&won(3, 2)),
            Ok(TournamentStep::NextMatch([id(1), id(3)]))
        );
        assert_eq!(tournament.last_match, "3 beat 2 (1 - 0)");
        let Ok(TournamentStep::Over(result)) = tournament.record(&won(1, 3)) else {
            panic!("the final ends it");
        };
        let order: Vec<String> = result.placements.iter().map(|p| p.describe()).collect();
        assert_eq!(order, ["1st 1", "2nd 3", "3rd 2"]);
        assert_eq!(result.summary, "Final: 1 beat 3 (1 - 0)");
    }

    #[test]
    fn a_result_for_someone_not_playing_is_refused() {
        let players = (1..=3).map(player).collect();
        let mut tournament = Tournament::new(Minigame::TwoPlayerPong, players).unwrap();
        assert!(tournament.record(&won(1, 2)).is_err());
        assert_eq!(tournament.bracket.next_match(), Some((0, [id(2), id(3)])));
    }

    #[test]
    fn seeds_meet_as_late_as_possible() {
        assert_eq!(seed_order(2), [1, 2]);
        assert_eq!(seed_order(4), [1, 4, 2, 3]);
        assert_eq!(seed_order(8), [1, 8, 4, 5, 2, 7, 3, 6]);
    }

    #[test]
    fn the_overview_fills_later_rounds_with_the_winners_so_far() {
        let mut bracket = bracket(4);
        bracket.record_winner(id(1)).unwrap();
        let overview = bracket.overview();
        assert_eq!(overview.len(), 2);
        assert_eq!(
            overview[1],
            [BracketMatch {
                players: [Some(id(1)), None],
                winner: None,
                bye: false,
            }]
        );
        bracket.record_winner(id(3)).unwrap();
        assert_eq!(bracket.overview()[1][0].players, [Some(id(1)), Some(id(3))]);
    }

    #[test]
    fn the_overview_shows_byes_and_every_round() {
        let overview = bracket(5).overview();
        let sizes: Vec<usize> = overview.iter().map(Vec::len).collect();
        assert_eq!(sizes, [4, 2, 1]);
        let byes = overview[0].iter().filter(|pairing| pairing.bye).count();
        assert_eq!(byes, 3);
        // Byes go through straight away, so the semi-final with two of them is already set.
        assert_eq!(overview[1][1].players, [Some(id(2)), Some(id(3))]);
    }

    #[test]
    fn needs_two_players() {
        assert!(Bracket::new(Vec::new()).is_none());
        assert!(Bracket::new(bracket(2).players[..1].to_vec()).is_none());
    }

    #[test]
    fn two_players_play_a_final() {
        let mut bracket = bracket(2);
        assert_eq!(bracket.round_name(0), "Final");
        assert_eq!(next(&bracket), Some((0, [1, 2])));
        bracket.record_winner(id(2)).unwrap();
        assert_eq!(next(&bracket), None);
        assert_eq!(bracket.champion(), Some(id(2)));
        assert_eq!(places(&bracket), [(1, "2".into()), (2, "1".into())]);
    }

    #[test]
    fn with_three_players_the_top_seed_gets_a_bye() {
        let mut bracket = bracket(3);
        assert_eq!(bracket.describe(), "Semi-final: 1 (bye), 2 v 3");
        assert_eq!(next(&bracket), Some((0, [2, 3])));
        bracket.record_winner(id(3)).unwrap();
        assert_eq!(next(&bracket), Some((1, [1, 3])));
        bracket.record_winner(id(3)).unwrap();
        assert_eq!(
            bracket.describe(),
            "Semi-final: 1 (bye), 3 beat 2 · Final: 3 beat 1"
        );
        assert_eq!(
            places(&bracket),
            [(1, "3".into()), (2, "1".into()), (3, "2".into())]
        );
    }

    #[test]
    fn four_players_play_semi_finals_then_a_final() {
        let mut bracket = bracket(4);
        assert_eq!(next(&bracket), Some((0, [1, 4])));
        bracket.record_winner(id(1)).unwrap();
        assert_eq!(next(&bracket), Some((0, [2, 3])));
        bracket.record_winner(id(3)).unwrap();
        assert_eq!(next(&bracket), Some((1, [1, 3])));
        assert_eq!(bracket.round_name(1), "Final");
    }

    #[test]
    fn five_players_need_three_byes() {
        let mut bracket = bracket(5);
        assert_eq!(
            bracket.describe(),
            "Quarter-final: 1 (bye), 4 v 5, 2 (bye), 3 (bye)"
        );
        assert_eq!(play_out(&mut bracket), 4);
        assert_eq!(
            places(&bracket),
            [
                (1, "1".into()),
                (2, "2".into()),
                (3, "3".into()),
                (3, "4".into()),
                (5, "5".into())
            ]
        );
    }

    #[test]
    fn eight_players_share_places_by_the_round_they_went_out() {
        let mut bracket = bracket(8);
        assert_eq!(play_out(&mut bracket), 7);
        let shared: Vec<u32> = places(&bracket).iter().map(|(place, _)| *place).collect();
        assert_eq!(shared, [1, 2, 3, 3, 5, 5, 5, 5]);
    }

    #[test]
    fn n_players_play_n_minus_one_matches() {
        for n in 2..=16 {
            let mut bracket = bracket(n);
            assert_eq!(play_out(&mut bracket), n as usize - 1, "{n} players");
            assert_eq!(bracket.champion(), Some(id(1)), "{n} players");
            assert_eq!(
                bracket.placements().unwrap().len(),
                n as usize,
                "{n} players"
            );
        }
    }

    #[test]
    fn only_a_player_in_the_match_can_win_it() {
        let mut bracket = bracket(4);
        let before = bracket.clone();
        assert_eq!(bracket.record_winner(id(2)), Err(NotInMatch(id(2))));
        assert_eq!(bracket, before);
    }

    #[test]
    fn no_placements_before_the_final() {
        let mut bracket = bracket(4);
        bracket.record_winner(id(1)).unwrap();
        assert_eq!(bracket.champion(), None);
        assert_eq!(bracket.placements(), None);
    }
}
