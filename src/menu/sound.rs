//! The Sound screen, shown over the main menu: the master, effects and music volumes, each with
//! "-" and "+". Changing the master or effects volume plays a short beep at the new volume.

use bevy::input_focus::tab_navigation::TabGroup;
use bevy::input_focus::{AutoFocus, FocusCause, InputFocus};
use bevy::prelude::*;

use super::MenuButton;
use super::widgets::{HINT_COLOR, label, menu_button, small_button};
use crate::app_state::AppState;
use crate::sound::{VolumeControl, VolumeSettings, play_sound, volume_label};

/// Played at the new volume when the master or effects volume changes, so the player hears it.
const VOLUME_CHECK_SOUND: &str = "menu/volume_check.wav";
const VOLUME_CHECK_VOLUME: f32 = 0.5;
/// How wide the column of volume names is, in pixels.
const NAME_COLUMN_WIDTH: f32 = 120.0;

/// The Sound screen.
#[derive(Component)]
pub struct SoundScreen;

/// Shows one volume on the Sound screen.
#[derive(Component)]
pub struct VolumeText(pub VolumeControl);

/// The beep [`VOLUME_CHECK_SOUND`], loaded at startup so the first change is heard.
#[derive(Resource)]
pub(super) struct VolumeCheckSound(Handle<AudioSource>);

pub(super) fn load_volume_check(assets: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(VolumeCheckSound(assets.load(VOLUME_CHECK_SOUND)));
}

/// After the player changes `control`, beep at the new volume, unless it's the music volume:
/// changing that changes the music itself, if any is playing.
pub(super) fn check_volume(
    control: VolumeControl,
    sound: Option<&VolumeCheckSound>,
    commands: &mut Commands,
) {
    if let (Some(sound), false) = (sound, control == VolumeControl::Music) {
        play_sound(commands, &sound.0, VOLUME_CHECK_VOLUME);
    }
}

/// Command: show the Sound screen over the main menu, unless it's already there.
#[derive(Debug, Clone, Copy)]
pub struct OpenSound;

impl Command for OpenSound {
    type Out = ();

    fn apply(self, world: &mut World) {
        let open = world
            .query_filtered::<(), With<SoundScreen>>()
            .iter(world)
            .next()
            .is_some();
        if open {
            return;
        }
        debug!("Opening the sound screen");
        // Despawn when leaving whichever top-level state opened this screen (MainMenu or InSession).
        let state = world
            .get_resource::<State<AppState>>()
            .map_or(AppState::MainMenu, |state| *state.get());
        world
            .spawn((
                SoundScreen,
                DespawnOnExit(state),
                // `Tab` stays on this screen.
                TabGroup::modal(),
                // Above the in-session Escape menu (`GlobalZIndex(10)`).
                GlobalZIndex(20),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: px(12),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.07, 0.12, 0.96)),
            ))
            .with_children(|screen| {
                screen.spawn(label("Sound", 48.0, Color::WHITE));
                for control in VolumeControl::ALL {
                    screen
                        .spawn(Node {
                            column_gap: px(12),
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                label(control.name(), 24.0, HINT_COLOR),
                                Node {
                                    width: px(NAME_COLUMN_WIDTH),
                                    ..default()
                                },
                            ));
                            let mut down =
                                row.spawn(small_button(MenuButton::ChangeVolume(control, -1), "-"));
                            if control == VolumeControl::ALL[0] {
                                down.insert(AutoFocus);
                            }
                            row.spawn((
                                VolumeText(control),
                                label("", 24.0, Color::WHITE),
                                Node {
                                    min_width: px(80),
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                TextLayout::justify(Justify::Center),
                            ));
                            row.spawn(small_button(MenuButton::ChangeVolume(control, 1), "+"));
                        });
                }
                screen.spawn(menu_button(MenuButton::CloseSound, "Back"));
            });
    }
}

/// Command: close the Sound screen, and give keyboard focus back to the main menu's Sound button.
#[derive(Debug, Clone, Copy)]
pub struct CloseSound;

impl Command for CloseSound {
    type Out = ();

    fn apply(self, world: &mut World) {
        let screens: Vec<Entity> = world
            .query_filtered::<Entity, With<SoundScreen>>()
            .iter(world)
            .collect();
        if screens.is_empty() {
            return;
        }
        for screen in screens {
            world.despawn(screen);
        }
        debug!("Closed the sound screen");
        let button = world
            .query::<(Entity, &MenuButton)>()
            .iter(world)
            .find(|(_, button)| **button == MenuButton::Sound)
            .map(|(entity, _)| entity);
        if let (Some(button), Some(mut focus)) = (button, world.get_resource_mut::<InputFocus>()) {
            focus.set(button, FocusCause::Navigated);
        }
    }
}

/// Show each volume as a percentage.
pub(super) fn update_volume_labels(
    settings: Res<VolumeSettings>,
    mut texts: Query<(&VolumeText, &mut Text)>,
) {
    for (VolumeText(control), mut text) in &mut texts {
        let shown = volume_label(settings.get(*control));
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

/// `Esc` closes the screen.
pub(super) fn close_sound_on_escape(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::Escape) {
        commands.queue(CloseSound);
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;

    use super::{CloseSound, OpenSound, SoundScreen, VolumeText, update_volume_labels};
    use crate::sound::{VOLUME_STEPS, VolumeControl, VolumeSettings};

    fn screens(world: &mut World) -> usize {
        world
            .query_filtered::<(), With<SoundScreen>>()
            .iter(world)
            .count()
    }

    #[test]
    fn opens_once_and_closes() {
        let mut world = World::new();
        world.commands().queue(OpenSound);
        world.commands().queue(OpenSound);
        world.flush();
        assert_eq!(screens(&mut world), 1);
        world.commands().queue(CloseSound);
        world.flush();
        assert_eq!(screens(&mut world), 0);
    }

    #[test]
    fn shows_each_volume() {
        let mut world = World::new();
        world.insert_resource(VolumeSettings {
            master: VOLUME_STEPS,
            effects: VOLUME_STEPS / 2,
            music: 0,
        });
        world.commands().queue(OpenSound);
        world.flush();
        world.run_system_once(update_volume_labels).unwrap();
        let mut shown: Vec<(VolumeControl, String)> = world
            .query::<(&VolumeText, &Text)>()
            .iter(&world)
            .map(|(VolumeText(control), text)| (*control, text.0.clone()))
            .collect();
        shown.sort_by_key(|(control, _)| VolumeControl::ALL.iter().position(|c| c == control));
        assert_eq!(
            shown,
            [
                (VolumeControl::Master, "100%".to_string()),
                (VolumeControl::Effects, "50%".to_string()),
                (VolumeControl::Music, "0%".to_string()),
            ]
        );
    }
}
