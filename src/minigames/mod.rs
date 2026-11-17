mod four_player_pong;
pub mod two_player_pong;

use bevy::prelude::*;

pub struct MinigamesPlugin;

impl Plugin for MinigamesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(two_player_pong::TwoPlayerPongPlugin);
    }
}

/// Sprites and other local-only visuals for every minigame.
///
/// Kept separate from [`MinigamesPlugin`] because it needs a renderer and an asset server.
/// The binary adds it next to [`CameraPlugin`](crate::camera::CameraPlugin), so tests can stay headless.
pub struct MinigameVisualsPlugin;

impl Plugin for MinigameVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(two_player_pong::PongVisualsPlugin);
    }
}
