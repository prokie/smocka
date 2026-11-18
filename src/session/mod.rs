//! What the session is doing right now (waiting in the lobby, or playing a minigame), decided by
//! the host and followed by every peer.
//!
//! [`SessionPhase`] is a sub-state of [`AppState::InSession`]: it only exists while this app is in
//! a session, and is removed (running `OnExit` and [`DespawnOnExit`]) when the app goes back to
//! the menu.
//!
//! Bevy [`States`] only exist inside one app, so they can't be replicated directly. Instead:
//!
//! 1. The host spawns one [`CurrentPhase`] entity and replicates it to every client.
//! 2. Only the host changes [`CurrentPhase`] (see `choose_phase`).
//! 3. Every peer, host included, copies [`CurrentPhase`] into its local [`SessionPhase`] state
//!    (see `follow_phase`).
//!
//! Gameplay code then uses ordinary Bevy state tools, the same way on every peer:
//! `OnEnter(SessionPhase::TwoPlayerPong)`, `run_if(in_state(...))` and `DespawnOnExit(...)`.

use bevy::prelude::*;
use lightyear::prelude::{server::Started, *};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::lobby::Player;
use crate::network::is_host;

/// What the session is doing. A local Bevy sub-state of [`AppState::InSession`] that follows the
/// host's [`CurrentPhase`].
#[derive(SubStates, Serialize, Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[source(AppState = AppState::InSession)]
pub enum SessionPhase {
    /// Waiting for enough players.
    #[default]
    Lobby,
    /// Playing two-player Pong.
    TwoPlayerPong,
}

/// The host's decision about the [`SessionPhase`], replicated to every client.
///
/// There is exactly one, spawned by the host when its server starts.
#[derive(Component, Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct CurrentPhase(pub SessionPhase);

/// Registers the session state and its replicated component.
///
/// Must be added after [`NetworkPlugin`](crate::network::NetworkPlugin) and
/// [`AppStatePlugin`](crate::app_state::AppStatePlugin), and before
/// [`MinigamesPlugin`](crate::minigames::MinigamesPlugin): it registers a replicated component,
/// its state is a sub-state of [`AppState`], and minigames use [`SessionPhase`].
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.add_sub_state::<SessionPhase>();
        app.component::<CurrentPhase>().replicate();

        app.add_observer(spawn_current_phase);
        app.add_systems(
            PreUpdate,
            (choose_phase.run_if(is_host), follow_phase)
                .chain()
                .run_if(in_state(AppState::InSession)),
        );
    }
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

/// Host only: pick the phase from the number of players.
///
/// For now: two or more players play Pong, fewer wait in the lobby. A hub or board will make
/// this choice later.
fn choose_phase(players: Query<(), With<Player>>, mut phase: Single<&mut CurrentPhase>) {
    let wanted = if players.iter().len() >= 2 {
        SessionPhase::TwoPlayerPong
    } else {
        SessionPhase::Lobby
    };
    phase.set_if_neq(CurrentPhase(wanted));
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
        next.set(phase.0);
    }
}
