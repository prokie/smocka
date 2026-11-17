//! Two-player Pong: each player moves a paddle on their side of the arena.

pub mod paddles;
mod pong_input;
mod visuals;

pub use visuals::PongVisualsPlugin;

use bevy::prelude::*;

pub struct TwoPlayerPongPlugin;

impl Plugin for TwoPlayerPongPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((paddles::PaddlePlugin, pong_input::PongInputPlugin));
    }
}
