use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, Button};
use lightyear::prelude::*;

use super::MenuButton;
use super::widgets::{HINT_COLOR, label, menu_button, screen};
use crate::lobby::{PLAYER_COLORS, PickColor, Player, color_is_free, local_peer_id, player_color};
use crate::minigames::Minigame;
use crate::network::Hosting;
use crate::session::{SessionPhase, can_start};

/// Size of the colour swatches you click to pick a colour, in screen pixels.
const SWATCH_SIZE: f32 = 36.0;
/// Size of the colour square next to each player's name.
const ROW_SWATCH_SIZE: f32 = 18.0;
/// A colour someone else has is drawn this transparent.
const TAKEN_ALPHA: f32 = 0.2;

/// The list of players in the lobby: one row per player, rebuilt when anything in it changes.
#[derive(Component)]
pub struct PlayerList;

/// The line under the player list saying what happens next.
#[derive(Component)]
pub struct LobbyHint;

/// A clickable swatch that picks colour `.0` from [`PLAYER_COLORS`].
#[derive(Component, Debug, Clone, Copy)]
pub struct ColorSwatch(pub u8);

pub(super) fn spawn_lobby_screen(hosting: Hosting, mut commands: Commands) {
    let root = commands
        .spawn((
            screen(SessionPhase::Lobby),
            children![
                label("Lobby", 48.0, Color::WHITE),
                (
                    PlayerList,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    },
                ),
                label("Your colour", 20.0, HINT_COLOR),
            ],
        ))
        .id();
    let swatches = commands
        .spawn(Node {
            column_gap: px(10),
            ..default()
        })
        .id();
    for index in 0..PLAYER_COLORS.len() as u8 {
        commands.entity(swatches).with_child(swatch(index));
    }
    commands.entity(root).add_child(swatches);
    if hosting.is_hosting() {
        for game in Minigame::ALL {
            commands.entity(root).with_child((
                menu_button(MenuButton::Start(game), game.name()),
                InteractionDisabled,
            ));
        }
    }
    commands
        .entity(root)
        .with_child((LobbyHint, label("", 20.0, HINT_COLOR)));
}

fn swatch(index: u8) -> impl Bundle {
    (
        ColorSwatch(index),
        Button,
        Hovered::default(),
        TabIndex(0),
        Node {
            width: px(SWATCH_SIZE),
            height: px(SWATCH_SIZE),
            border: px(3).all(),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(player_color(index)),
        BorderColor::all(Color::NONE),
    )
}

/// Clicking a swatch asks for its colour.
pub(super) fn on_swatch(
    trigger: On<Activate>,
    swatches: Query<&ColorSwatch>,
    mut commands: Commands,
) {
    if let Ok(ColorSwatch(index)) = swatches.get(trigger.entity) {
        commands.queue(PickColor(*index));
    }
}

/// The players, sorted in joining order, and which of them is this app's.
fn players_and_you<'a>(
    players: &'a Query<&Player>,
    clients: &Query<&LocalId, (With<Client>, With<Connected>)>,
) -> (Vec<&'a Player>, Option<PeerId>) {
    let mut players: Vec<&Player> = players.iter().collect();
    players.sort_by_key(|player| player.number);
    (players, local_peer_id(clients))
}

/// Rebuild the player rows when a name, a colour or the players themselves change.
pub(super) fn update_player_list(
    players: Query<&Player>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    list: Single<Entity, With<PlayerList>>,
    mut shown: Local<Vec<(String, u8)>>,
    mut commands: Commands,
) {
    let (players, you) = players_and_you(&players, &clients);
    let rows: Vec<(String, u8)> = players
        .iter()
        .map(|player| (player_label(player, you), player.color))
        .collect();
    if *shown == rows {
        return;
    }
    commands.entity(*list).despawn_children();
    for (text, color) in &rows {
        commands.entity(*list).with_child(player_row(text, *color));
    }
    *shown = rows;
}

