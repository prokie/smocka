use bevy::prelude::*;
use lightyear::netcode::TokenUserData;
use lightyear::prelude::{server::ClientOf, *};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::network::is_host;
use crate::profile::{LocalProfile, clean_name, name_from_user_data};

/// A person in the session. Spawned by the host for every connected client (including its own)
/// and replicated to everyone.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Player {
    pub id: PeerId,
    /// Given out in joining order, starting at 1 for the host, and not reused in the same session.
    pub number: u32,
    /// The name this player picked, or "Player {number}" if they didn't.
    pub name: String,
    /// Index into [`PLAYER_COLORS`]. The host keeps each colour to one player while there are
    /// enough to go round.
    pub color: u8,
}

impl Player {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The colour minigames tint this player's things with.
    pub fn color(&self) -> Color {
        player_color(self.color)
    }

    /// Whether this is the hosting player. The host's own client is linked to its server in the
    /// same process, so it has a local id rather than a netcode one.
    pub fn is_host(&self) -> bool {
        matches!(self.id, PeerId::Local(_))
    }
}

/// The colours players choose from in the lobby.
pub const PLAYER_COLORS: [Color; 8] = [
    Color::srgb(0.35, 0.65, 1.0), // blue
    Color::srgb(1.0, 0.4, 0.4),   // red
    Color::srgb(0.45, 0.9, 0.45), // green
    Color::srgb(1.0, 0.85, 0.3),  // yellow
    Color::srgb(0.8, 0.5, 1.0),   // purple
    Color::srgb(1.0, 0.6, 0.25),  // orange
    Color::srgb(0.35, 0.95, 0.9), // cyan
    Color::srgb(1.0, 0.55, 0.85), // pink
];

/// The colour with index `index` in [`PLAYER_COLORS`]. Indexes past the last colour wrap around.
pub fn player_color(index: u8) -> Color {
    PLAYER_COLORS[index as usize % PLAYER_COLORS.len()]
}

/// The first colour nobody has taken. If every colour is taken, the one fewest players have.
pub fn first_free_color(taken: impl IntoIterator<Item = u8>) -> u8 {
    let mut uses = [0usize; PLAYER_COLORS.len()];
    for color in taken {
        uses[color as usize % PLAYER_COLORS.len()] += 1;
    }
    (0..PLAYER_COLORS.len())
        .min_by_key(|&color| uses[color])
        .unwrap_or_default() as u8
}

/// Whether the player `who` may switch to `color`: it exists, and no other player has it.
pub fn color_is_free<'a>(
    players: impl IntoIterator<Item = &'a Player>,
    who: PeerId,
    color: u8,
) -> bool {
    (color as usize) < PLAYER_COLORS.len()
        && players
            .into_iter()
            .all(|player| player.id == who || player.color != color)
}

/// Client to host: "I'd like this colour". Sent by [`PickColor`]; the host checks it's free.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorRequest(pub u8);

/// The channel profile changes travel on: reliable and in order, since each one matters.
pub struct ProfileChannel;

/// Every peer: triggered when another player leaves the session this app is still in.
///
/// Not triggered for the players removed when this app itself leaves or loses the session.
#[derive(Event, Debug, Clone, PartialEq)]
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
        app.add_channel::<ProfileChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ColorRequest>()
            .add_direction(NetworkDirection::ClientToServer);

        app.init_resource::<NextPlayerNumber>();
        app.init_resource::<LocalProfile>();
        app.add_observer(spawn_player);
        app.add_observer(announce_player_left);
        app.add_systems(OnEnter(AppState::MainMenu), reset_player_numbers);
        app.add_systems(Update, apply_color_requests.run_if(is_host));
    }
}

/// This app's own [`PeerId`] in the session it is connected to, if any. Compare it with
/// [`Player::id`] to find "you".
pub fn local_peer_id(clients: &Query<&LocalId, (With<Client>, With<Connected>)>) -> Option<PeerId> {
    clients.iter().next().map(|id| id.0)
}

