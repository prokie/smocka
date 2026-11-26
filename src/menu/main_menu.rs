//! The title screen: the player's name, hosting, joining an address, and why the last session
//! ended.

use bevy::ecs::error::warn;
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::text::EditableText;

use super::MenuButton;
use super::widgets::{ERROR_COLOR, HINT_COLOR, label, menu_button, screen, text_field};
use crate::app_state::{AppState, SessionEndReason};
use crate::config::network::{SERVER_ADDR, VERSION};
use crate::network::{JoinSession, parse_address};
use crate::profile::{LocalProfile, MAX_NAME_CHARS, clean_name};

/// The text field holding the address to join.
#[derive(Component)]
pub struct AddressInput;

/// The text field holding this player's name, and the name it was last known to hold, to tell
/// when the player edits it.
#[derive(Component)]
pub struct NameInput(String);

/// Whether the player typed a new name since it was last saved. A name given with `--name` isn't
/// typed, so it isn't saved.
#[derive(Resource, Default)]
pub(super) struct NameEdited(bool);

/// The main menu line that explains what went wrong, if anything.
#[derive(Component)]
pub struct MenuMessage;

pub(super) fn spawn_main_menu(
    mut commands: Commands,
    reason: Res<SessionEndReason>,
    profile: Res<LocalProfile>,
) {
    commands.spawn((
        screen(AppState::MainMenu),
        children![
            label("SMOCKA", 64.0, Color::WHITE),
            label(format!("Version {VERSION}"), 16.0, HINT_COLOR),
            (
                Node {
                    column_gap: px(12),
                    align_items: AlignItems::Center,
                    ..default()
                },
                children![
                    label("Name", 24.0, HINT_COLOR),
                    (
                        NameInput(profile.name.clone()),
                        text_field(profile.name.clone(), MAX_NAME_CHARS, 12.0),
                    ),
                ],
            ),
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
                        text_field(SERVER_ADDR.to_string(), 64, 14.0),
                        AutoFocus,
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

/// When the player edits the name field, use the new name for hosting and joining.
pub(super) fn follow_name_input(
    mut field: Single<(&EditableText, &mut NameInput)>,
    mut profile: ResMut<LocalProfile>,
    mut edited: ResMut<NameEdited>,
) {
    let (text, known) = &mut *field;
    let typed = text.value().to_string();
    if known.0 == typed {
        return;
    }
    known.0.clone_from(&typed);
    profile.name = clean_name(&typed);
    edited.0 = true;
}

/// Leaving the main menu (to host or join): remember a newly typed name for next time.
pub(super) fn save_profile(profile: Res<LocalProfile>, mut edited: ResMut<NameEdited>) {
    if edited.0 {
        profile.save();
        edited.0 = false;
    }
}

/// Gates [`join_on_enter`]: a focused button already turns `Enter` into its own
/// [`Activate`](bevy::ui_widgets::Activate).
pub(super) fn address_input_focused(
    focus: Option<Res<InputFocus>>,
    address: Query<(), With<AddressInput>>,
) -> bool {
    focus
        .and_then(|focus| focus.get())
        .is_some_and(|entity| address.contains(entity))
}

pub(super) fn join_on_enter(
    keys: Res<ButtonInput<KeyCode>>,
    address: Single<&EditableText, With<AddressInput>>,
    mut message: Query<&mut Text, With<MenuMessage>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Enter) {
        join(&address.value().to_string(), &mut commands, &mut message);
    }
}

/// Join the address the player typed, or say why it isn't one.
pub(super) fn join(
    typed: &str,
    commands: &mut Commands,
    message: &mut Query<&mut Text, With<MenuMessage>>,
) {
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
pub(super) fn show_session_end_reason(
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

#[cfg(test)]
mod tests {
    use super::{
        AddressInput, MenuMessage, SessionEndReason, address_input_focused, show_session_end_reason,
    };
    use bevy::input_focus::InputFocus;
    use bevy::prelude::*;

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
}
