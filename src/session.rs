use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{
    server::{ClientOf, Started},
    *,
};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::config::session::{MIN_PLAYERS, RESULTS_DURATION, STANDINGS_DURATION};
use crate::minigames::Minigame;
use crate::minigames::outcome::{MinigameEnded, MinigameResult, Participants, Placement};
use crate::network::is_host;
use crate::party::Party;
use crate::players::{Player, local_peer_id};
use crate::standings::{Points, award_points};
use crate::tournament::{Tournament, needs_tournament, start_tournament};

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
    /// Playing a minigame. Each minigame's module has its own as `PHASE`, for example
    /// [`racing::PHASE`](crate::minigames::racing::PHASE).
    Playing(Minigame),
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
        matches!(self, SessionPhase::Playing(_))
    }

    /// Whether this phase is a timed waiting screen between minigames that players can vote to
    /// skip.
    pub fn is_waiting(self) -> bool {
        matches!(
            self,
            SessionPhase::Results
                | SessionPhase::Standings
                | SessionPhase::NextMatch
                | SessionPhase::NextRound
                | SessionPhase::PartyOver
        )
    }
}

/// The host's decision about the [`SessionPhase`], replicated to every client.
///
/// Bevy states can't be replicated, so only the host changes this, and every peer (host included)
/// copies it into its local [`SessionPhase`].
#[derive(Component, Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct CurrentPhase(pub SessionPhase);

/// Who has voted to skip the current waiting screen. Kept next to [`CurrentPhase`] by the host
/// and replicated so every peer can show how many players are ready. Reset whenever a new waiting
/// countdown starts (see [`MoveOnAfter`]).
#[derive(Component, Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct SkipVotes {
    /// Players who have voted to skip this waiting screen.
    pub voters: Vec<PeerId>,
    /// Incremented each time a new waiting countdown starts, so an in-flight [`SkipRequest`] from
    /// the previous screen is ignored.
    pub generation: u32,
}

impl SkipVotes {
    /// Whether `player` has voted to skip the current waiting screen.
    pub fn contains(&self, player: PeerId) -> bool {
        self.voters.contains(&player)
    }
}

/// Client to host: "I'm ready to skip waiting screen `.0` (its [`SkipVotes::generation`])".
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkipRequest(pub u32);

/// The channel session-level client requests (such as [`SkipRequest`]) travel on.
pub struct SessionChannel;

/// Command: this player votes to skip the current waiting screen. The host records its own vote
/// straight away; a joiner sends a [`SkipRequest`] to the host.
#[derive(Debug, Clone, Copy)]
pub struct VoteSkip;

impl Command for VoteSkip {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Some(generation) = world
            .query::<&SkipVotes>()
            .iter(world)
            .next()
            .map(|votes| votes.generation)
        else {
            return;
        };
        if world.run_system_cached(is_host).unwrap_or(false) {
            let host = world
                .query_filtered::<&LocalId, (With<Client>, With<Connected>)>()
                .iter(world)
                .next()
                .map(|id| id.0);
            if let Some(host) = host {
                record_skip_vote_in_world(world, host, generation);
            }
            return;
        }
        let mut senders = world
            .query_filtered::<&mut MessageSender<SkipRequest>, (With<Client>, With<Connected>)>();
        if let Some(mut sender) = senders.iter_mut(world).next() {
            debug!("Voting to skip waiting screen (generation {generation})");
            sender.send::<SessionChannel>(SkipRequest(generation));
        }
    }
}

/// Whether every present player (at least one) is in `voters`.
pub fn everyone_agreed(players: impl IntoIterator<Item = PeerId>, voters: &[PeerId]) -> bool {
    let mut count = 0;
    for id in players {
        count += 1;
        if !voters.contains(&id) {
            return false;
        }
    }
    count > 0
}

fn record_skip_vote_in_world(world: &mut World, voter: PeerId, generation: u32) {
    let Some(during) = world.get_resource::<PhaseTimer>().map(|timer| timer.during) else {
        return;
    };
    let in_phase = world
        .query::<&CurrentPhase>()
        .single(world)
        .is_ok_and(|phase| phase.0 == during);
    if !in_phase {
        return;
    }
    let player_count = world.query::<&Player>().iter(world).len();
    let is_present = world
        .query::<&Player>()
        .iter(world)
        .any(|player| player.id == voter);
    if !is_present {
        return;
    }
    let Ok(mut votes) = world.query::<&mut SkipVotes>().single_mut(world) else {
        return;
    };
    record_skip_vote(&mut votes, voter, generation, during, player_count);
}

