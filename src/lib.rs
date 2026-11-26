pub mod app_state;
pub mod camera;
pub mod config;
pub mod menu;
pub mod minigames;
pub mod network;
pub mod players;
pub mod profile;
pub mod session;

use bevy::prelude::*;

pub struct SmockaPlugin;

impl Plugin for SmockaPlugin {
    fn build(&self, app: &mut App) {
        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((
            network::NetworkPlugin,
            app_state::AppStatePlugin,
            players::PlayersPlugin,
            session::SessionPlugin,
            minigames::MinigamesPlugin,
        ));
    }
}
