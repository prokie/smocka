//! Who is in the session: one replicated [`Player`] per connected person, independent of any minigame.
//!
//! When someone leaves, every peer still in the session gets a [`PlayerLeft`] event, so it can
//! tell its player who went.

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

/// Every peer: triggered when another player leaves the session this app is still in.
///
/// Not triggered for the players removed when this app itself leaves or loses the session.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct PlayerLeft(pub Player);

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
        app.add_observer(announce_player_left);
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

/// When a [`Player`] is removed while this app is connected and in a session, trigger
/// [`PlayerLeft`].
///
/// On the host the player is despawned with its client's connection; on clients its replicated
/// despawn arrives. Players removed because this app left (it is back in the menu, or its client is
/// no longer [`Connected`]) are not announced.
fn announce_player_left(
    trigger: On<Remove<Player>>,
    players: Query<&Player>,
    state: Option<Res<State<AppState>>>,
    connected: Query<(), (With<Client>, With<Connected>)>,
    mut commands: Commands,
) {
    let in_session = state.is_some_and(|state| *state.get() == AppState::InSession);
    if !in_session || connected.is_empty() {
        return;
    }
    let Ok(player) = players.get(trigger.entity) else {
        return;
    };
    info!("{} ({:?}) left", player.name(), player.id);
    commands.trigger(PlayerLeft(*player));
}

/// Number the players of the next hosted session from 1 again.
fn reset_player_numbers(mut next_number: ResMut<NextPlayerNumber>) {
    *next_number = NextPlayerNumber::default();
}
