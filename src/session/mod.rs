//! What the session is doing right now (waiting in the lobby, playing a minigame, or showing its
//! results), decided by the host and followed by every peer.
//!
//! [`SessionPhase`] is a sub-state of [`AppState::InSession`]: it only exists while this app is in
//! a session, and is removed (running `OnExit` and [`DespawnOnExit`]) when the app goes back to
//! the menu.
//!
//! Bevy [`States`] only exist inside one app, so they can't be replicated directly. Instead:
//!
//! 1. The host spawns one [`CurrentPhase`] entity and replicates it to every client.
//! 2. Only the host changes [`CurrentPhase`]: [`StartMinigame`] leaves the lobby, a minigame
//!    triggers [`MinigameEnded`] when it is over, and the results return to the lobby by
//!    themselves after [`RESULTS_DURATION`].
//! 3. Every peer, host included, copies [`CurrentPhase`] into its local [`SessionPhase`] state
//!    (see `follow_phase`).
//!
//! Gameplay code then uses ordinary Bevy state tools, the same way on every peer:
//! `OnEnter(SessionPhase::TwoPlayerPong)`, `run_if(in_state(...))` and `DespawnOnExit(...)`.

use bevy::prelude::*;
use lightyear::prelude::{server::Started, *};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::config::session::{MIN_PLAYERS, RESULTS_DURATION};
use crate::lobby::Player;
use crate::minigames::Minigame;
use crate::network::is_host;

/// What the session is doing. A local Bevy sub-state of [`AppState::InSession`] that follows the
/// host's [`CurrentPhase`].
#[derive(SubStates, Serialize, Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[source(AppState = AppState::InSession)]
pub enum SessionPhase {
    /// Players gather until the host starts a minigame.
    #[default]
    Lobby,
    /// Playing two-player Pong.
    TwoPlayerPong,
    /// Playing the racing minigame.
    Racing,
    /// Showing who won the last minigame, before going back to the lobby.
    Results,
}

impl SessionPhase {
    /// Whether this phase is a minigame being played.
    pub fn is_minigame(self) -> bool {
        match self {
            SessionPhase::TwoPlayerPong | SessionPhase::Racing => true,
            SessionPhase::Lobby | SessionPhase::Results => false,
        }
    }
}

/// The host's decision about the [`SessionPhase`], replicated to every client.
///
/// There is exactly one, spawned by the host when its server starts.
#[derive(Component, Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct CurrentPhase(pub SessionPhase);

/// Who won the last minigame, shown during [`SessionPhase::Results`]. Spawned by the host and
/// replicated to everyone; removed when the results end.
#[derive(Component, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MinigameResult {
    /// The winning [`Player`]'s id.
    pub winner: PeerId,
    /// A short summary of how the game ended, such as the final score.
    pub summary: String,
}

/// Host only: trigger this (`commands.trigger(...)`) when the current minigame is over.
///
/// This is how a minigame hands control back to the session. Triggering it outside a minigame,
/// or more than once for the same game, does nothing.
#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub enum MinigameEnded {
    /// Someone won: show the result, then go back to the lobby.
    Won(MinigameResult),
    /// The game can't go on (for example, a player left): go straight back to the lobby.
    Abandoned,
}

/// Host only: counts down the results before going back to the lobby.
#[derive(Resource, Debug)]
struct ResultsTimer(Timer);

/// Registers the session state, its replicated components, and the host's phase changes.
///
/// Must be added after [`NetworkPlugin`](crate::network::NetworkPlugin) and
/// [`AppStatePlugin`](crate::app_state::AppStatePlugin), and before
/// [`MinigamesPlugin`](crate::minigames::MinigamesPlugin): it registers replicated components,
/// its state is a sub-state of [`AppState`], and minigames use [`SessionPhase`].
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_sub_state::<SessionPhase>();
        app.component::<CurrentPhase>().replicate();
        app.component::<MinigameResult>().replicate();

        app.add_observer(spawn_current_phase);
        app.add_observer(end_minigame);
        app.add_systems(
            PreUpdate,
            follow_phase.run_if(in_state(AppState::InSession)),
        );
        app.add_systems(
            OnEnter(SessionPhase::Results),
            start_results_timer.run_if(is_host),
        );
        app.add_systems(
            Update,
            return_to_lobby_after_results
                .run_if(is_host)
                .run_if(in_state(SessionPhase::Results)),
        );
    }
}

