mod lobby;
mod notices;
mod results;

use bevy::ecs::error::warn;
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{Activate, Button, TextInput};
use bevy::window::WindowCloseRequested;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::app_state::{AppState, SessionEndReason};
use crate::config::network::{HOST_ADDR, SERVER_ADDR, SERVER_PORT, VERSION};
use crate::minigames::Minigame;
use crate::network::{HostSession, JoinSession, LeaveSession, close_session};
use crate::session::{SessionPhase, StartMinigame};

const BUTTON_COLOR: Color = Color::srgb(0.15, 0.15, 0.18);
const BUTTON_HOVERED_COLOR: Color = Color::srgb(0.25, 0.25, 0.3);
const BUTTON_PRESSED_COLOR: Color = Color::srgb(0.35, 0.55, 0.85);
const BUTTON_DISABLED_COLOR: Color = Color::srgb(0.1, 0.1, 0.1);
const ERROR_COLOR: Color = Color::srgb(1.0, 0.45, 0.45);
const HINT_COLOR: Color = Color::srgb(0.7, 0.7, 0.75);

/// What a menu button does when activated.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuButton {
    Host,
    Join,
    Quit,
    Cancel,
    Start(Minigame),
}

/// The text field holding the address to join.
#[derive(Component)]
pub struct AddressInput;

/// The main menu line that explains what went wrong, if anything.
#[derive(Component)]
pub struct MenuMessage;

/// All local UI. Needs a renderer, so headless tests leave it out.
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::MainMenu), spawn_main_menu);
        app.add_systems(OnEnter(AppState::Connecting), spawn_connecting_screen);
        app.add_systems(
            OnEnter(AppState::InSession),
            (spawn_leave_hint, notices::spawn_notice_area),
        );
        app.add_systems(OnEnter(SessionPhase::Lobby), lobby::spawn_lobby_screen);
        app.add_systems(
            OnEnter(SessionPhase::Results),
            results::spawn_results_screen,
        );
        app.add_observer(on_menu_button);
        app.add_observer(notices::notice_player_left);
        app.add_systems(
            Update,
            (
                style_buttons,
                (
                    join_on_enter.run_if(address_input_focused),
                    show_session_end_reason,
                )
                    .run_if(in_state(AppState::MainMenu)),
                leave_on_escape.run_if(not(in_state(AppState::MainMenu))),
                lobby::update_lobby_screen.run_if(in_state(SessionPhase::Lobby)),
                results::update_results_screen.run_if(in_state(SessionPhase::Results)),
                notices::expire_notices,
                close_session.run_if(on_message::<WindowCloseRequested>),
            ),
        );
    }
}

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

fn spawn_main_menu(mut commands: Commands, reason: Res<SessionEndReason>) {
    commands.spawn((
        screen(AppState::MainMenu),
        children![
            label("SMOCKA", 64.0, Color::WHITE),
            label(format!("Version {VERSION}"), 16.0, HINT_COLOR),
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

fn spawn_connecting_screen(mut commands: Commands) {
    commands.spawn((
        screen(AppState::Connecting),
        children![
            label("Connecting…", 32.0, Color::WHITE),
            menu_button(MenuButton::Cancel, "Cancel"),
        ],
    ));
}

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

type ButtonLook = (
    &'static Hovered,
    Has<Pressed>,
    Has<InteractionDisabled>,
    &'static mut BackgroundColor,
);

fn style_buttons(mut buttons: Query<ButtonLook, With<MenuButton>>) {
    for (hovered, pressed, disabled, mut background) in &mut buttons {
        let color = match (disabled, pressed, hovered.get()) {
            (true, _, _) => BUTTON_DISABLED_COLOR,
            (false, true, _) => BUTTON_PRESSED_COLOR,
            (false, false, true) => BUTTON_HOVERED_COLOR,
            (false, false, false) => BUTTON_COLOR,
        };
        background.set_if_neq(BackgroundColor(color));
    }
}

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
        MenuButton::Start(game) => commands.queue(StartMinigame(*game)),
    }
}

/// Gates [`join_on_enter`]: a focused button already turns `Enter` into its own [`Activate`].
fn address_input_focused(
    focus: Option<Res<InputFocus>>,
    address: Query<(), With<AddressInput>>,
) -> bool {
    focus
        .and_then(|focus| focus.get())
        .is_some_and(|entity| address.contains(entity))
}

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

fn join(typed: &str, commands: &mut Commands, message: &mut Query<&mut Text, With<MenuMessage>>) {
    match parse_address(typed) {
        Some(addr) => {
            // A failure leaves the app in this menu, and `show_session_end_reason` shows why.
            commands.queue_handled(JoinSession { addr }, warn);
        }
        None => {
            debug!("Not joining: {typed:?} isn't an address");
            for mut text in message.iter_mut() {
                text.0 = format!(
                    "\"{}\" isn't an address like 192.168.1.10:5000.",
                    typed.trim()
                );
            }
        }
    }
}

/// Shows a reason set while already in the menu, such as a [`JoinSession`] that failed.
fn show_session_end_reason(
    reason: Res<SessionEndReason>,
    mut message: Query<&mut Text, With<MenuMessage>>,
) {
    if !reason.is_changed() {
        return;
    }
    if let Some(text) = &reason.0 {
        for mut line in &mut message {
            line.0.clone_from(text);
        }
    }
}

fn leave_on_escape(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::Escape) {
        commands.queue(LeaveSession);
    }
}

/// Accepts an IPv4 address or `localhost`, with or without a port (default [`SERVER_PORT`]).
/// Other host names are not looked up, so this never blocks on DNS.
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
    use super::{
        AddressInput, MenuMessage, SessionEndReason, address_input_focused, parse_address,
        show_session_end_reason,
    };
    use crate::config::network::SERVER_PORT;
    use bevy::input_focus::InputFocus;
    use bevy::prelude::*;
    use std::net::SocketAddr;

    fn address_focused(world: &mut World) -> bool {
        world.run_system_cached(address_input_focused).unwrap()
    }

    #[test]
    fn enter_joins_only_from_the_address_field() {
        let mut world = World::new();
        let address = world.spawn(AddressInput).id();
        let button = world.spawn_empty().id();

        assert!(!address_focused(&mut world), "no focus resource");

        world.insert_resource(InputFocus::default());
        assert!(!address_focused(&mut world), "nothing focused");

        world.insert_resource(InputFocus::from_entity(button));
        assert!(!address_focused(&mut world), "a button focused");

        world.insert_resource(InputFocus::from_entity(address));
        assert!(address_focused(&mut world), "the address field focused");
    }

    #[test]
    fn shows_a_reason_set_while_in_the_menu() {
        let mut world = World::new();
        world.init_resource::<SessionEndReason>();
        let message = world.spawn((MenuMessage, Text::new(""))).id();
        let mut system = IntoSystem::into_system(show_session_end_reason);
        system.initialize(&mut world);

        system.run((), &mut world).unwrap();
        assert_eq!(world.get::<Text>(message).unwrap().0, "");

        world.resource_mut::<SessionEndReason>().0 = Some("Couldn't join.".into());
        system.run((), &mut world).unwrap();
        assert_eq!(world.get::<Text>(message).unwrap().0, "Couldn't join.");
    }

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
}