fn record_skip_vote(
    votes: &mut SkipVotes,
    voter: PeerId,
    generation: u32,
    during: SessionPhase,
    player_count: usize,
) {
    if votes.generation != generation || votes.voters.contains(&voter) {
        return;
    }
    votes.voters.push(voter);
    debug!(
        "{voter:?} voted to skip {during:?} ({}/{player_count})",
        votes.voters.len()
    );
}

/// Command, host only: keep the session on the `during` screen for `wait`, then do `then`. It
/// replaces a countdown already running, and is dropped if the session leaves `during` first (a
/// walkover can end a tournament mid-countdown, say).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveOnAfter {
    pub during: SessionPhase,
    pub wait: Duration,
    pub then: Then,
}

impl Command for MoveOnAfter {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.insert_resource(PhaseTimer {
            timer: Timer::new(self.wait, TimerMode::Once),
            during: self.during,
            then: self.then,
        });
        if let Ok(mut votes) = world.query::<&mut SkipVotes>().single_mut(world) {
            votes.generation = votes.generation.wrapping_add(1);
            votes.voters.clear();
        }
    }
}

/// What the session does when a [`MoveOnAfter`] runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// Move on to this phase.
    Phase(SessionPhase),
    /// Start this minigame with everyone here, as a tournament if it seats fewer than are here.
    Minigame(Minigame),
}

/// Host only: the countdown a [`MoveOnAfter`] started.
#[derive(Resource, Debug)]
struct PhaseTimer {
    timer: Timer,
    during: SessionPhase,
    then: Then,
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
pub(crate) fn after_abandoning(in_party: bool) -> SessionPhase {
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
        app.component::<SkipVotes>().replicate();
        app.component::<MinigameResult>().replicate();
        app.add_channel::<SessionChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<SkipRequest>()
            .add_direction(NetworkDirection::ClientToServer);

        app.add_observer(spawn_current_phase);
        app.add_observer(end_minigame);
        app.add_systems(
            PreUpdate,
            follow_phase.run_if(in_state(AppState::InSession)),
        );
        app.add_systems(
            OnEnter(SessionPhase::Results),
            (|rules: Res<SessionRules>, mut commands: Commands| {
                commands.queue(MoveOnAfter {
                    during: SessionPhase::Results,
                    wait: rules.results,
                    then: Then::Phase(SessionPhase::Standings),
                });
            })
            .run_if(is_host),
        );
        app.add_systems(
            OnEnter(SessionPhase::Standings),
            start_standings_timer.run_if(is_host),
        );
        app.add_systems(
            Update,
            (apply_skip_requests, move_on_when_the_timer_ends)
                .chain()
                .run_if(is_host)
                .run_if(resource_exists::<PhaseTimer>),
        );
    }
}

/// Command: the host starts a [`Minigame`] from the lobby, with the [`Participants`] chosen by
/// [`pick_participants`], or as a [`Tournament`] if it seats fewer than are here (see
/// [`needs_tournament`]). Refused unless hosting, in the lobby, with at least
/// [`min_players`](crate::minigames::MinigameInfo::min_players) players.
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

/// Command: the host takes everyone back to the lobby from wherever the session is. A minigame
/// being played is abandoned, with no points for anyone; a [`Party`] or [`Tournament`] ends (they
/// go when the lobby is entered). Points already won stay. Refused unless hosting, and outside the
/// lobby.
#[derive(Debug, Clone, Copy)]
pub struct ReturnToLobby;

impl Command for ReturnToLobby {
    type Out = ();

    fn apply(self, world: &mut World) {
        if !world.run_system_cached(is_host).unwrap_or(false) {
            warn!("Only the host can take everyone back to the lobby");
            return;
        }
        let Ok(mut phase) = world.query::<&mut CurrentPhase>().single_mut(world) else {
            error!("Can't go back to the lobby: the session has no phase");
            return;
        };
        if phase.0 == SessionPhase::Lobby {
            warn!("Already in the lobby");
            return;
        }
        info!(
            "The host took everyone back to the lobby from {:?}",
            phase.0
        );
        phase.0 = SessionPhase::Lobby;
    }
}