/// Command: the host starts the given [`Minigame`] from the lobby.
///
/// Does nothing (apart from a log line) unless this app is hosting, the session is in the lobby,
/// and at least [`MIN_PLAYERS`] players are connected. For now the host picks the minigame in the
/// lobby; a board or vote will pick it later.
#[derive(Debug, Clone, Copy)]
pub struct StartMinigame(pub Minigame);

impl Command for StartMinigame {
    type Out = ();

    fn apply(self, world: &mut World) {
        if !world.run_system_cached(is_host).unwrap_or(false) {
            warn!("Only the host can start a minigame");
            return;
        }
        let players = world
            .query_filtered::<(), With<Player>>()
            .iter(world)
            .count();
        let Ok(mut phase) = world.query::<&mut CurrentPhase>().single_mut(world) else {
            return;
        };
        if phase.0 != SessionPhase::Lobby {
            warn!("Can't start a minigame outside the lobby");
            return;
        }
        if !can_start(players) {
            warn!("Need at least {MIN_PLAYERS} players to start, have {players}");
            return;
        }
        let StartMinigame(game) = self;
        phase.0 = game.phase();
        info!("Starting {game:?} with {players} players");
    }
}

/// Whether the host may start a minigame with `players` players.
pub fn can_start(players: usize) -> bool {
    players >= MIN_PLAYERS
}

/// Host only: when the server starts, spawn the [`CurrentPhase`] entity that clients will follow.
/// It is removed when the host goes back to the menu.
fn spawn_current_phase(_trigger: On<Add<Started>>, mut commands: Commands) {
    commands.spawn((
        Name::from("CurrentPhase"),
        CurrentPhase(SessionPhase::Lobby),
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

/// Host only: when a minigame ends, show its result or go straight back to the lobby.
fn end_minigame(
    trigger: On<MinigameEnded>,
    mut phase: Single<&mut CurrentPhase>,
    mut commands: Commands,
) {
    if !phase.0.is_minigame() {
        return;
    }
    match trigger.event() {
        MinigameEnded::Won(result) => {
            info!("Minigame won by {:?} ({})", result.winner, result.summary);
            commands.spawn((
                result.clone(),
                DespawnOnExit(SessionPhase::Results),
                DespawnOnEnter(AppState::MainMenu),
                Replicate::to_clients(NetworkTarget::All),
            ));
            phase.0 = SessionPhase::Results;
        }
        MinigameEnded::Abandoned => {
            info!("Minigame abandoned");
            phase.0 = SessionPhase::Lobby;
        }
    }
}

/// Host only: when the results appear, start counting down to the lobby.
fn start_results_timer(mut commands: Commands) {
    commands.insert_resource(ResultsTimer(Timer::new(RESULTS_DURATION, TimerMode::Once)));
}

/// Host only: go back to the lobby once the results have been up for [`RESULTS_DURATION`].
fn return_to_lobby_after_results(
    time: Res<Time>,
    timer: Option<ResMut<ResultsTimer>>,
    mut phase: Single<&mut CurrentPhase>,
) {
    let Some(mut timer) = timer else {
        return;
    };
    if timer.0.tick(time.delta()).is_finished() {
        phase.0 = SessionPhase::Lobby;
    }
}

/// Every peer: if the local [`SessionPhase`] differs from the host's [`CurrentPhase`], switch to it.
///
/// This compares instead of waiting for [`CurrentPhase`] to change: a joiner can receive it before
/// its [`SessionPhase`] exists (while still [`AppState::Connecting`]), and must still pick it up.
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
    use super::{SessionPhase, can_start};

    #[test]
    fn needs_two_players_to_start() {
        assert!(!can_start(0));
        assert!(!can_start(1));
        assert!(can_start(2));
        assert!(can_start(5));
    }

    #[test]
    fn only_minigames_are_minigames() {
        assert!(SessionPhase::TwoPlayerPong.is_minigame());
        assert!(SessionPhase::Racing.is_minigame());
        assert!(!SessionPhase::Lobby.is_minigame());
        assert!(!SessionPhase::Results.is_minigame());
    }
}
