//! Menus and on-screen hints: the main menu, the connecting screen, and the in-session overlay.
//!
//! All local UI, driven by [`AppState`] and [`SessionPhase`]:
//!
//! - **Main menu** ([`AppState::MainMenu`]): host, join an address, or quit. Shows why the last
//!   session ended, if it wasn't the player's choice.
//! - **Connecting** ([`AppState::Connecting`]): a cancel button.
//! - **In a session** ([`AppState::InSession`]): `Esc` leaves. In the lobby, a line shows how many
//!   players are waiting.
//!
//! Needs a renderer, so the binary adds it next to the camera and minigame visuals; headless tests
//! drive the same states through [`HostSession`], [`JoinSession`] and [`LeaveSession`] instead.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bevy::input_focus::AutoFocus;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::Pressed;
use bevy::ui_widgets::{Activate, Button, TextInput};

use crate::app_state::{AppState, SessionEndReason};
use crate::config::network::{HOST_ADDR, SERVER_ADDR, SERVER_PORT};
use crate::lobby::Player;
use crate::network::{HostSession, JoinSession, LeaveSession};
use crate::session::SessionPhase;

/// Background of a button that isn't hovered or pressed.
const BUTTON_COLOR: Color = Color::srgb(0.15, 0.15, 0.18);
/// Background of a hovered button.
const BUTTON_HOVERED_COLOR: Color = Color::srgb(0.25, 0.25, 0.3);
/// Background of a pressed button.
const BUTTON_PRESSED_COLOR: Color = Color::srgb(0.35, 0.55, 0.85);
/// Colour of messages about a session that ended or an address that can't be used.
const ERROR_COLOR: Color = Color::srgb(1.0, 0.45, 0.45);
/// Colour of secondary text, such as hints.
const HINT_COLOR: Color = Color::srgb(0.7, 0.7, 0.75);

/// What a menu button does when activated.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuButton {
    /// Host a session on [`HOST_ADDR`].
    Host,
    /// Join the session at the address typed into the [`AddressInput`].
    Join,
    /// Close the game.
    Quit,
    /// Stop connecting and go back to the main menu.
    Cancel,
}

/// The text field holding the address to join.
#[derive(Component)]
pub struct AddressInput;

/// The line under the main menu buttons that explains what went wrong, if anything.
#[derive(Component)]
pub struct MenuMessage;

/// The lobby line showing how many players are waiting.
#[derive(Component)]
pub struct LobbyStatus;

/// Adds the menus and in-session hints.
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), spawn_main_menu);
        app.add_systems(OnEnter(AppState::Connecting), spawn_connecting_screen);
        app.add_systems(OnEnter(AppState::InSession), spawn_leave_hint);
        app.add_systems(OnEnter(SessionPhase::Lobby), spawn_lobby_status);
        app.add_observer(on_menu_button);
        app.add_systems(
            Update,
            (
                style_buttons,
                join_on_enter.run_if(in_state(AppState::MainMenu)),
                leave_on_escape.run_if(not(in_state(AppState::MainMenu))),
                update_lobby_status.run_if(in_state(SessionPhase::Lobby)),
            ),
        );
    }
}

/// A full-screen node that centres its children in a column. Removed when leaving `state`.
fn screen<S: States>(state: S) -> impl Bundle {
    (
        DespawnOnExit(state),
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(16),
            ..default()
        },
    )
}

/// A line of text in the given size and colour.
fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

/// A clickable button labelled `text` that does `action`.
fn menu_button(action: MenuButton, text: &str) -> impl Bundle {
    (
        action,
        Button,
        Hovered::default(),
        Node {
            width: px(200),
            height: px(48),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(BUTTON_COLOR),
        children![label(text, 24.0, Color::WHITE)],
    )
}

/// The main menu: title, host, address field and join, quit, and the reason the last session
/// ended (if any).
fn spawn_main_menu(mut commands: Commands, reason: Res<SessionEndReason>) {
    commands.spawn((
        screen(AppState::MainMenu),
        children![
            label("SMOCKA", 64.0, Color::WHITE),
            menu_button(MenuButton::Host, "Host game"),
            (
                Node {
                    column_gap: px(12),
                    align_items: AlignItems::Center,
                    ..default()
                },
                children![
                    (
                        AddressInput,
                        TextInput,
                        EditableText {
                            max_characters: Some(64),
                            visible_width: Some(14.0),
                            ..EditableText::new(SERVER_ADDR.to_string())
                        },
                        TextFont {
                            font_size: FontSize::Px(24.0),
                            ..default()
                        },
                        TextLayout::no_wrap(),
                        TextCursorStyle::default(),
                        AutoFocus,
                        Node {
                            padding: UiRect::axes(px(10), px(10)),
                            border: px(2).all(),
                            border_radius: BorderRadius::all(px(6)),
                            ..default()
                        },
                        BorderColor::all(HINT_COLOR),
                        BackgroundColor(BUTTON_COLOR),
                    ),
                    menu_button(MenuButton::Join, "Join"),
                ],
            ),
            menu_button(MenuButton::Quit, "Quit"),
            (
                MenuMessage,
                label(reason.0.clone().unwrap_or_default(), 20.0, ERROR_COLOR),
            ),
        ],
    ));
}

/// The connecting screen: a message and a cancel button.
fn spawn_connecting_screen(mut commands: Commands) {
    commands.spawn((
        screen(AppState::Connecting),
        children![
            label("Connecting…", 32.0, Color::WHITE),
            menu_button(MenuButton::Cancel, "Cancel"),
        ],
    ));
}

/// While in a session, remind the player how to leave.
fn spawn_leave_hint(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::InSession),
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(8),
            ..default()
        },
        children![label("Esc: leave", 16.0, HINT_COLOR)],
    ));
}

