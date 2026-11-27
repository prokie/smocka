//! All local UI: one file per screen, plus the shared widgets they are built from.

mod background;
mod controls;
mod lobby;
mod main_menu;
mod minigame_settings;
mod next_match;
mod notices;
mod party;
mod results;
mod sound;
mod standings;
mod watching;
mod widgets;

pub use main_menu::{AddressInput, MenuMessage, NameInput};

use bevy::app::OnAppExitSystems;
use bevy::ecs::error::warn;
use bevy::ecs::system::SystemParam;
use bevy::input_focus::tab_navigation::TabNavigationPlugin;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui_widgets::Activate;
use bevy::window::WindowCloseRequested;

use crate::app_state::AppState;
use crate::config::network::HOST_ADDR;
use crate::controls::{ControlAction, Controls};
use crate::minigames::Minigame;
use crate::minigames::settings::{ChangeSetting, SettingId};
use crate::network::{HostSession, Hosting, LeaveSession, close_session};
use crate::party::{ChangeRounds, StartParty};
use crate::profile::{LastAddress, LocalProfile};
use crate::session::{ReturnToLobby, SessionPhase, StartMinigame};
use crate::sound::{ChangeVolume, VolumeControl, VolumeSettings};
use widgets::{HINT_COLOR, label, menu_button, screen};

/// The font for all text.
const UI_FONT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/FiraMono-Regular.ttf"
));

/// What a menu button does when activated.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuButton {
    Host,
    Join,
    Quit,
    Cancel,
    Start(Minigame),
    StartParty,
    /// Change how many rounds the next party has by this many.
    ChangeRounds(i32),
    /// Open the Controls screen.
    Controls,
    /// Close the Controls screen.
    CloseControls,
    /// Put every control back to its default.
    ResetControls,
    /// Bind the next key, gamepad button or stick push to this action's binding (from 0).
    Rebind(ControlAction, usize),
    /// Open the Sound screen.
    Sound,
    /// Close the Sound screen.
    CloseSound,
    /// Change one of the volumes by this many steps.
    ChangeVolume(VolumeControl, i32),
    /// Open this minigame's settings screen (the host, in the lobby).
    MinigameSettings(Minigame),
    /// Close the minigame settings screen.
    CloseMinigameSettings,
    /// Change a minigame setting by this many.
    ChangeSetting(SettingId, i32),
}

