//! Two-player Pong: each player moves a paddle on their side of the arena.

pub mod ball;
mod collisions;
pub mod paddles;
mod pong_input;
mod visuals;

pub use visuals::PongVisualsPlugin;

use bevy::prelude::*;

pub struct TwoPlayerPongPlugin;

impl Plugin for TwoPlayerPongPlugin {
    fn build(&self, app: &mut App) {
        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((
            paddles::PaddlePlugin,
            ball::BallPlugin,
            pong_input::PongInputPlugin,
        ));
    }
}
