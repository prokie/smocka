use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{server::Started, *};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::config::session::{MIN_PLAYERS, RESULTS_DURATION, STANDINGS_DURATION};
use crate::minigames::Minigame;
use crate::network::{Hosting, is_host};
use crate::party::Party;
use crate::players::{Player, PlayerLeft};
use crate::standings::{Points, award_points};
use crate::tournament::{Tournament, TournamentRules, TournamentStep, needs_tournament, seed};

/// What the session is doing. A local Bevy sub-state of [`AppState::InSession`] that follows the
/// host's [`CurrentPhase`].
#[derive(SubStates, Serialize, Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[source(AppState = AppState::InSession)]
pub enum SessionPhase {
    /// Players gather until the host starts a minigame or a [`Party`].
    #[default]
    Lobby,
    /// In a [`Party`], before each round: which round it is, and the minigame picked for it.
    NextRound,
    /// Playing two-player Pong.
    TwoPlayerPong,
    /// Playing the racing minigame.
    Racing,
    /// Between a tournament's matches: who plays next, and how the last match went.
    NextMatch,
    /// Showing how everyone placed in the last minigame.
    Results,
    /// Showing everyone's points so far, before the next round of a [`Party`] or the lobby.
    Standings,
    /// After a [`Party`]'s last round: who won it, before going back to the lobby.
    PartyOver,
}

impl SessionPhase {
    /// Whether this phase is a minigame being played.
    pub fn is_minigame(self) -> bool {
        match self {
            SessionPhase::TwoPlayerPong | SessionPhase::Racing => true,
            SessionPhase::Lobby
            | SessionPhase::NextRound
            | SessionPhase::NextMatch
            | SessionPhase::Results
            | SessionPhase::Standings
            | SessionPhase::PartyOver => false,
        }
    }
}

/// The host's decision about the [`SessionPhase`], replicated to every client.
///
/// Bevy states can't be replicated, so only the host changes this, and every peer (host included)
/// copies it into its local [`SessionPhase`].
#[derive(Component, Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct CurrentPhase(pub SessionPhase);

/// Who plays the current minigame, in seat order: Pong's left then right paddle, racing's grid
/// slots. In a [`Tournament`], the two in the current (or next) match. Chosen by the host, kept
/// next to the [`CurrentPhase`] and replicated, so every peer knows who plays and who watches.
/// Minigames seat exactly these players rather than picking their own.
#[derive(Component, Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Participants(pub Vec<PeerId>);

impl Participants {
    /// Whether `player` plays the current minigame.
    pub fn contains(&self, player: PeerId) -> bool {
        self.0.contains(&player)
    }
}

/// How the last minigame went, shown during [`SessionPhase::Results`]. Spawned by the host and
/// replicated to everyone; removed when the results end.
#[derive(Component, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MinigameResult {
    /// Everyone who played, best first. Players who only watched (such as Pong's spectators)
    /// aren't in it.
    pub placements: Vec<Placement>,
    /// A short summary of how the game ended, such as the final score. Can be empty.
    pub summary: String,
}

impl MinigameResult {
    /// Who came first: one player, or several if they tied.
    pub fn winners(&self) -> impl Iterator<Item = &Placement> {
        self.placements
            .iter()
            .filter(|placement| placement.place == 1)
    }
}

/// One player's place in a [`MinigameResult`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The [`Player::id`].
    pub player: PeerId,
    /// The player's name and [`Player::color`] when the minigame ended, so they can still be
    /// shown if the player has left since.
    pub name: String,
    pub color: u8,
    /// 1 for the winner, counting up. Players who tie share a place, and the next place is
    /// skipped (1st, 1st, 3rd).
    pub place: u32,
    /// A short note on how they got there, such as "DNF". Usually empty.
    pub note: String,
}

impl Placement {
    /// `player` in place `place`, with no note.
    pub fn new(player: &Player, place: u32) -> Self {
        Self {
            player: player.id,
            name: player.name.clone(),
            color: player.color,
            place,
            note: String::new(),
        }
    }

    /// For example "1st Ada" or "3rd Bo (DNF)".
    pub fn describe(&self) -> String {
        let mut line = format!("{} {}", ordinal(self.place), self.name);
        if !self.note.is_empty() {
            line.push_str(&format!(" ({})", self.note));
        }
        line
    }
}

/// `1st`, `2nd`, `3rd`, `4th`, …, `11th`, `12th`, `13th`, `21st`, …
pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Host only: trigger this when the current minigame is over. Triggering it outside a minigame,
/// or more than once, does nothing.
#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub enum MinigameEnded {
    /// The game was played to the end: show the result and the standings, then go back to the
    /// lobby, or on to a [`Party`]'s next round. In a [`Tournament`], a match ended instead: the
    /// next match follows, or after the final, the tournament's result.
    Won(MinigameResult),
    /// The game can't go on (for example, a player left): go straight back to the lobby, or in
    /// a [`Party`], on to the round again with a new minigame. Ends a [`Tournament`] too.
    Abandoned,
}