/// Host only: start a [`Minigame`] with everyone here, as the [`Participants`] chosen by
/// [`pick_participants`], or as a [`Tournament`] if it seats fewer than are here (see
/// [`needs_tournament`]). With fewer than
/// [`min_players`](crate::minigames::MinigameInfo::min_players) players it isn't started; in a
/// [`Party`] that ends the party.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BeginMinigame(pub Minigame);

impl Command for BeginMinigame {
    type Out = ();

    fn apply(self, world: &mut World) {
        let BeginMinigame(game) = self;
        let in_party = world.query::<&Party>().iter(world).next().is_some();
        let standings: Vec<(Player, Points)> = world
            .query::<(&Player, &Points)>()
            .iter(world)
            .map(|(player, points)| (player.clone(), *points))
            .collect();
        let count = standings.len();
        if count >= game.info().min_players && needs_tournament(game, count) {
            start_tournament(world, game, standings, after_abandoning(in_party));
            return;
        }
        let Ok((mut phase, mut participants)) = world
            .query::<(&mut CurrentPhase, &mut Participants)>()
            .single_mut(world)
        else {
            return;
        };
        if count < game.info().min_players {
            if in_party {
                info!(
                    "Ending the party: {game:?} needs at least {} players, {count} are left",
                    game.info().min_players,
                );
                phase.0 = SessionPhase::Lobby;
            } else {
                warn!(
                    "{game:?} needs at least {} players, have {count}",
                    game.info().min_players,
                );
            }
            return;
        }
        let players: Vec<Player> = standings.into_iter().map(|(player, _)| player).collect();
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
            count - names.len()
        );
        phase.0 = SessionPhase::Playing(game);
    }
}