/// Outline your own colour, dim and disable colours other players have.
pub(super) fn update_swatches(
    players: Query<&Player>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut swatches: Query<SwatchLook>,
    focus: Res<InputFocus>,
    focus_visible: Res<InputFocusVisible>,
    mut commands: Commands,
) {
    let (players, you) = players_and_you(&players, &clients);
    let own = players.iter().find(|player| Some(player.id) == you);
    for (ColorSwatch(index), disabled, hovered, mut background, mut border, entity) in &mut swatches
    {
        let yours = own.is_some_and(|own| own.color == *index);
        let free = own.is_some_and(|own| color_is_free(players.iter().copied(), own.id, *index));
        let alpha = if free { 1.0 } else { TAKEN_ALPHA };
        background.set_if_neq(BackgroundColor(player_color(*index).with_alpha(alpha)));
        let tabbed_to = focus_visible.0 && focus.get() == Some(entity);
        let outline = match (yours, (hovered.get() || tabbed_to) && free) {
            (true, _) => Color::WHITE,
            (false, true) => HINT_COLOR,
            (false, false) => Color::NONE,
        };
        border.set_if_neq(BorderColor::all(outline));
        match (free && !yours, disabled) {
            (true, true) => {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
            (false, false) => {
                commands.entity(entity).insert(InteractionDisabled);
            }
            _ => {}
        }
    }
}

/// Keep the hint and the start buttons up to date with how many players are here.
pub(super) fn update_lobby_hint(
    players: Query<&Player>,
    hosting: Hosting,
    mut hint: Single<&mut Text, With<LobbyHint>>,
    start: Query<(Entity, Has<InteractionDisabled>), With<MenuButton>>,
    mut commands: Commands,
) {
    let startable = can_start(players.iter().len());
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

/// A swatch's state and the parts of it that show that state.
type SwatchLook = (
    &'static ColorSwatch,
    Has<InteractionDisabled>,
    &'static Hovered,
    &'static mut BackgroundColor,
    &'static mut BorderColor,
    Entity,
);

/// A player's row: a square of their colour, then their name.
fn player_row(text: &str, color: u8) -> impl Bundle {
    (
        Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        },
        children![
            (
                Node {
                    width: px(ROW_SWATCH_SIZE),
                    height: px(ROW_SWATCH_SIZE),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(player_color(color)),
            ),
            label(text.to_string(), 24.0, Color::WHITE),
        ],
    )
}

/// A player's name, marking the host and `you`.
pub fn player_label(player: &Player, you: Option<PeerId>) -> String {
    let mut line = player.name().to_string();
    match (player.is_host(), Some(player.id) == you) {
        (true, true) => line.push_str(" (host, you)"),
        (true, false) => line.push_str(" (host)"),
        (false, true) => line.push_str(" (you)"),
        (false, false) => {}
    }
    line
}

pub fn lobby_hint(hosting: bool, startable: bool) -> &'static str {
    match (hosting, startable) {
        (true, true) => "Pick a game when everyone is here.",
        (true, false) => "Waiting for at least one more player.",
        (false, _) => "Waiting for the host to start.",
    }
}

#[cfg(test)]
mod tests {
    use super::{lobby_hint, player_label};
    use crate::lobby::Player;
    use lightyear::prelude::PeerId;

    fn player(id: PeerId, number: u32, name: &str) -> Player {
        Player {
            id,
            number,
            name: name.into(),
            color: 0,
        }
    }

    #[test]
    fn marks_the_host_and_you() {
        let host = player(PeerId::Local(0), 1, "Ada");
        let joiner = player(PeerId::Netcode(7), 2, "Bo");
        assert_eq!(player_label(&host, Some(PeerId::Netcode(7))), "Ada (host)");
        assert_eq!(player_label(&joiner, Some(PeerId::Netcode(7))), "Bo (you)");
        assert_eq!(
            player_label(&host, Some(PeerId::Local(0))),
            "Ada (host, you)"
        );
        assert_eq!(player_label(&joiner, Some(PeerId::Local(0))), "Bo");
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
