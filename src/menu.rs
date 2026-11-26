//! All local UI: one file per screen, plus the shared widgets they are built from.

mod lobby;
mod main_menu;
mod notices;
mod results;
mod standings;
mod widgets;

pub use main_menu::{AddressInput, MenuMessage, NameInput};

use bevy::input_focus::tab_navigation::TabNavigationPlugin;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::Activate;
use bevy::window::WindowCloseRequested;

use crate::app_state::AppState;
use crate::config::network::HOST_ADDR;
use crate::minigames::Minigame;
use crate::minigames::racing::race::ChangeLaps;
use crate::network::{HostSession, LeaveSession, close_session};
use crate::profile::LocalProfile;
use crate::session::{SessionPhase, StartMinigame};
use widgets::{HINT_COLOR, label, menu_button, screen};

/// What a menu button does when activated.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuButton {
    Host,
    Join,
    Quit,
    Cancel,
    Start(Minigame),
    /// Change how many laps the next race has by this many.
    ChangeLaps(i32),
}

/// All local UI. Needs a renderer, so headless tests leave it out.
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        // `Tab` / `Shift+Tab` move between fields and buttons. Not part of `DefaultPlugins`.
        if !app.is_plugin_added::<TabNavigationPlugin>() {
            app.add_plugins(TabNavigationPlugin);
        }
        // Only the real game remembers the player's name; headless tests start with none.
        app.insert_resource(LocalProfile::load());
        app.init_resource::<main_menu::NameEdited>();
        app.add_systems(OnEnter(AppState::MainMenu), main_menu::spawn_main_menu);
        app.add_systems(OnExit(AppState::MainMenu), main_menu::save_profile);
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
        app.add_systems(
            OnEnter(SessionPhase::Standings),
            standings::spawn_standings_screen,
        );
        app.add_observer(on_menu_button);
        app.add_observer(lobby::on_swatch);
        app.add_observer(notices::notice_player_left);
        app.add_systems(
            Update,
            (
                widgets::style_buttons,
                (
                    main_menu::join_on_enter.run_if(main_menu::address_input_focused),
                    main_menu::show_session_end_reason,
                    main_menu::follow_name_input,
                )
                    .run_if(in_state(AppState::MainMenu)),
                leave_on_escape.run_if(not(in_state(AppState::MainMenu))),
                (
                    lobby::update_player_list,
                    lobby::update_swatches,
                    lobby::update_lobby_hint,
                    lobby::update_lap_picker,
                )
                    .run_if(in_state(SessionPhase::Lobby)),
                results::update_results_screen.run_if(in_state(SessionPhase::Results)),
                standings::update_standings_screen.run_if(in_state(SessionPhase::Standings)),
                widgets::update_countdowns,
                notices::expire_notices,
                close_session.run_if(on_message::<WindowCloseRequested>),
            ),
        );
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
            main_menu::join(&typed, &mut commands, &mut message);
        }
        MenuButton::Quit => {
            commands.write_message(AppExit::Success);
        }
        MenuButton::Cancel => commands.queue(LeaveSession),
        MenuButton::Start(game) => commands.queue(StartMinigame(*game)),
        MenuButton::ChangeLaps(change) => commands.queue(ChangeLaps(*change)),
    }
}

fn spawn_connecting_screen(mut commands: Commands) {
    commands.spawn((
        screen(AppState::Connecting),
        children![
            label("Connecting...", 32.0, Color::WHITE),
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

fn leave_on_escape(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::Escape) {
        commands.queue(LeaveSession);
    }
}