/// All local UI. Needs a renderer, so headless tests leave it out.
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        // `Tab` / `Shift+Tab` move between fields and buttons. Not part of `DefaultPlugins`.
        if !app.is_plugin_added::<TabNavigationPlugin>() {
            app.add_plugins(TabNavigationPlugin);
        }
        app.add_plugins(background::MenuBackgroundPlugin);
        // Only the real game remembers the player's name, last address, controls and volumes;
        // headless tests start with none of them.
        app.insert_resource(LocalProfile::load());
        app.insert_resource(LastAddress::load());
        app.insert_resource(Controls::load());
        app.insert_resource(VolumeSettings::load());
        app.add_systems(PreStartup, use_ui_font);
        app.add_systems(Startup, sound::load_volume_check);
        app.init_resource::<main_menu::NameEdited>();
        app.add_systems(OnEnter(AppState::MainMenu), main_menu::spawn_main_menu);
        app.add_systems(OnExit(AppState::MainMenu), main_menu::save_profile);
        // Quitting (the Quit button or closing the window) never leaves the main menu, so save a
        // new name on the way out too. Runs after the window's own exit systems, which send
        // `AppExit` in `Last`.
        app.add_systems(
            Last,
            main_menu::save_profile
                .in_set(OnAppExitSystems)
                .run_if(on_message::<AppExit>),
        );
        app.add_systems(OnEnter(AppState::Connecting), spawn_connecting_screen);
        app.add_systems(
            OnEnter(AppState::InSession),
            (
                spawn_leave_hint,
                notices::spawn_notice_area,
                watching::spawn_watching_text,
                standings::spawn_standings_overlay,
            ),
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
        app.add_systems(
            OnEnter(SessionPhase::NextMatch),
            next_match::spawn_next_match_screen,
        );
        app.add_systems(
            OnEnter(SessionPhase::NextRound),
            party::spawn_next_round_screen,
        );
        app.add_systems(
            OnEnter(SessionPhase::PartyOver),
            party::spawn_party_over_screen,
        );
        app.add_observer(on_menu_button);
        app.add_observer(lobby::on_swatch);
        app.add_observer(notices::notice_player_left);
        app.add_systems(
            Update,
            (
                (
                    widgets::style_buttons,
                    widgets::scroll_focus_into_view,
                    widgets::show_scrollbars_when_needed,
                    widgets::style_scrollbar_thumbs,
                ),
                (
                    main_menu::join_on_enter.run_if(main_menu::address_input_focused),
                    main_menu::show_session_end_reason,
                    main_menu::follow_name_input,
                )
                    .run_if(in_state(AppState::MainMenu)),
                (
                    controls::close_controls_on_escape
                        .run_if(not(resource_exists::<controls::Rebinding>)),
                    controls::capture_control.run_if(resource_exists::<controls::Rebinding>),
                    controls::update_control_labels,
                )
                    .chain()
                    .run_if(any_with_component::<controls::ControlsScreen>),
                save_controls
                    .run_if(resource_changed::<Controls>.and_then(not(resource_added::<Controls>))),
                (sound::close_sound_on_escape, sound::update_volume_labels)
                    .run_if(any_with_component::<sound::SoundScreen>),
                save_volume.run_if(
                    resource_changed::<VolumeSettings>
                        .and_then(not(resource_added::<VolumeSettings>)),
                ),
                (
                    // On a settings screen, `Esc` closes it instead.
                    escape_session.run_if(not(in_state(AppState::MainMenu)).and_then(not(
                        any_with_component::<minigame_settings::MinigameSettingsScreen>,
                    ))),
                    update_escape_hint.run_if(in_state(AppState::InSession)),
                ),
                (
                    minigame_settings::update_setting_values,
                    minigame_settings::close_on_escape,
                )
                    .run_if(any_with_component::<minigame_settings::MinigameSettingsScreen>),
                (
                    lobby::update_player_list,
                    lobby::update_swatches,
                    lobby::update_lobby_hint,
                    lobby::update_settings_summaries,
                    lobby::dim_disabled_thumbnails,
                    lobby::update_thumbnails,
                    lobby::update_round_picker,
                )
                    .run_if(in_state(SessionPhase::Lobby)),
                results::update_results_screen.run_if(in_state(SessionPhase::Results)),
                standings::update_standings_screen.run_if(
                    in_state(SessionPhase::Standings)
                        .or_else(in_state(SessionPhase::NextRound))
                        .or_else(in_state(SessionPhase::PartyOver)),
                ),
                party::update_next_round_screen.run_if(in_state(SessionPhase::NextRound)),
                party::update_party_over_screen.run_if(in_state(SessionPhase::PartyOver)),
                (
                    next_match::update_next_match_screen,
                    next_match::update_bracket_view,
                    next_match::restart_countdown_for_a_new_match,
                )
                    .run_if(in_state(SessionPhase::NextMatch)),
                widgets::update_countdowns,
                (widgets::vote_skip_on_input, widgets::update_skip_hints)
                    .run_if(any_with_component::<widgets::SkipHint>),
                notices::expire_notices,
                (
                    watching::update_watching_text,
                    standings::update_standings_overlay,
                )
                    .run_if(in_state(AppState::InSession)),
                close_session.run_if(on_message::<WindowCloseRequested>),
            ),
        );
    }
}

/// Puts [`UI_FONT`] in place of Bevy's default font, which every text uses unless it picks
/// another. Must run before `bevy_text` first reads the fonts (in `PostUpdate`): it doesn't notice
/// a font replaced later.
fn use_ui_font(mut fonts: ResMut<Assets<Font>>) {
    if let Err(error) = fonts.insert(AssetId::default(), Font::from_bytes(UI_FONT.to_vec())) {
        error!("Couldn't use the UI font, keeping Bevy's: {error}");
        return;
    }
    debug!("Using Fira Sans for all text");
}

/// Main-menu fields read when the Join button is activated.
#[derive(SystemParam)]
struct JoinButtonFields<'w, 's> {
    address: Query<'w, 's, &'static EditableText, With<AddressInput>>,
    last_address: ResMut<'w, LastAddress>,
    message: Query<'w, 's, &'static mut Text, With<MenuMessage>>,
}

fn on_menu_button(
    trigger: On<Activate>,
    buttons: Query<&MenuButton>,
    mut join_fields: JoinButtonFields,
    volume_check: Option<Res<sound::VolumeCheckSound>>,
    rebinding: Option<Res<controls::Rebinding>>,
    mut commands: Commands,
) {
    // While waiting for a binding, ignore button clicks so binding `Mouse left` can't also
    // activate whatever button sits under the cursor.
    if rebinding.is_some() {
        return;
    }
    let Ok(button) = buttons.get(trigger.entity) else {
        return;
    };
    match button {
        // A failure leaves the app in this menu, and `show_session_end_reason` shows why.
        MenuButton::Host => commands.queue_handled(HostSession { addr: HOST_ADDR }, warn),
        MenuButton::Join => {
            let typed = join_fields
                .address
                .single()
                .map(|text| text.value().to_string())
                .unwrap_or_default();
            main_menu::join(
                &typed,
                &mut join_fields.last_address,
                &mut commands,
                &mut join_fields.message,
            );
        }
        MenuButton::Quit => {
            commands.write_message(AppExit::Success);
        }
        MenuButton::Cancel => commands.queue(LeaveSession),
        MenuButton::Start(game) => commands.queue(StartMinigame(*game)),
        MenuButton::StartParty => commands.queue(StartParty),
        MenuButton::ChangeRounds(change) => commands.queue(ChangeRounds(*change)),
        MenuButton::Controls => commands.queue(controls::OpenControls),
        MenuButton::CloseControls => commands.queue(controls::CloseControls),
        MenuButton::ResetControls => commands.queue(controls::ResetControls),
        MenuButton::Rebind(action, slot) => commands.queue(controls::StartRebinding {
            action: *action,
            slot: *slot,
            button: trigger.entity,
        }),
        MenuButton::MinigameSettings(game) => {
            commands.queue(minigame_settings::OpenMinigameSettings(*game));
        }
        MenuButton::CloseMinigameSettings => {
            commands.queue(minigame_settings::CloseMinigameSettings);
        }
        MenuButton::ChangeSetting(setting, by) => commands.queue(ChangeSetting {
            setting: *setting,
            by: *by,
        }),
        MenuButton::Sound => commands.queue(sound::OpenSound),
        MenuButton::CloseSound => commands.queue(sound::CloseSound),
        MenuButton::ChangeVolume(control, steps) => {
            commands.queue(ChangeVolume {
                control: *control,
                steps: *steps,
            });
            // Queued after the change, so it plays at the new volume.
            sound::check_volume(*control, volume_check.as_deref(), &mut commands);
        }
    }
}