/// Host only: how long the current screen between minigames has left, and what comes after it.
/// Only counts while the phase is still `during`: a walkover can end a tournament mid-countdown.
#[derive(Resource, Debug)]
pub(crate) struct PhaseTimer {
    pub timer: Timer,
    pub during: SessionPhase,
    pub then: Then,
}

/// What a [`PhaseTimer`] does when it runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Then {
    /// Move on to this phase.
    Phase(SessionPhase),
    /// Start this minigame with everyone here (see [`BeginMinigame`]).
    Minigame(Minigame),
}

/// Host only: how long the screens after a minigame stay up. Tests make them shorter.
#[derive(Resource, Debug, Clone)]
pub struct SessionRules {
    /// How long a minigame's result stays up. Defaults to [`RESULTS_DURATION`].
    pub results: Duration,
    /// How long the standings stay up. Defaults to [`STANDINGS_DURATION`].
    pub standings: Duration,
}

impl Default for SessionRules {
    fn default() -> Self {
        Self {
            results: RESULTS_DURATION,
            standings: STANDINGS_DURATION,
        }
    }
}

/// Where the session goes when a minigame can't go on: back to the lobby, or in a [`Party`] to
/// the round's start, which picks a new minigame for it (or ends the party, if too few players
/// are left).
fn after_abandoning(in_party: bool) -> SessionPhase {
    if in_party {
        SessionPhase::NextRound
    } else {
        SessionPhase::Lobby
    }
}

/// Must be added after [`AppStatePlugin`](crate::app_state::AppStatePlugin) and before
/// [`MinigamesPlugin`](crate::minigames::MinigamesPlugin).
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_sub_state::<SessionPhase>();
        app.init_resource::<SessionRules>();
        app.component::<CurrentPhase>().replicate();
        app.component::<Participants>().replicate();
        app.component::<MinigameResult>().replicate();

        app.add_observer(spawn_current_phase);
        app.add_observer(end_minigame);
        app.add_observer(walk_over_leavers);
        app.add_systems(
            PreUpdate,
            follow_phase.run_if(in_state(AppState::InSession)),
        );
        app.add_systems(
            OnEnter(SessionPhase::Results),
            (|rules: Res<SessionRules>, mut commands: Commands| {
                commands.insert_resource(PhaseTimer {
                    timer: Timer::new(rules.results, TimerMode::Once),
                    during: SessionPhase::Results,
                    then: Then::Phase(SessionPhase::Standings),
                });
            })
            .run_if(is_host),
        );
        app.add_systems(
            OnEnter(SessionPhase::NextMatch),
            start_next_match_timer.run_if(is_host),
        );
        app.add_systems(
            OnEnter(SessionPhase::Standings),
            start_standings_timer.run_if(is_host),
        );
        app.add_systems(
            Update,
            move_on_when_the_timer_ends
                .run_if(is_host)
                .run_if(resource_exists::<PhaseTimer>),
        );
    }
}

/// Command: the host starts a [`Minigame`] from the lobby, with the [`Participants`] chosen by
/// [`pick_participants`], or as a [`Tournament`] if it seats fewer than are here (see
/// [`needs_tournament`]). Refused unless hosting, in the lobby, with at least
/// [`Minigame::min_players`] players.
#[derive(Debug, Clone, Copy)]
pub struct StartMinigame(pub Minigame);

impl Command for StartMinigame {
    type Out = ();

    fn apply(self, world: &mut World) {
        if !world.run_system_cached(is_host).unwrap_or(false) {
            warn!("Only the host can start a minigame");
            return;
        }
        let in_lobby = world
            .query::<&CurrentPhase>()
            .single(world)
            .is_ok_and(|phase| phase.0 == SessionPhase::Lobby);
        if !in_lobby {
            warn!("Can't start {:?} outside the lobby", self.0);
            return;
        }
        BeginMinigame(self.0).apply(world);
    }
}

/// Host only: start a [`Minigame`] with everyone here, as the [`Participants`] chosen by
/// [`pick_participants`], or as a [`Tournament`] if it seats fewer than are here (see
/// [`needs_tournament`]). With fewer than [`Minigame::min_players`] players it isn't started; in
/// a [`Party`] that ends the party.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BeginMinigame(pub Minigame);

impl Command for BeginMinigame {
    type Out = ();

