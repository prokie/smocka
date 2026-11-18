//! What the session is doing right now (waiting in the lobby, or playing a minigame), decided by
//! the host and followed by every peer.
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

use crate::lobby::Player;
use crate::network::is_host;

/// What the session is doing. A local Bevy state that follows the host's [`CurrentPhase`].
#[derive(States, Serialize, Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
/// Must be added after [`NetworkPlugin`](crate::network::NetworkPlugin) and before
/// [`MinigamesPlugin`](crate::minigames::MinigamesPlugin): it registers a replicated component, and
/// minigames use [`SessionPhase`].
pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<SessionPhase>();
        app.component::<CurrentPhase>().replicate();

        app.add_observer(spawn_current_phase);
        app.add_observer(return_to_lobby_when_disconnected);
        app.add_systems(
            PreUpdate,
            (choose_phase.run_if(is_host), follow_phase).chain(),
        );
    }
}

/// Host only: when the server starts, spawn the [`CurrentPhase`] entity that clients will follow.
fn spawn_current_phase(_trigger: On<Add<Started>>, mut commands: Commands) {
    commands.spawn((
        Name::from("CurrentPhase"),
        CurrentPhase(SessionPhase::Lobby),
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

/// Every peer: when [`CurrentPhase`] arrives or changes, switch the local [`SessionPhase`] to match.
fn follow_phase(
    phase: Option<Single<&CurrentPhase, Changed<CurrentPhase>>>,
    mut next: ResMut<NextState<SessionPhase>>,
) {
    if let Some(phase) = phase {
        next.set(phase.0);
    }
}

/// Every peer: if the [`CurrentPhase`] entity goes away (for example, a client loses its
/// connection to the host), go back to the lobby.
fn return_to_lobby_when_disconnected(
    _trigger: On<Remove<CurrentPhase>>,
    mut next: ResMut<NextState<SessionPhase>>,
) {
    next.set(SessionPhase::Lobby);
}
