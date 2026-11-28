//! The in-session Escape menu, shown over the lobby, waiting screens and minigames.
//!
//! Pressing `Esc` in a session opens this menu instead of leaving immediately. Both host and
//! joiners can resume, open Controls or Sound settings, or leave to the main menu; the host also
//! gets a "Back to lobby" button whenever the session is outside the lobby.

use bevy::input_focus::tab_navigation::TabGroup;
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;

use super::MenuButton;
use super::widgets::{label, wide_menu_button};
use crate::app_state::AppState;
use crate::network::{Hosting, is_host};
use crate::session::SessionPhase;

/// Sits above the lobby, waiting screens, HUD and standings overlay, and below Controls/Sound.
const SESSION_MENU_Z_INDEX: i32 = 10;
const BACKDROP_COLOR: Color = Color::srgba(0.05, 0.07, 0.12, 0.80);

/// The in-session Escape menu overlay.
#[derive(Component)]
pub struct SessionMenuScreen;

/// The host-only "Back to lobby" button inside [`SessionMenuScreen`].
#[derive(Component)]
pub(super) struct BackToLobbyButton;

/// Whether the Escape menu should offer "Back to lobby" for a peer that is `hosting` (or not)
/// while the session is in `phase`.
pub fn can_return_to_lobby(hosting: bool, phase: Option<SessionPhase>) -> bool {
    hosting && phase.is_some_and(|phase| phase != SessionPhase::Lobby)
}

/// Command: show the in-session Escape menu over the current screen, unless it is already open.
#[derive(Debug, Clone, Copy)]
pub struct OpenSessionMenu;

impl Command for OpenSessionMenu {
    type Out = ();

    fn apply(self, world: &mut World) {
        let open = world
            .query_filtered::<(), With<SessionMenuScreen>>()
            .iter(world)
            .next()
            .is_some();
        if open {
            return;
        }
        let hosting = world.run_system_cached(is_host).unwrap_or(false);
        let phase = world
            .get_resource::<State<SessionPhase>>()
            .map(|state| *state.get());
        let show_back_to_lobby = can_return_to_lobby(hosting, phase);
        debug!("Opening the in-session menu (hosting={hosting}, phase={phase:?})");

        let back_to_lobby = world
            .spawn((
                BackToLobbyButton,
                wide_menu_button(MenuButton::ReturnToLobby, "Back to lobby"),
            ))
            .id();
        if !show_back_to_lobby && let Some(mut node) = world.get_mut::<Node>(back_to_lobby) {
            node.display = Display::None;
        }

        let menu = world
            .spawn((
                SessionMenuScreen,
                DespawnOnExit(AppState::InSession),
                // `Tab` stays on this menu.
                TabGroup::modal(),
                GlobalZIndex(SESSION_MENU_Z_INDEX),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: px(16),
                    ..default()
                },
                BackgroundColor(BACKDROP_COLOR),
                children![
                    label("Menu", 48.0, Color::WHITE),
                    (
                        wide_menu_button(MenuButton::CloseSessionMenu, "Resume"),
                        AutoFocus,
                    ),
                    wide_menu_button(MenuButton::Controls, "Controls"),
                    wide_menu_button(MenuButton::Sound, "Sound"),
                ],
            ))
            .id();
        world.entity_mut(menu).add_child(back_to_lobby);
        world
            .entity_mut(menu)
            .with_child(wide_menu_button(MenuButton::LeaveToMainMenu, "Main menu"));
    }
}

/// Command: close the in-session Escape menu.
#[derive(Debug, Clone, Copy)]
pub struct CloseSessionMenu;

impl Command for CloseSessionMenu {
    type Out = ();

    fn apply(self, world: &mut World) {
        let screens: Vec<Entity> = world
            .query_filtered::<Entity, With<SessionMenuScreen>>()
            .iter(world)
            .collect();
        if screens.is_empty() {
            return;
        }
        for screen in screens {
            world.despawn(screen);
        }
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            focus.clear();
        }
        debug!("Closed the in-session menu");
    }
}

