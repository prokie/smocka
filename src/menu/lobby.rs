//! The lobby screen: who is here, and (for the host) the button that starts a minigame.

use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use lightyear::prelude::*;

use super::{HINT_COLOR, MenuButton, label, menu_button, screen};
use crate::lobby::{Player, local_peer_id};
use crate::network::Hosting;
use crate::session::{SessionPhase, can_start};

/// The list of players in the lobby.
#[derive(Component)]
pub struct PlayerList;

/// The line under the player list saying what happens next.
#[derive(Component)]
pub struct LobbyHint;

/// When the lobby opens, show the player list, plus a Start button if this app is hosting.
pub(super) fn spawn_lobby_screen(hosting: Hosting, mut commands: Commands) {
    let root = commands
        .spawn((
            screen(SessionPhase::Lobby),
            children![
                label("Lobby", 48.0, Color::WHITE),
                (PlayerList, label("", 24.0, Color::WHITE)),
            ],
        ))
        .id();
    if hosting.is_hosting() {
        commands
            .entity(root)
            .with_child((menu_button(MenuButton::Start, "Start"), InteractionDisabled));
    }
    commands
        .entity(root)
        .with_child((LobbyHint, label("", 20.0, HINT_COLOR)));
}

/// Keep the player list, the hint and the Start button up to date with who is connected.
pub(super) fn update_lobby_screen(
    players: Query<&Player>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    hosting: Hosting,
    mut list: Single<&mut Text, (With<PlayerList>, Without<LobbyHint>)>,
    mut hint: Single<&mut Text, (With<LobbyHint>, Without<PlayerList>)>,
    start: Query<(Entity, Has<InteractionDisabled>), With<MenuButton>>,
    mut commands: Commands,
) {
    let mut players: Vec<&Player> = players.iter().collect();
    players.sort_by_key(|player| player.number);
    let you = local_peer_id(&clients);

    let shown = player_list(&players, you);
    if list.0 != shown {
        list.0 = shown;
    }

    let startable = can_start(players.len());
    let shown = lobby_hint(hosting.is_hosting(), startable);
    if hint.0 != shown {
        hint.0 = shown.to_string();
    }

    for (button, disabled) in &start {
        match (startable, disabled) {
            (true, true) => {
                commands.entity(button).remove::<InteractionDisabled>();
            }
            (false, false) => {
                commands.entity(button).insert(InteractionDisabled);
            }
            _ => {}
        }
    }
}

/// One line per player, in joining order, marking the host and `you`.
pub fn player_list(players: &[&Player], you: Option<PeerId>) -> String {
    players
        .iter()
        .map(|player| {
            let mut line = player.name();
            match (player.is_host(), Some(player.id) == you) {
                (true, true) => line.push_str(" (host, you)"),
                (true, false) => line.push_str(" (host)"),
                (false, true) => line.push_str(" (you)"),
                (false, false) => {}
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the lobby tells this player to expect.
pub fn lobby_hint(hosting: bool, startable: bool) -> &'static str {
    match (hosting, startable) {
        (true, true) => "Press Start when everyone is here.",
        (true, false) => "Waiting for at least one more player.",
        (false, _) => "Waiting for the host to start.",
    }
}

#[cfg(test)]
mod tests {
    use super::{lobby_hint, player_list};
    use crate::lobby::Player;
    use lightyear::prelude::PeerId;

    #[test]
    fn marks_the_host_and_you() {
        let host = Player {
            id: PeerId::Local(0),
            number: 1,
        };
        let joiner = Player {
            id: PeerId::Netcode(7),
            number: 2,
        };
        assert_eq!(
            player_list(&[&host, &joiner], Some(PeerId::Netcode(7))),
            "Player 1 (host)\nPlayer 2 (you)"
        );
        assert_eq!(
            player_list(&[&host, &joiner], Some(PeerId::Local(0))),
            "Player 1 (host, you)\nPlayer 2"
        );
    }

    #[test]
    fn joiners_wait_for_the_host() {
        assert_eq!(lobby_hint(false, true), "Waiting for the host to start.");
        assert_eq!(
            lobby_hint(true, false),
            "Waiting for at least one more player."
        );
    }
}
