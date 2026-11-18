pub mod camera;
pub mod config;
pub mod lobby;
pub mod minigames;
pub mod network;
pub mod session;

use bevy::prelude::*;

pub struct SmockaPlugin;

impl Plugin for SmockaPlugin {
    fn build(&self, app: &mut App) {
        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((
            network::NetworkPlugin,
            lobby::LobbyPlugin,
            session::SessionPlugin,
            minigames::MinigamesPlugin,
        ));
    }
}