/// In the lobby, show how many players are waiting.
fn spawn_lobby_status(mut commands: Commands) {
    commands.spawn((
        screen(SessionPhase::Lobby),
        children![(LobbyStatus, label("", 28.0, Color::WHITE))],
    ));
}

/// Keep the lobby line up to date with the number of connected players.
fn update_lobby_status(
    players: Query<(), With<Player>>,
    mut status: Single<&mut Text, With<LobbyStatus>>,
) {
    let shown = lobby_status(players.iter().len());
    if status.0 != shown {
        status.0 = shown;
    }
}

/// The lobby line for `players` connected players.
fn lobby_status(players: usize) -> String {
    format!("Waiting for players ({players}/2)")
}

/// Colour each button for its hover and press state.
fn style_buttons(
    mut buttons: Query<(&Hovered, Has<Pressed>, &mut BackgroundColor), With<MenuButton>>,
) {
    for (hovered, pressed, mut background) in &mut buttons {
        let color = match (pressed, hovered.get()) {
            (true, _) => BUTTON_PRESSED_COLOR,
            (false, true) => BUTTON_HOVERED_COLOR,
            (false, false) => BUTTON_COLOR,
        };
        background.set_if_neq(BackgroundColor(color));
    }
}

/// Do what a [`MenuButton`] says when it is clicked.
fn on_menu_button(
    trigger: On<Activate>,
    buttons: Query<&MenuButton>,
    address: Query<&EditableText, With<AddressInput>>,
    mut message: Query<&mut Text, With<MenuMessage>>,
    mut commands: Commands,
) {
    let Ok(button) = buttons.get(trigger.entity) else {
        return;
    };
    match button {
        MenuButton::Host => commands.queue(HostSession { addr: HOST_ADDR }),
        MenuButton::Join => {
            let typed = address
                .single()
                .map(|text| text.value().to_string())
                .unwrap_or_default();
            join(&typed, &mut commands, &mut message);
        }
        MenuButton::Quit => {
            commands.write_message(AppExit::Success);
        }
        MenuButton::Cancel => commands.queue(LeaveSession),
    }
}

/// In the main menu, `Enter` joins the typed address, like the Join button.
fn join_on_enter(
    keys: Res<ButtonInput<KeyCode>>,
    address: Single<&EditableText, With<AddressInput>>,
    mut message: Query<&mut Text, With<MenuMessage>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Enter) {
        join(&address.value().to_string(), &mut commands, &mut message);
    }
}

/// Join the session at `typed`, or explain in the menu message why it isn't a usable address.
fn join(typed: &str, commands: &mut Commands, message: &mut Query<&mut Text, With<MenuMessage>>) {
    match parse_address(typed) {
        Some(addr) => {
            commands.queue(JoinSession { addr });
        }
        None => {
            for mut text in message.iter_mut() {
                text.0 = format!(
                    "\"{}\" isn't an address like 192.168.1.10:5000.",
                    typed.trim()
                );
            }
        }
    }
}

/// In a session or while connecting, `Esc` leaves and goes back to the main menu.
fn leave_on_escape(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::Escape) {
        commands.queue(LeaveSession);
    }
}

/// The address a player typed, as something to connect to.
///
/// Accepts an IPv4 address with or without a port (`192.168.1.10`, `192.168.1.10:5000`), or
/// `localhost`. Without a port, [`SERVER_PORT`] is used. Host names other than `localhost` are not
/// looked up, so this never blocks on DNS.
pub fn parse_address(typed: &str) -> Option<SocketAddr> {
    let typed = typed.trim();
    let (host, port) = match typed.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (typed, SERVER_PORT),
    };
    let ip = if host.eq_ignore_ascii_case("localhost") {
        Ipv4Addr::LOCALHOST
    } else {
        host.parse().ok()?
    };
    Some(SocketAddr::new(IpAddr::V4(ip), port))
}

#[cfg(test)]
mod tests {
    use super::{lobby_status, parse_address};
    use crate::config::network::SERVER_PORT;
    use std::net::SocketAddr;

    fn addr(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    #[test]
    fn reads_an_address_with_a_port() {
        assert_eq!(
            parse_address("192.168.1.10:6000"),
            Some(addr("192.168.1.10:6000"))
        );
    }

    #[test]
    fn uses_the_default_port_when_none_is_given() {
        assert_eq!(
            parse_address(" 10.0.0.2 "),
            Some(addr(&format!("10.0.0.2:{SERVER_PORT}")))
        );
    }

    #[test]
    fn understands_localhost() {
        assert_eq!(
            parse_address("localhost:5001"),
            Some(addr("127.0.0.1:5001"))
        );
    }

    #[test]
    fn rejects_anything_else() {
        for typed in ["", "my-friend", "1.2.3", "1.2.3.4:port", "1.2.3.4:99999"] {
            assert_eq!(parse_address(typed), None, "{typed:?}");
        }
    }

    #[test]
    fn lobby_status_counts_players() {
        assert_eq!(lobby_status(1), "Waiting for players (1/2)");
    }
}
