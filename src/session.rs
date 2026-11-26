use bevy::prelude::*;
use lightyear::prelude::{server::Started, *};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::config::session::{MIN_PLAYERS, RESULTS_DURATION, STANDINGS_DURATION};
use crate::minigames::Minigame;
use crate::network::is_host;
use crate::players::Player;
use crate::standings::{Points, award_points};

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
    /// Showing how everyone placed in the last minigame.
    Results,
    /// Showing everyone's points so far, before going back to the lobby.
    Standings,
}

impl SessionPhase {
    /// Whether this phase is a minigame being played.
    pub fn is_minigame(self) -> bool {
        match self {
            SessionPhase::TwoPlayerPong | SessionPhase::Racing => true,
            SessionPhase::Lobby | SessionPhase::Results | SessionPhase::Standings => false,
        }
    }
}

/// The host's decision about the [`SessionPhase`], replicated to every client.
///
/// Bevy states can't be replicated, so only the host changes this, and every peer (host included)
/// copies it into its local [`SessionPhase`].
#[derive(Component, Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct CurrentPhase(pub SessionPhase);

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
    /// The game was played to the end: show the result, then go back to the lobby.
    Won(MinigameResult),
    /// The game can't go on (for example, a player left): go straight back to the lobby.
    Abandoned,
}

/// Host only: how long the current screen between minigames has left, and what comes after it.
#[derive(Resource, Debug)]
struct PhaseTimer {
    timer: Timer,
    then: SessionPhase,
}

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
            (|mut commands: Commands| {
                commands.insert_resource(PhaseTimer {
                    timer: Timer::new(RESULTS_DURATION, TimerMode::Once),
                    then: SessionPhase::Standings,
                });
            })
            .run_if(is_host),
        );
        app.add_systems(
            OnEnter(SessionPhase::Standings),
            (|mut commands: Commands| {
                commands.insert_resource(PhaseTimer {
                    timer: Timer::new(STANDINGS_DURATION, TimerMode::Once),
                    then: SessionPhase::Lobby,
                });
            })
            .run_if(is_host),
        );
        app.add_systems(
            Update,
            move_on_when_the_timer_ends
                .run_if(is_host)
                .run_if(resource_exists::<PhaseTimer>),
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
    mut players: Query<(&Player, &mut Points)>,
    mut commands: Commands,
) {
    if !phase.0.is_minigame() {
        return;
    }
    match trigger.event() {
        MinigameEnded::Won(result) if result.winners().next().is_none() => {
            error!("Minigame ended without a winner: {result:?}");
            phase.0 = SessionPhase::Lobby;
        }
        MinigameEnded::Won(result) => {
            let order: Vec<String> = result.placements.iter().map(Placement::describe).collect();
            info!("Minigame over: {} ({})", order.join(", "), result.summary);
            award_points(result, &mut players);
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

/// Host only: when the results or standings have been up long enough, move on. A timer left over
/// from a session that ended on one of those screens is dropped.
fn move_on_when_the_timer_ends(
    time: Res<Time>,
    mut timer: ResMut<PhaseTimer>,
    mut phase: Single<&mut CurrentPhase>,
    mut commands: Commands,
) {
    if !matches!(phase.0, SessionPhase::Results | SessionPhase::Standings) {
        commands.remove_resource::<PhaseTimer>();
        return;
    }
    if timer.timer.tick(time.delta()).is_finished() {
        phase.0 = timer.then;
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
    use super::{Placement, SessionPhase, can_start, ordinal};
    use lightyear::prelude::PeerId;

    #[test]
    fn needs_two_players_to_start() {
        assert!(!can_start(0));
        assert!(!can_start(1));
        assert!(can_start(2));
        assert!(can_start(5));
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
    }
}
