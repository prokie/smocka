//! Who is in the session: one replicated [`Player`] per connected person, independent of any minigame.

use bevy::prelude::*;
use lightyear::prelude::{server::ClientOf, *};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;

/// A person in the session. Spawned by the host for every connected client (including its own)
/// and replicated to everyone.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub id: PeerId,
}

/// Must be added after [`NetworkPlugin`](crate::network::NetworkPlugin): it registers replicated components.
pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Player>().replicate();
        app.add_observer(spawn_player);
    }
}

/// When a client connects, the host spawns its [`Player`].
///
/// `ClientOf` limits this to the host's side of each connection. `ControlledBy` with the default
/// session-based lifetime despawns the player again when that client disconnects.
fn spawn_player(
    trigger: On<Add<Connected>>,
    clients: Query<&RemoteId, With<ClientOf>>,
    mut commands: Commands,
) {
    let Ok(remote_id) = clients.get(trigger.entity) else {
        return;
    };
    commands.spawn((
        Player { id: remote_id.0 },
        // Usually removed when its client disconnects; this also covers the host leaving.
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
        ControlledBy {
            owner: trigger.entity,
            lifetime: Lifetime::default(),
        },
    ));
    info!("Player {:?} joined", remote_id.0);
}
