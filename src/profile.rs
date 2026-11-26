//! This player's own profile: the name they picked in the main menu, remembered between launches.
//!
//! A joiner sends the name to the host inside its netcode connect token (the token's "user data"),
//! so the host knows it the moment the player connects. The host uses its own profile directly.

use std::path::PathBuf;

use bevy::prelude::*;

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
        let name = settings_file()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| clean_name(&text))
            .unwrap_or_default();
        Self { name }
    }

    /// Save the profile for the next run. Failing to save is logged, not fatal.
    pub fn save(&self) {
        let Some(path) = settings_file() else {
            warn!("Can't save the player name: no settings folder on this system");
            return;
        };
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, &self.name));
        match result {
            Ok(()) => debug!("Saved the player name to {}", path.display()),
            Err(error) => warn!(
                "Couldn't save the player name to {}: {error}",
                path.display()
            ),
        }
    }
}

/// Where the name is saved: a `smocka/name.txt` in the system's settings folder (for example
/// `~/.config` on Linux, `%APPDATA%` on Windows).
fn settings_file() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("smocka").join("name.txt"))
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
    use super::{MAX_NAME_CHARS, clean_name, name_from_user_data, name_to_user_data};

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