    fn apply(self, world: &mut World) {
        let BeginMinigame(game) = self;
        // In a party, the last round's tournament is still here: the session hasn't been back to
        // the lobby since.
        let old: Vec<Entity> = world
            .query_filtered::<Entity, With<Tournament>>()
            .iter(world)
            .collect();
        for tournament in old {
            world.despawn(tournament);
        }
        let in_party = world.query::<&Party>().iter(world).next().is_some();
        let standings: Vec<(Player, Points)> = world
            .query::<(&Player, Option<&Points>)>()
            .iter(world)
            .map(|(player, points)| (player.clone(), points.copied().unwrap_or_default()))
            .collect();
        let players: Vec<Player> = standings.iter().map(|(player, _)| player.clone()).collect();
        let Ok((mut phase, mut participants)) = world
            .query::<(&mut CurrentPhase, &mut Participants)>()
            .single_mut(world)
        else {
            return;
        };
        if players.len() < game.min_players() {
            if in_party {
                info!(
                    "Ending the party: {game:?} needs at least {} players, {} are left",
                    game.min_players(),
                    players.len()
                );
                phase.0 = SessionPhase::Lobby;
            } else {
                warn!(
                    "{game:?} needs at least {} players, have {}",
                    game.min_players(),
                    players.len()
                );
            }
            return;
        }
        if needs_tournament(game, players.len()) {
            let Some(tournament) = Tournament::new(game, seed(standings)) else {
                error!(
                    "Couldn't draw a {game:?} tournament for {} players",
                    players.len()
                );
                phase.0 = after_abandoning(in_party);
                return;
            };
            let Some((_, first)) = tournament.bracket.next_match() else {
                error!("A new {game:?} tournament has no match to play");
                phase.0 = after_abandoning(in_party);
                return;
            };
            info!(
                "Starting a {game:?} tournament for {} players: {}",
                players.len(),
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
            return;
        }
        participants.0 = pick_participants(game, &players);
        let names: Vec<&str> = participants
            .0
            .iter()
            .filter_map(|id| players.iter().find(|player| player.id == *id))
            .map(Player::name)
            .collect();
        info!(
            "Starting {game:?}: {} play, {} watch",
            names.join(", "),
            players.len() - names.len()
        );
        phase.0 = game.phase();
    }
}

/// Who plays `game` out of `players`: the lowest-numbered (earliest to join), up to
/// [`Minigame::max_players`].
pub fn pick_participants(game: Minigame, players: &[Player]) -> Vec<PeerId> {
    let mut players: Vec<&Player> = players.iter().collect();
    players.sort_by_key(|player| player.number);
    players
        .into_iter()
        .take(game.max_players())
        .map(|player| player.id)
        .collect()
}

/// Whether the host may start a minigame with `players` players.
pub fn can_start(players: usize) -> bool {
    players >= MIN_PLAYERS
}

fn spawn_current_phase(_trigger: On<Add<Started>>, mut commands: Commands) {
    commands.spawn((
        Name::from("CurrentPhase"),
        CurrentPhase(SessionPhase::Lobby),
        Participants::default(),
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

fn end_minigame(
    trigger: On<MinigameEnded>,
    session: Single<(&mut CurrentPhase, &mut Participants)>,
    tournament: Option<Single<&mut Tournament>>,
    party: Option<Single<&Party>>,
    mut players: Query<(&Player, &mut Points)>,
    mut commands: Commands,
) {
    let (mut phase, mut participants) = session.into_inner();
    if !phase.0.is_minigame() {
        return;
    }
    let abandon_to = after_abandoning(party.is_some());
    let present: Vec<PeerId> = players.iter().map(|(player, _)| player.id).collect();
    let tournament = tournament.map(Single::into_inner);
    match (trigger.event(), tournament) {
        (MinigameEnded::Won(result), _) if result.winners().next().is_none() => {
            error!("Minigame ended without a winner: {result:?}");
            phase.0 = abandon_to;
        }
        (MinigameEnded::Won(result), None) => {
            show_result(result, &mut phase, &mut players, &mut commands);
        }
        (MinigameEnded::Won(result), Some(mut tournament)) => {
            let step = tournament.record(result, &present);
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
        (MinigameEnded::Abandoned, None) => {
            info!("Minigame abandoned: going to {abandon_to:?}");
            phase.0 = abandon_to;
        }
        (MinigameEnded::Abandoned, Some(mut tournament)) => {
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
    }
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
    let in_play = phase.0 == SessionPhase::NextMatch || phase.0 == tournament.game.phase();
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

/// Host only: award `result`'s points and show it.
fn show_result(
    result: &MinigameResult,
    phase: &mut CurrentPhase,
    players: &mut Query<(&Player, &mut Points)>,
    commands: &mut Commands,
) {
    let order: Vec<String> = result.placements.iter().map(Placement::describe).collect();
    info!("Minigame over: {} ({})", order.join(", "), result.summary);
    award_points(result, players);
    commands.spawn((
        result.clone(),
        DespawnOnExit(SessionPhase::Results),
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
    ));
    phase.0 = SessionPhase::Results;
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
    commands.insert_resource(PhaseTimer {
        timer: Timer::new(rules.next_match, TimerMode::Once),
        during: SessionPhase::NextMatch,
        then: Then::Phase(tournament.game.phase()),
    });
}

/// Host only: show the standings for a while, then go back to the lobby, or in a [`Party`] on to
/// the next round or (after the last) the winner.
fn start_standings_timer(
    rules: Res<SessionRules>,
    party: Option<Single<&Party>>,
    mut commands: Commands,
) {
    let then = match party {
        None => SessionPhase::Lobby,
        Some(party) if party.is_over() => SessionPhase::PartyOver,
        Some(_) => SessionPhase::NextRound,
    };
    commands.insert_resource(PhaseTimer {
        timer: Timer::new(rules.standings, TimerMode::Once),
        during: SessionPhase::Standings,
        then: Then::Phase(then),
    });
}

/// Host only: when a screen between minigames has been up long enough, move on. A timer whose
/// phase is over (left early, or from a session that ended on that screen) is dropped.
fn move_on_when_the_timer_ends(
    time: Res<Time>,
    mut timer: ResMut<PhaseTimer>,
    mut phase: Single<&mut CurrentPhase>,
    mut commands: Commands,
) {
    if phase.0 != timer.during {
        commands.remove_resource::<PhaseTimer>();
        return;
    }
    if timer.timer.tick(time.delta()).is_finished() {
        match timer.then {
            Then::Phase(then) => phase.0 = then,
            Then::Minigame(game) => commands.queue(BeginMinigame(game)),
        }
        commands.remove_resource::<PhaseTimer>();
    }
}

/// Compares instead of waiting for [`CurrentPhase`] to change: a joiner can receive it before its
/// [`SessionPhase`] exists, and must still pick it up.
fn follow_phase(
    phase: Option<Single<&CurrentPhase>>,
    state: Option<Res<State<SessionPhase>>>,
    mut next: ResMut<NextState<SessionPhase>>,
) {
    let (Some(phase), Some(state)) = (phase, state) else {
        return;
    };
    if *state.get() != phase.0 {
        info!("Session phase {:?} -> {:?}", state.get(), phase.0);
        next.set(phase.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{Placement, SessionPhase, can_start, ordinal, pick_participants};
    use crate::minigames::Minigame;
    use crate::players::Player;
    use lightyear::prelude::PeerId;

    #[test]
    fn needs_two_players_to_start() {
        assert!(!can_start(0));
        assert!(!can_start(1));
        assert!(can_start(2));
        assert!(can_start(5));
    }

    #[test]
    fn the_earliest_players_play_up_to_the_seats() {
        let players: Vec<Player> = [3, 1, 4, 2]
            .into_iter()
            .map(|number| Player {
                id: PeerId::Netcode(number.into()),
                number,
                name: format!("Player {number}"),
                color: 0,
            })
            .collect();
        assert_eq!(
            pick_participants(Minigame::TwoPlayerPong, &players),
            [1, 2].map(PeerId::Netcode)
        );
        assert_eq!(
            pick_participants(Minigame::Racing, &players),
            [1, 2, 3, 4].map(PeerId::Netcode)
        );
    }

    #[test]
    fn describes_a_placement() {
        let mut placement = Placement {
            player: PeerId::Netcode(2),
            name: "Bo".into(),
            color: 0,
            place: 3,
            note: String::new(),
        };
        assert_eq!(placement.describe(), "3rd Bo");
        placement.note = "DNF".into();
        assert_eq!(placement.describe(), "3rd Bo (DNF)");
    }

    #[test]
    fn ordinals() {
        let shown: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111]
            .into_iter()
            .map(ordinal)
            .collect();
        assert_eq!(
            shown,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st",
                "111th"
            ]
        );
    }

    #[test]
    fn only_minigames_are_minigames() {
        assert!(SessionPhase::TwoPlayerPong.is_minigame());
        assert!(SessionPhase::Racing.is_minigame());
        assert!(!SessionPhase::Lobby.is_minigame());
        assert!(!SessionPhase::Results.is_minigame());
        assert!(!SessionPhase::Standings.is_minigame());
        assert!(!SessionPhase::NextMatch.is_minigame());
        assert!(!SessionPhase::NextRound.is_minigame());
        assert!(!SessionPhase::PartyOver.is_minigame());
    }
}
