//! A minigame's settings screen, shown over the lobby to the host: one row per setting the
//! minigame lists (see [`settings`](crate::minigames::settings)), each with "-" and "+".

use bevy::input_focus::tab_navigation::TabGroup;
use bevy::input_focus::{AutoFocus, FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;

use super::MenuButton;
use super::lobby::{picker, show_pick};
use super::widgets::{label, menu_button};
use crate::minigames::Minigame;
use crate::minigames::settings::{MinigameSettings, SettingId};
use crate::session::SessionPhase;

/// The settings screen of a minigame.
#[derive(Component, Debug)]
pub struct MinigameSettingsScreen(pub Minigame);

/// Shows the value of one setting on the settings screen.
#[derive(Component, Debug)]
pub struct SettingValue(pub SettingId);

/// Command: show `.0`'s settings screen over the lobby, unless a settings screen is already there.
#[derive(Debug, Clone, Copy)]
pub struct OpenMinigameSettings(pub Minigame);

impl Command for OpenMinigameSettings {
    type Out = ();

    fn apply(self, world: &mut World) {
        let open = world
            .query_filtered::<(), With<MinigameSettingsScreen>>()
            .iter(world)
            .next()
            .is_some();
        if open {
            return;
        }
        let game = self.0;
        debug!("Opening the {} settings screen", game.info().name);
        // Hides the lobby behind it.
        let background = world
            .get_resource::<ClearColor>()
            .cloned()
            .unwrap_or_default()
            .0;
        let screen = world
            .spawn((
                MinigameSettingsScreen(game),
                DespawnOnExit(SessionPhase::Lobby),
                // `Tab` stays on this screen.
                TabGroup::modal(),
                GlobalZIndex(1),
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
                BackgroundColor(background),
            ))
            .with_child(label(
                format!("{} settings", game.info().name),
                48.0,
                Color::WHITE,
            ))
            .id();
        let mut commands = world.commands();
        for (row_index, id) in SettingId::all_of(game).enumerate() {
            let change = move |by| MenuButton::ChangeSetting(id, by);
            let row = picker(
                id.setting().name,
                SettingValue(id),
                Some(change),
                &mut commands,
            );
            if row_index == 0 {
                focus_first_button(row, &mut commands);
            }
            commands.entity(screen).add_child(row);
        }
        commands
            .entity(screen)
            .with_child(menu_button(MenuButton::CloseMinigameSettings, "Back"));
        world.flush();
    }
}

/// Give keyboard focus to the first button in `row` (its "-") once it's spawned.
fn focus_first_button(row: Entity, commands: &mut Commands) {
    commands.queue(move |world: &mut World| {
        let first = world.get::<Children>(row).and_then(|children| {
            children
                .iter()
                .find(|child| world.get::<MenuButton>(*child).is_some())
        });
        if let Some(button) = first {
            world.entity_mut(button).insert(AutoFocus);
        }
    });
}

/// Command: close the settings screen, and give keyboard focus back to the lobby's Settings
/// button for its minigame.
#[derive(Debug, Clone, Copy)]
pub struct CloseMinigameSettings;

impl Command for CloseMinigameSettings {
    type Out = ();

    fn apply(self, world: &mut World) {
        let screens: Vec<(Entity, Minigame)> = world
            .query::<(Entity, &MinigameSettingsScreen)>()
            .iter(world)
            .map(|(entity, screen)| (entity, screen.0))
            .collect();
        let Some(&(_, game)) = screens.first() else {
            return;
        };
        for (screen, _) in screens {
            world.despawn(screen);
        }
        debug!("Closed the {} settings screen", game.info().name);
        let button = world
            .query::<(Entity, &MenuButton)>()
            .iter(world)
            .find(|(_, button)| **button == MenuButton::MinigameSettings(game))
            .map(|(entity, _)| entity);
        if let (Some(button), Some(mut focus)) = (button, world.get_resource_mut::<InputFocus>()) {
            focus.set(button, FocusCause::Navigated);
        }
    }
}

/// Show each setting's value, and disable "-" and "+" at its limits.
pub(super) fn update_setting_values(
    settings: Res<MinigameSettings>,
    mut values: Query<(&SettingValue, &mut Text)>,
    buttons: Query<(Entity, &MenuButton, Has<InteractionDisabled>)>,
    mut commands: Commands,
) {
    for (SettingValue(id), mut text) in &mut values {
        let setting = id.setting();
        let value = settings.get(*id);
        show_pick(
            value,
            setting.label(value),
            (setting.min, setting.max),
            &mut text,
            &buttons,
            &mut commands,
            |action| match action {
                MenuButton::ChangeSetting(button_id, by) if button_id == id => Some(*by),
                _ => None,
            },
        );
    }
}

/// `Esc` closes the screen, instead of leaving the session.
pub(super) fn close_on_escape(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::Escape) {
        commands.queue(CloseMinigameSettings);
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;

    use super::{
        CloseMinigameSettings, MinigameSettingsScreen, OpenMinigameSettings, SettingValue,
        update_setting_values,
    };
    use crate::minigames::Minigame;
    use crate::minigames::racing::track::TrackId;
    use crate::minigames::racing::{LAPS_SETTING, TRACK_SETTING};
    use crate::minigames::settings::MinigameSettings;

    fn screens(world: &mut World) -> usize {
        world
            .query_filtered::<(), With<MinigameSettingsScreen>>()
            .iter(world)
            .count()
    }

    #[test]
    fn opens_once_and_closes() {
        let mut world = World::new();
        world
            .commands()
            .queue(OpenMinigameSettings(Minigame::Racing));
        world
            .commands()
            .queue(OpenMinigameSettings(Minigame::TwoPlayerPong));
        world.flush();
        assert_eq!(screens(&mut world), 1);
        world.commands().queue(CloseMinigameSettings);
        world.flush();
        assert_eq!(screens(&mut world), 0);
    }

    #[test]
    fn shows_one_row_per_setting_with_the_hosts_pick() {
        let mut world = World::new();
        let mut settings = MinigameSettings::default();
        settings.set(LAPS_SETTING, 7);
        settings.set(TRACK_SETTING, TrackId::Oval as u32);
        world.insert_resource(settings);
        world
            .commands()
            .queue(OpenMinigameSettings(Minigame::Racing));
        world.flush();
        world.run_system_once(update_setting_values).unwrap();
        let shown: Vec<String> = world
            .query_filtered::<&Text, With<SettingValue>>()
            .iter(&world)
            .map(|text| text.0.clone())
            .collect();
        assert_eq!(shown, ["7", "Oval"]);
    }
}
