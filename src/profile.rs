//! This player's own profile: the name they picked in the main menu, the address they last
//! joined, their [`Controls`] and their [`VolumeSettings`], all remembered between launches in
//! `smocka/settings.toml` in the system's settings folder.
//!
//! A joiner sends the name to the host inside its netcode connect token (the token's "user data"),
//! so the host knows it the moment the player connects. The host uses its own profile directly.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::network::SERVER_ADDR;
use crate::controls::Controls;
use crate::sound::VolumeSettings;

/// The longest name allowed, in characters.
pub const MAX_NAME_CHARS: usize = 16;

/// Bytes available for the name in a connect token (netcode's `USER_DATA_BYTES`).
const USER_DATA_BYTES: usize = 256;

/// This app's player profile. Empty means "no name picked": the host then calls the player
/// "Player N".
///
/// Starts empty. Only the main menu loads and saves it (see [`LocalProfile::load`]), so headless
/// tests never depend on a name saved on the developer's machine.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct LocalProfile {
    pub name: String,
}

impl LocalProfile {
    /// The profile saved by an earlier run, or an empty one.
    pub fn load() -> Self {
        Self {
            name: clean_name(&SavedSettings::load().name),
        }
    }

    /// Save the profile for the next run. Failing to save is logged, not fatal.
    pub fn save(&self) {
        SavedSettings::update("player name", |saved| saved.name.clone_from(&self.name));
    }
}

/// The address last typed into the main menu's address field, shown there again when the menu
/// comes back (for example after a join that failed) and on the next launch.
///
/// Defaults to a host on this machine. Like [`LocalProfile`], only the main menu loads and saves
/// it.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct LastAddress(pub String);

impl Default for LastAddress {
    fn default() -> Self {
        Self(SERVER_ADDR.to_string())
    }
}

impl LastAddress {
    /// The address saved by an earlier run, or the default.
    pub fn load() -> Self {
        SavedSettings::load()
            .last_address
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .map_or_else(Self::default, Self)
    }

    /// Save the address for the next run. Failing to save is logged, not fatal.
    pub fn save(&self) {
        SavedSettings::update("last address", |saved| {
            saved.last_address = Some(self.0.clone());
        });
    }
}

impl Controls {
    /// The controls saved by an earlier run (made safe, see [`Controls::sanitized`]), or the
    /// defaults. Like [`LocalProfile`], only the main menu loads and saves them.
    pub fn load() -> Self {
        SavedSettings::load().controls.sanitized()
    }

    /// Save the controls for the next run. Failing to save is logged, not fatal.
    pub fn save(&self) {
        SavedSettings::update("controls", |saved| saved.controls.clone_from(self));
    }
}

impl VolumeSettings {
    /// The volumes saved by an earlier run (made safe, see [`VolumeSettings::sanitized`]), or the
    /// defaults. Like [`LocalProfile`], only the main menu loads and saves them.
    pub fn load() -> Self {
        SavedSettings::load().volume.sanitized()
    }

    /// Save the volumes for the next run. Failing to save is logged, not fatal.
    pub fn save(&self) {
        SavedSettings::update("volume", |saved| saved.volume = *self);
    }
}

/// The file everything is saved to, in [`settings_dir`].
const SETTINGS_FILE: &str = "settings.toml";
/// Where versions up to 0.1.1 saved the name, read when there's no [`SETTINGS_FILE`] yet.
const LEGACY_NAME_FILE: &str = "name.txt";

/// Everything remembered between launches, saved as TOML. A missing field takes its default, so
/// files from older versions still load.
#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq, Eq)]
#[serde(default)]
struct SavedSettings {
    /// The player's name, as typed.
    name: String,
    /// The address last joined from the main menu, as typed.
    last_address: Option<String>,
    /// What each action in each minigame is bound to.
    controls: Controls,
    /// How loud sound and music are.
    volume: VolumeSettings,
}

impl SavedSettings {
    /// What was saved in the system's settings folder, or defaults.
    fn load() -> Self {
        settings_dir()
            .map(|dir| Self::read(&dir))
            .unwrap_or_default()
    }

    /// Change one setting in the saved file, keeping the others. `what` names it in the log.
    fn update(what: &str, change: impl FnOnce(&mut Self)) {
        let Some(dir) = settings_dir() else {
            warn!("Can't save the {what}: no settings folder on this system");
            return;
        };
        let mut saved = Self::read(&dir);
        change(&mut saved);
        match saved.write(&dir) {
            Ok(path) => debug!("Saved the {what} to {}", path.display()),
            Err(error) => warn!("Couldn't save the {what} to {}: {error}", dir.display()),
        }
    }

