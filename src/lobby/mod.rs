//! Who is in the session: one replicated [`Player`] per connected person, independent of any minigame.

use bevy::prelude::*;
use lightyear::prelude::{server::ClientOf, *};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;

/// A person in the session. Spawned by the host for every connected client (including its own)
/// and replicated to everyone.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Player {
    /// The player's network id.
    pub id: PeerId,
    /// Shown to people as "Player {number}". Given out in joining order, starting at 1 for the
    /// host, and not reused in the same session.
    pub number: u32,
}

impl Player {
    /// The name shown for this player.
    pub fn name(&self) -> String {
        format!("Player {}", self.number)
    }

    /// Whether this is the hosting player. The host's own client is linked to its server in the
    /// same process, so it has a local id rather than a netcode one.
    pub fn is_host(&self) -> bool {
        matches!(self.id, PeerId::Local(_))
    }
}

/// Host only: the [`Player::number`] the next player to join gets.
#[derive(Resource, Debug)]
struct NextPlayerNumber(u32);

impl Default for NextPlayerNumber {
    fn default() -> Self {
        Self(1)
    }
}

/// Must be added after [`NetworkPlugin`](crate::network::NetworkPlugin): it registers replicated components.
pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Player>().replicate();
        app.init_resource::<NextPlayerNumber>();
        app.add_observer(spawn_player);
        app.add_systems(OnEnter(AppState::MainMenu), reset_player_numbers);
    }
}

/// This app's own [`PeerId`] in the session it is connected to, if any. Compare it with
/// [`Player::id`] to find "you".
pub fn local_peer_id(clients: &Query<&LocalId, (With<Client>, With<Connected>)>) -> Option<PeerId> {
    clients.iter().next().map(|id| id.0)
}

/// When a client connects, the host spawns its [`Player`].
///
/// `ClientOf` limits this to the host's side of each connection. `ControlledBy` with the default
/// session-based lifetime despawns the player again when that client disconnects.
fn spawn_player(
    trigger: On<Add<Connected>>,
    clients: Query<&RemoteId, With<ClientOf>>,
    mut next_number: ResMut<NextPlayerNumber>,
    mut commands: Commands,
) {
    let Ok(remote_id) = clients.get(trigger.entity) else {
        return;
    };
    let player = Player {
        id: remote_id.0,
        number: next_number.0,
    };
    next_number.0 += 1;
    commands.spawn((
        player,
        // Usually removed when its client disconnects; this also covers the host leaving.
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
        ControlledBy {
            owner: trigger.entity,
            lifetime: Lifetime::default(),
        },
    ));
    info!("{} ({:?}) joined", player.name(), remote_id.0);
}

/// Number the players of the next hosted session from 1 again.
fn reset_player_numbers(mut next_number: ResMut<NextPlayerNumber>) {
    *next_number = NextPlayerNumber::default();
}