/// Remember changed controls for the next launch. Separate from changing them, so tests of the
/// Controls screen never write to the developer's settings.
fn save_controls(controls: Res<Controls>) {
    controls.save();
}

/// Remember changed volumes for the next launch. Separate from changing them, like
/// [`save_controls`].
fn save_volume(settings: Res<VolumeSettings>) {
    settings.save();
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

/// What `Esc` does in a session (or while connecting to one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeAction {
    /// Leave the session, for the main menu.
    Leave,
    /// The host takes everyone back to the lobby: see [`ReturnToLobby`].
    BackToLobby,
}

impl EscapeAction {
    /// The hint in the corner saying what `Esc` does.
    pub fn hint(self) -> &'static str {
        match self {
            EscapeAction::Leave => "Esc: leave",
            EscapeAction::BackToLobby => "Esc: back to lobby",
        }
    }
}

/// What `Esc` does for a peer that's `hosting` (or not), with the session in `phase` (`None`
/// while connecting): the host takes everyone back to the lobby from anywhere else in the session;
/// in the lobby, or for a joiner, it leaves.
pub fn escape_action(hosting: bool, phase: Option<SessionPhase>) -> EscapeAction {
    match phase {
        Some(phase) if hosting && phase != SessionPhase::Lobby => EscapeAction::BackToLobby,
        _ => EscapeAction::Leave,
    }
}

/// The hint in the corner saying what `Esc` does (see [`escape_action`]).
#[derive(Component)]
struct EscapeHint;

fn spawn_leave_hint(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::InSession),
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(8),
            ..default()
        },
        children![(
            EscapeHint,
            label(EscapeAction::Leave.hint(), 16.0, HINT_COLOR)
        )],
    ));
}

fn update_escape_hint(
    hosting: Hosting,
    phase: Option<Res<State<SessionPhase>>>,
    mut hint: Single<&mut Text, With<EscapeHint>>,
) {
    let action = escape_action(hosting.is_hosting(), phase.map(|phase| *phase.get()));
    if hint.0 != action.hint() {
        hint.0 = action.hint().into();
    }
}

fn escape_session(
    keys: Res<ButtonInput<KeyCode>>,
    hosting: Hosting,
    phase: Option<Res<State<SessionPhase>>>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match escape_action(hosting.is_hosting(), phase.map(|phase| *phase.get())) {
        EscapeAction::Leave => commands.queue(LeaveSession),
        EscapeAction::BackToLobby => commands.queue(ReturnToLobby),
    }
}

#[cfg(test)]
mod tests {
    use super::{EscapeAction, escape_action};
    use crate::minigames::Minigame;
    use crate::session::SessionPhase;

    #[test]
    fn the_host_goes_back_to_the_lobby_from_anywhere_else_in_the_session() {
        for phase in [
            SessionPhase::Playing(Minigame::Racing),
            SessionPhase::NextRound,
            SessionPhase::NextMatch,
            SessionPhase::Results,
            SessionPhase::Standings,
            SessionPhase::PartyOver,
        ] {
            assert_eq!(
                escape_action(true, Some(phase)),
                EscapeAction::BackToLobby,
                "{phase:?}"
            );
        }
    }

    #[test]
    fn escape_leaves_from_the_lobby_for_joiners_and_while_connecting() {
        assert_eq!(
            escape_action(true, Some(SessionPhase::Lobby)),
            EscapeAction::Leave
        );
        assert_eq!(
            escape_action(false, Some(SessionPhase::Playing(Minigame::Racing))),
            EscapeAction::Leave
        );
        assert_eq!(escape_action(false, None), EscapeAction::Leave);
        assert_eq!(escape_action(true, None), EscapeAction::Leave);
    }
}