/// `Esc` opens the in-session menu, or closes it if already open.
pub(super) fn toggle_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    open: Query<(), With<SessionMenuScreen>>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if open.is_empty() {
        commands.queue(OpenSessionMenu);
    } else {
        commands.queue(CloseSessionMenu);
    }
}

/// Keep the host's "Back to lobby" button visible only while the session is outside the lobby.
pub(super) fn update_back_to_lobby_button(
    hosting: Hosting,
    phase: Option<Res<State<SessionPhase>>>,
    mut buttons: Query<&mut Node, With<BackToLobbyButton>>,
) {
    let show = can_return_to_lobby(hosting.is_hosting(), phase.map(|phase| *phase.get()));
    let expected = if show { Display::Flex } else { Display::None };
    for mut node in &mut buttons {
        if node.display != expected {
            node.display = expected;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use lightyear::prelude::server::{Server, Started};

    use super::{
        BackToLobbyButton, CloseSessionMenu, OpenSessionMenu, SessionMenuScreen,
        can_return_to_lobby, toggle_on_escape, update_back_to_lobby_button,
    };
    use crate::minigames::Minigame;
    use crate::session::SessionPhase;

    fn open_count(world: &mut World) -> usize {
        world
            .query_filtered::<(), With<SessionMenuScreen>>()
            .iter(world)
            .count()
    }

    #[test]
    fn back_to_lobby_is_offered_only_to_the_host_outside_the_lobby() {
        for phase in [
            SessionPhase::Playing(Minigame::Racing),
            SessionPhase::Playing(Minigame::Shootout),
            SessionPhase::NextRound,
            SessionPhase::NextMatch,
            SessionPhase::Results,
            SessionPhase::Standings,
            SessionPhase::PartyOver,
        ] {
            assert!(can_return_to_lobby(true, Some(phase)), "{phase:?}");
            assert!(!can_return_to_lobby(false, Some(phase)), "{phase:?}");
        }
        assert!(!can_return_to_lobby(true, Some(SessionPhase::Lobby)));
        assert!(!can_return_to_lobby(false, Some(SessionPhase::Lobby)));
        assert!(!can_return_to_lobby(true, None));
    }

    #[test]
    fn escape_toggles_the_session_menu_and_back_to_lobby_tracks_host_and_phase() {
        let mut world = World::new();
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Escape);
        world.insert_resource(keys);
        world.insert_resource(State::new(SessionPhase::Playing(Minigame::Shootout)));

        // As a joiner, Esc opens the menu once with "Back to lobby" hidden.
        world.run_system_once(toggle_on_escape).unwrap();
        assert_eq!(open_count(&mut world), 1);
        world.commands().queue(OpenSessionMenu);
        world.flush();
        assert_eq!(open_count(&mut world), 1);
        let display = world
            .query_filtered::<&Node, With<BackToLobbyButton>>()
            .single(&world)
            .unwrap()
            .display;
        assert_eq!(display, Display::None);

        // If hosting outside the lobby, "Back to lobby" becomes visible; in Lobby it hides again.
        world.spawn((Server, Started));
        world.run_system_once(update_back_to_lobby_button).unwrap();
        let display = world
            .query_filtered::<&Node, With<BackToLobbyButton>>()
            .single(&world)
            .unwrap()
            .display;
        assert_eq!(display, Display::Flex);

        world.insert_resource(State::new(SessionPhase::Lobby));
        world.run_system_once(update_back_to_lobby_button).unwrap();
        let display = world
            .query_filtered::<&Node, With<BackToLobbyButton>>()
            .single(&world)
            .unwrap()
            .display;
        assert_eq!(display, Display::None);

        // Pressing Esc again closes the menu.
        world.run_system_once(toggle_on_escape).unwrap();
        assert_eq!(open_count(&mut world), 0);

        // Closing when already closed is a safe no-op.
        world.commands().queue(CloseSessionMenu);
        world.flush();
        assert_eq!(open_count(&mut world), 0);
    }
}
