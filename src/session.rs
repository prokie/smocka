use bevy::prelude::*;
use lightyear::prelude::{server::Started, *};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::config::session::{MIN_PLAYERS, RESULTS_DURATION};
use crate::minigames::Minigame;
use crate::network::is_host;
use crate::players::Player;

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
/// Bevy states can't be replicated, so only the host changes this, and every peer (host included)
/// copies it into its local [`SessionPhase`].
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

/// Host only: trigger this when the current minigame is over. Triggering it outside a minigame,
/// or more than once, does nothing.
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

/// Must be added after [`AppStatePlugin`](crate::app_state::AppStatePlugin) and before
/// [`MinigamesPlugin`](crate::minigames::MinigamesPlugin).
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

/// Command: the host starts a [`Minigame`] from the lobby. Refused unless hosting, in the lobby,
/// with at least [`MIN_PLAYERS`] players.
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

fn spawn_current_phase(_trigger: On<Add<Started>>, mut commands: Commands) {
    commands.spawn((
        Name::from("CurrentPhase"),
        CurrentPhase(SessionPhase::Lobby),
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

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

fn start_results_timer(mut commands: Commands) {
    commands.insert_resource(ResultsTimer(Timer::new(RESULTS_DURATION, TimerMode::Once)));
}

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