    /// What was saved in `dir`, or defaults. A file that doesn't parse is logged and ignored: the
    /// next save replaces it.
    fn read(dir: &Path) -> Self {
        let path = dir.join(SETTINGS_FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|error| {
                warn!(
                    "Ignoring unreadable settings in {}: {error}",
                    path.display()
                );
                Self::default()
            }),
            Err(_) => Self {
                name: std::fs::read_to_string(dir.join(LEGACY_NAME_FILE)).unwrap_or_default(),
                ..Self::default()
            },
        }
    }

    /// Write to `dir`, creating it if needed, and return the file's path.
    fn write(&self, dir: &Path) -> std::io::Result<PathBuf> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let path = dir.join(SETTINGS_FILE);
        std::fs::create_dir_all(dir)?;
        std::fs::write(&path, text)?;
        Ok(path)
    }
}

/// Where settings are saved: a `smocka` folder in the system's settings folder (for example
/// `~/.config` on Linux, `%APPDATA%` on Windows).
fn settings_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("smocka"))
}

/// A typed name made safe to show: control characters removed, spaces collapsed and trimmed, and
/// cut to [`MAX_NAME_CHARS`]. May be empty.
pub fn clean_name(typed: &str) -> String {
    let words: Vec<&str> = typed
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|word| !word.is_empty())
        .collect();
    let name: String = words.join(" ").chars().take(MAX_NAME_CHARS).collect();
    name.trim_end().to_string()
}

/// `name` packed into a connect token's user data: UTF-8, padded with zeros.
pub fn name_to_user_data(name: &str) -> [u8; USER_DATA_BYTES] {
    let mut data = [0; USER_DATA_BYTES];
    let name = clean_name(name);
    let bytes = name.as_bytes();
    data[..bytes.len()].copy_from_slice(bytes);
    data
}

/// The name packed by [`name_to_user_data`], cleaned again since it came from another machine.
pub fn name_from_user_data(data: &[u8]) -> String {
    let end = data
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(data.len());
    clean_name(&String::from_utf8_lossy(&data[..end]))
}

#[cfg(test)]
mod tests {
    use super::{
        LEGACY_NAME_FILE, MAX_NAME_CHARS, SETTINGS_FILE, SavedSettings, clean_name,
        name_from_user_data, name_to_user_data,
    };
    use crate::controls::{Control, ControlAction, Controls};
    use crate::sound::VolumeSettings;
    use bevy::prelude::GamepadButton;
    use std::path::PathBuf;

    /// An empty folder of its own for one test, under the system's temp folder.
    fn empty_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("smocka-{test}-{}", std::process::id()));
        // Left over from an earlier run, if it's there at all.
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn settings_survive_a_save() {
        let dir = empty_dir("settings-round-trip");
        let mut controls = Controls::default();
        controls.bind(
            ControlAction::Throttle,
            0,
            Control::Button(GamepadButton::South),
        );
        let saved = SavedSettings {
            name: "Åsa \"Ö\"".into(),
            last_address: Some("192.168.1.42".into()),
            controls,
            volume: VolumeSettings {
                master: 7,
                effects: 0,
                music: 3,
            },
        };
        saved.write(&dir).unwrap();
        assert_eq!(SavedSettings::read(&dir), saved);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_or_broken_settings_load_as_defaults() {
        let dir = empty_dir("settings-defaults");
        assert_eq!(SavedSettings::read(&dir), SavedSettings::default());

        std::fs::write(dir.join(SETTINGS_FILE), "name = \"Ada\"\n").unwrap();
        let saved = SavedSettings::read(&dir);
        assert_eq!((saved.name.as_str(), saved.last_address), ("Ada", None));

        std::fs::write(dir.join(SETTINGS_FILE), "this isn't toml").unwrap();
        assert_eq!(SavedSettings::read(&dir), SavedSettings::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_name_saved_by_an_older_version_is_kept() {
        let dir = empty_dir("settings-legacy-name");
        std::fs::write(dir.join(LEGACY_NAME_FILE), "Pontus").unwrap();
        assert_eq!(SavedSettings::read(&dir).name, "Pontus");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names_are_trimmed_collapsed_and_cut() {
        assert_eq!(clean_name("  Ada   Lovelace \n"), "Ada Lovelace");
        assert_eq!(clean_name("tab\tand\u{7}bell"), "tab and bell");
        assert_eq!(clean_name("   "), "");
        let long = "x".repeat(40);
        assert_eq!(clean_name(&long).chars().count(), MAX_NAME_CHARS);
        // Cutting doesn't leave a trailing space.
        assert_eq!(clean_name("fifteen letters x"), "fifteen letters");
    }

    #[test]
    fn names_survive_the_connect_token() {
        for name in ["Pontus", "Åsa ÖÄ", "🚗 racer", ""] {
            assert_eq!(name_from_user_data(&name_to_user_data(name)), name);
        }
    }

    #[test]
    fn garbage_user_data_becomes_a_safe_name() {
        let mut data = [0u8; 256];
        data[..4].copy_from_slice(&[0xff, b'a', 0x07, b'b']);
        assert_eq!(name_from_user_data(&data), "\u{fffd}a b");
    }
}
