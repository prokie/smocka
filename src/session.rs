use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{server::Started, *};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::config::session::{MIN_PLAYERS, RESULTS_DURATION, STANDINGS_DURATION};
use crate::minigames::Minigame;
use crate::minigames::outcome::{MinigameEnded, MinigameResult, Participants, Placement};
use crate::network::is_host;
use crate::party::Party;
use crate::players::Player;
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
}

/// The host's decision about the [`SessionPhase`], replicated to every client.
///
/// Bevy states can't be replicated, so only the host changes this, and every peer (host included)
/// copies it into its local [`SessionPhase`].
#[derive(Component, Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct CurrentPhase(pub SessionPhase);

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
        app.component::<MinigameResult>().replicate();

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
            move_on_when_the_timer_ends
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
            .query::<(&Player, Option<&Points>)>()
            .iter(world)
            .map(|(player, points)| (player.clone(), points.copied().unwrap_or_default()))
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
    use super::{SessionPhase, can_start, pick_participants};
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