/// Who plays `game` out of `players`: the lowest-numbered (earliest to join), up to
/// [`max_players`](crate::minigames::MinigameInfo::max_players).
pub fn pick_participants(game: Minigame, players: &[Player]) -> Vec<PeerId> {
    let mut players: Vec<&Player> = players.iter().collect();
    players.sort_by_key(|player| player.number);
    players
        .into_iter()
        .take(game.info().max_players)
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
        SkipVotes::default(),
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

/// Host only: show a finished minigame's result, or go back after an abandoned one. While a
/// tournament is running, it decides instead (see [`Tournament`]).
fn end_minigame(
    trigger: On<MinigameEnded>,
    mut phase: Single<&mut CurrentPhase>,
    tournament: Query<(), With<Tournament>>,
    party: Option<Single<&Party>>,
    mut players: Query<(&Player, &mut Points)>,
    mut commands: Commands,
) {
    if !phase.0.is_minigame() || !tournament.is_empty() {
        return;
    }
    let abandon_to = after_abandoning(party.is_some());
    match trigger.event() {
        MinigameEnded::Won(result) if result.winners().next().is_none() => {
            error!("Minigame ended without a winner: {result:?}");
            phase.0 = abandon_to;
        }
        MinigameEnded::Won(result) => {
            show_result(result, &mut phase, &mut players, &mut commands);
        }
        MinigameEnded::Abandoned => {
            info!("Minigame abandoned: going to {abandon_to:?}");
            phase.0 = abandon_to;
        }
    }
}

/// Host only: award `result`'s points and show it.
pub(crate) fn show_result(
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
    commands.queue(MoveOnAfter {
        during: SessionPhase::Standings,
        wait: rules.standings,
        then: Then::Phase(then),
    });
}

/// Host only: record skip votes sent by remote clients for the current waiting countdown.
fn apply_skip_requests(
    timer: Res<PhaseTimer>,
    phase: Single<&CurrentPhase>,
    mut votes: Single<&mut SkipVotes>,
    players: Query<&Player>,
    mut links: Query<(&RemoteId, &mut MessageReceiver<SkipRequest>), With<ClientOf>>,
) {
    if phase.0 != timer.during {
        return;
    }
    let present: Vec<PeerId> = players.iter().map(|player| player.id).collect();
    for (remote_id, mut receiver) in &mut links {
        for SkipRequest(generation) in receiver.receive() {
            if present.contains(&remote_id.0) {
                record_skip_vote(
                    &mut votes,
                    remote_id.0,
                    generation,
                    timer.during,
                    present.len(),
                );
            }
        }
    }
}

/// Host only: when a screen between minigames has been up long enough — or every player in the
/// session has voted to skip it — move on. A timer whose phase is over (left early, or from a
/// session that ended on that screen) is dropped.
fn move_on_when_the_timer_ends(
    time: Res<Time>,
    mut timer: ResMut<PhaseTimer>,
    mut phase: Single<(&mut CurrentPhase, &mut SkipVotes)>,
    players: Query<&Player>,
    mut commands: Commands,
) {
    let (ref mut phase, ref mut votes) = *phase;
    if phase.0 != timer.during {
        votes.voters.clear();
        commands.remove_resource::<PhaseTimer>();
        return;
    }
    let present: Vec<PeerId> = players.iter().map(|player| player.id).collect();
    if votes.voters.iter().any(|id| !present.contains(id)) {
        votes.voters.retain(|id| present.contains(id));
    }
    let skipped = everyone_agreed(present.iter().copied(), &votes.voters);
    let timed_out = timer.timer.tick(time.delta()).is_finished();
    if timed_out || skipped {
        if skipped && !timed_out {
            info!(
                "Everyone agreed to skip {:?} ({} players)",
                timer.during,
                present.len()
            );
        }
        votes.voters.clear();
        match timer.then {
            Then::Phase(then) => phase.0 = then,
            Then::Minigame(game) => commands.queue(BeginMinigame(game)),
        }
        commands.remove_resource::<PhaseTimer>();
    }
}

/// Whether the local player has already voted to skip the current waiting screen.
pub fn local_player_voted(
    votes: &SkipVotes,
    clients: &Query<&LocalId, (With<Client>, With<Connected>)>,
) -> bool {
    local_peer_id(clients).is_some_and(|you| votes.contains(you))
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
    use super::{
        SessionPhase, SkipVotes, can_start, everyone_agreed, pick_participants, record_skip_vote,
    };
    use crate::minigames::Minigame;
    use crate::players::Player;
    use lightyear::prelude::PeerId;

    #[test]
    fn everyone_must_agree_to_skip() {
        let (a, b, c) = (PeerId::Netcode(1), PeerId::Netcode(2), PeerId::Netcode(3));
        assert!(!everyone_agreed([], &[]));
        assert!(!everyone_agreed([a, b], &[a]));
        assert!(everyone_agreed([a, b], &[a, b]));
        assert!(everyone_agreed([a, b], &[b, a, c]));
        assert!(!everyone_agreed([a, b, c], &[a, b]));
    }

    #[test]
    fn stale_generation_and_duplicate_votes_are_ignored() {
        let (a, b) = (PeerId::Netcode(1), PeerId::Netcode(2));
        let mut votes = SkipVotes {
            voters: Vec::new(),
            generation: 3,
        };
        record_skip_vote(&mut votes, a, 2, SessionPhase::Results, 2);
        assert!(votes.voters.is_empty());

        record_skip_vote(&mut votes, a, 3, SessionPhase::Results, 2);
        record_skip_vote(&mut votes, a, 3, SessionPhase::Results, 2);
        record_skip_vote(&mut votes, b, 3, SessionPhase::Results, 2);
        assert_eq!(votes.voters, [a, b]);
    }

    #[test]
    fn waiting_phases_are_the_timed_screens_between_minigames() {
        assert!(!SessionPhase::Lobby.is_waiting());
        assert!(!SessionPhase::Playing(Minigame::Shootout).is_waiting());
        assert!(SessionPhase::Results.is_waiting());
        assert!(SessionPhase::Standings.is_waiting());
        assert!(SessionPhase::NextMatch.is_waiting());
        assert!(SessionPhase::NextRound.is_waiting());
        assert!(SessionPhase::PartyOver.is_waiting());
    }

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
    fn only_minigames_are_minigames() {
        for game in Minigame::ALL {
            assert!(SessionPhase::Playing(game).is_minigame(), "{game:?}");
        }
        assert!(!SessionPhase::Lobby.is_minigame());
        assert!(!SessionPhase::Results.is_minigame());
        assert!(!SessionPhase::Standings.is_minigame());
        assert!(!SessionPhase::NextMatch.is_minigame());
        assert!(!SessionPhase::NextRound.is_minigame());
        assert!(!SessionPhase::PartyOver.is_minigame());
    }
}