/// The new player gets the name a joiner sent in its connect token (or the host's own
/// [`LocalProfile`]), and the first free colour. `ControlledBy` with the default lifetime despawns
/// the player when its client disconnects.
fn spawn_player(
    trigger: On<Add<Connected>>,
    clients: Query<(&RemoteId, Option<&TokenUserData>), With<ClientOf>>,
    players: Query<&Player>,
    profile: Res<LocalProfile>,
    mut next_number: ResMut<NextPlayerNumber>,
    mut commands: Commands,
) {
    let Ok((remote_id, user_data)) = clients.get(trigger.entity) else {
        return;
    };
    let number = next_number.0;
    next_number.0 += 1;
    let picked = match user_data {
        Some(TokenUserData(data)) => name_from_user_data(data),
        // The host's own client is linked in-process, without a token.
        None => clean_name(&profile.name),
    };
    let player = Player {
        id: remote_id.0,
        number,
        name: if picked.is_empty() {
            format!("Player {number}")
        } else {
            picked
        },
        color: first_free_color(players.iter().map(|player| player.color)),
    };
    info!(
        "{} ({:?}) joined with colour {}",
        player.name(),
        remote_id.0,
        player.color
    );
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
}

/// Command: this player asks for colour `index` from [`PLAYER_COLORS`]. The host applies its own
/// pick straight away; a joiner sends a [`ColorRequest`], and the host checks it's free.
#[derive(Debug, Clone, Copy)]
pub struct PickColor(pub u8);

impl Command for PickColor {
    type Out = ();

    fn apply(self, world: &mut World) {
        let PickColor(color) = self;
        if world.run_system_cached(is_host).unwrap_or(false) {
            let mut players = world.query::<&mut Player>();
            let others: Vec<Player> = players.iter(world).cloned().collect();
            let Some(mut own) = players.iter_mut(world).find(|player| player.is_host()) else {
                return;
            };
            set_color(&mut own, &others, color);
            return;
        }
        let mut senders = world
            .query_filtered::<&mut MessageSender<ColorRequest>, (With<Client>, With<Connected>)>();
        match senders.iter_mut(world).next() {
            Some(mut sender) => {
                debug!("Asking the host for colour {color}");
                sender.send::<ProfileChannel>(ColorRequest(color));
            }
            None => warn!("Can't pick a colour: not connected"),
        }
    }
}

/// Give `player` colour `color` if no one in `players` has it; log either way.
fn set_color(player: &mut Player, players: &[Player], color: u8) {
    if color_is_free(players, player.id, color) {
        info!("{} ({:?}) picked colour {color}", player.name, player.id);
        player.color = color;
    } else {
        warn!(
            "{} ({:?}) can't have colour {color}: it's taken or doesn't exist",
            player.name, player.id
        );
    }
}

/// Host only: apply the colours joiners asked for.
fn apply_color_requests(
    mut links: Query<(Entity, &mut MessageReceiver<ColorRequest>), With<ClientOf>>,
    mut players: Query<(&mut Player, &ControlledBy)>,
) {
    for (link, mut receiver) in &mut links {
        for ColorRequest(color) in receiver.receive() {
            let others: Vec<Player> = players.iter().map(|(player, _)| player.clone()).collect();
            let Some((mut player, _)) = players
                .iter_mut()
                .find(|(_, controlled_by)| controlled_by.owner == link)
            else {
                continue;
            };
            set_color(&mut player, &others, color);
        }
    }
}

/// Players removed because this app itself left are not announced.
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
    commands.trigger(PlayerLeft(player.clone()));
}

fn reset_player_numbers(mut next_number: ResMut<NextPlayerNumber>) {
    *next_number = NextPlayerNumber::default();
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::PeerId;

    use super::{PLAYER_COLORS, Player, color_is_free, first_free_color};

    fn player(id: u64, color: u8) -> Player {
        Player {
            id: PeerId::Netcode(id),
            number: id as u32,
            name: format!("Player {id}"),
            color,
        }
    }

    #[test]
    fn new_players_get_the_first_free_colour() {
        assert_eq!(first_free_color([]), 0);
        assert_eq!(first_free_color([0, 1, 3]), 2);
        // Every colour taken once: start sharing from the first.
        let all = 0..PLAYER_COLORS.len() as u8;
        assert_eq!(first_free_color(all.clone()), 0);
        assert_eq!(first_free_color(all.chain([0])), 1);
    }

    #[test]
    fn a_colour_is_free_unless_someone_else_has_it() {
        let players = [player(1, 0), player(2, 3)];
        assert!(color_is_free(&players, PeerId::Netcode(1), 5));
        assert!(!color_is_free(&players, PeerId::Netcode(1), 3));
        // Keeping your own colour is fine.
        assert!(color_is_free(&players, PeerId::Netcode(2), 3));
        assert!(!color_is_free(
            &players,
            PeerId::Netcode(1),
            PLAYER_COLORS.len() as u8
        ));
    }
}
