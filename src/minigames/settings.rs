//! Minigame settings: numbers the host picks in the lobby before playing, such as how many laps a
//! race has or how many goals win Pong.
//!
//! Each minigame lists its own in [`MinigameInfo::settings`](super::MinigameInfo::settings), and
//! names each one with a [`SettingId`] const in its module. Everything else is shared: the host
//! keeps its picks in [`MinigameSettings`], [`ChangeSetting`] changes one within its limits, and a
//! [`PublishedSettings`] copy is replicated so joiners see the picks too. The lobby gives each
//! minigame with settings its own settings screen.

use std::collections::BTreeMap;

use bevy::prelude::*;
use lightyear::prelude::{server::Started, *};
use serde::{Deserialize, Serialize};

use super::Minigame;
use crate::app_state::AppState;
use crate::network::is_host;

/// A whole number the host picks for a minigame, from `min` to `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setting {
    /// What the lobby calls it, e.g. "Laps".
    pub name: &'static str,
    pub min: u32,
    pub max: u32,
    /// Its value until the host picks another.
    pub default: u32,
    /// What the lobby calls each value, from `min` up, for a setting that picks one of a few
    /// things (such as racing's track). Empty to show the number.
    pub labels: &'static [&'static str],
}

impl Setting {
    /// What the lobby shows for `value`: its label, or the number.
    pub fn label(self, value: u32) -> String {
        value
            .checked_sub(self.min)
            .and_then(|index| self.labels.get(index as usize))
            .map_or_else(|| value.to_string(), |label| (*label).to_string())
    }

    /// Button labels for stepping this setting down and up: `("←", "→")` for named choices (such
    /// as racing's track) and `("-", "+")` for numbers.
    pub const fn step_labels(self) -> (&'static str, &'static str) {
        if self.labels.is_empty() {
            ("-", "+")
        } else {
            ("←", "→")
        }
    }
}

/// Which setting: a minigame, and where the setting is in that minigame's
/// [`settings`](super::MinigameInfo::settings). This is how peers name a setting to each other.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SettingId {
    pub game: Minigame,
    pub index: u8,
}

impl SettingId {
    pub const fn new(game: Minigame, index: u8) -> Self {
        Self { game, index }
    }

    /// The setting this names. Panics if the minigame has no setting at that index: each
    /// minigame's tests check its own.
    pub fn setting(self) -> Setting {
        self.game.info().settings[usize::from(self.index)]
    }

    /// Every setting `game` has, in the order its settings screen shows them.
    pub fn all_of(game: Minigame) -> impl Iterator<Item = SettingId> {
        (0..game.info().settings.len()).map(move |index| SettingId::new(game, index as u8))
    }
}

/// A value for each setting the host has picked. Settings missing from it have their default.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingValues(BTreeMap<SettingId, u32>);

impl SettingValues {
    /// The value of `id`: the one picked, kept within its limits, or its default.
    pub fn get(&self, id: SettingId) -> u32 {
        let setting = id.setting();
        self.0.get(&id).map_or(setting.default, |&value| {
            value.clamp(setting.min, setting.max)
        })
    }

    /// Pick `value` for `id`, kept within its limits, and return what it became.
    pub fn set(&mut self, id: SettingId, value: u32) -> u32 {
        let setting = id.setting();
        let value = value.clamp(setting.min, setting.max);
        self.0.insert(id, value);
        value
    }

    /// What the lobby shows for `game`'s settings, e.g. "Laps: 3 · Track: Oval". Empty if it
    /// has none.
    pub fn summary(&self, game: Minigame) -> String {
        SettingId::all_of(game)
            .map(|id| {
                let setting = id.setting();
                format!("{}: {}", setting.name, setting.label(self.get(id)))
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// Host only: the host's picks, which every minigame is started with. Lasts while the app runs.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Deref, DerefMut)]
pub struct MinigameSettings(pub SettingValues);

/// The host's [`MinigameSettings`], so joiners see them in the lobby and during a minigame. One
/// per session, spawned and kept up to date by the host.
#[derive(Component, Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, Deref)]
pub struct PublishedSettings(pub SettingValues);

/// Command: change setting `setting` by `by`, keeping it within its limits. Only means something
/// on the host, whose [`MinigameSettings`] minigames are started with; the lobby only offers it
/// there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeSetting {
    pub setting: SettingId,
    pub by: i32,
}

impl Command for ChangeSetting {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Some(mut settings) = world.get_resource_mut::<MinigameSettings>() else {
            error!("Can't change a minigame setting: there are no minigame settings");
            return;
        };
        let id = self.setting;
        let setting = id.setting();
        let Setting { name, min, max, .. } = setting;
        let game = id.game.info().name;
        let before = settings.get(id);
        let wanted = before.saturating_add_signed(self.by);
        if settings.set(id, wanted) == before {
            debug!(
                "{game} {name} stays at {}: it goes from {} to {}",
                setting.label(before),
                setting.label(min),
                setting.label(max)
            );
            return;
        }
        info!(
            "{game} {name} {} -> {}",
            setting.label(before),
            setting.label(settings.get(id))
        );
    }
}

/// Registers [`PublishedSettings`] and keeps it up to date on the host.
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.component::<PublishedSettings>().replicate();
        app.init_resource::<MinigameSettings>();
        app.add_observer(publish_settings_on_start);
        app.add_systems(Update, keep_settings_published.run_if(is_host));
    }
}

/// Host only: when the session starts, spawn the [`PublishedSettings`] everyone sees.
fn publish_settings_on_start(
    _trigger: On<Add<Started>>,
    settings: Res<MinigameSettings>,
    mut commands: Commands,
) {
    commands.spawn((
        Name::from("PublishedSettings"),
        PublishedSettings(settings.0.clone()),
        DespawnOnEnter(AppState::MainMenu),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

/// Host only: keep [`PublishedSettings`] up to date with the host's picks.
fn keep_settings_published(
    settings: Res<MinigameSettings>,
    mut published: Single<&mut PublishedSettings>,
) {
    if published.0 != settings.0 {
        published.0.clone_from(&settings.0);
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::{ChangeSetting, MinigameSettings, SettingId, SettingValues};
    use crate::minigames::Minigame;

    #[test]
    fn labelled_settings_name_every_value() {
        for game in Minigame::ALL {
            for id in SettingId::all_of(game) {
                let setting = id.setting();
                if !setting.labels.is_empty() {
                    assert_eq!(
                        setting.labels.len() as u32,
                        setting.max - setting.min + 1,
                        "{} {setting:?}",
                        game.info().name
                    );
                }
            }
        }
    }

    #[test]
    fn a_setting_shows_its_label_or_its_number() {
        let numbered = super::Setting {
            name: "Laps",
            min: 1,
            max: 3,
            default: 1,
            labels: &[],
        };
        assert_eq!(numbered.label(2), "2");
        let labelled = super::Setting {
            labels: &["One", "Two", "Three"],
            ..numbered
        };
        assert_eq!(labelled.label(1), "One");
        assert_eq!(labelled.label(3), "Three");
        // Out of range falls back to the number rather than panicking.
        assert_eq!(labelled.label(9), "9");
    }

    #[test]
    fn every_setting_starts_within_its_limits() {
        for game in Minigame::ALL {
            for id in SettingId::all_of(game) {
                let setting = id.setting();
                assert!(
                    setting.min <= setting.default && setting.default <= setting.max,
                    "{} {setting:?}",
                    game.info().name
                );
            }
        }
    }

    #[test]
    fn unpicked_settings_have_their_default_and_picks_stay_within_limits() {
        for game in Minigame::ALL {
            for id in SettingId::all_of(game) {
                let setting = id.setting();
                let mut values = SettingValues::default();
                assert_eq!(values.get(id), setting.default);
                assert_eq!(values.set(id, u32::MAX), setting.max);
                assert_eq!(values.set(id, 0), setting.min);
                assert_eq!(values.get(id), setting.min);
            }
        }
    }

    #[test]
    fn the_host_changes_a_setting_within_its_limits() {
        let id = SettingId::all_of(Minigame::Racing).next().unwrap();
        let setting = id.setting();
        let mut world = World::new();
        world.insert_resource(MinigameSettings::default());
        let mut change = |by| {
            world.commands().queue(ChangeSetting { setting: id, by });
            world.flush();
            world.resource::<MinigameSettings>().get(id)
        };
        assert_eq!(change(1), setting.default + 1);
        assert_eq!(change(-1), setting.default);
        assert_eq!(change(-1000), setting.min);
        assert_eq!(change(1000), setting.max);
    }

    #[test]
    fn the_summary_lists_a_games_settings() {
        let mut values = SettingValues::default();
        let mut racing = SettingId::all_of(Minigame::Racing);
        let (laps, track) = (racing.next().unwrap(), racing.next().unwrap());
        assert_eq!(laps.setting().step_labels(), ("-", "+"));
        assert_eq!(track.setting().step_labels(), ("←", "→"));
        values.set(laps, 4);
        assert_eq!(
            values.summary(Minigame::Racing),
            "Laps: 4 · Track: Figure eight"
        );
        values.set(track, 1);
        assert_eq!(values.summary(Minigame::Racing), "Laps: 4 · Track: Oval");
        assert_eq!(values.summary(Minigame::TwoPlayerPong), "Goals to win: 5");
    }
}
