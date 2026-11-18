//! Two-player Pong: each player moves a paddle on their side of the arena, and scores when the
//! ball gets past the other player's paddle.

pub mod ball;
mod collisions;
pub mod paddles;
mod pong_input;
pub mod score;
mod visuals;

pub use visuals::PongVisualsPlugin;

use bevy::prelude::*;

/// The order Pong's game logic runs in each `FixedUpdate` tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PongSystems {
    /// Move the paddles from this tick's input.
    MovePaddles,
    /// Move the ball and bounce it off the walls and this tick's paddle positions.
    MoveBall,
    /// Count a goal if the ball has left the arena, and serve it again.
    Score,
}

/// Registers two-player Pong's replicated components, input and game logic.
pub struct TwoPlayerPongPlugin;

impl Plugin for TwoPlayerPongPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedUpdate,
            (
                PongSystems::MovePaddles,
                PongSystems::MoveBall,
                PongSystems::Score,
            )
                .chain(),
        );

        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((
            paddles::PaddlePlugin,
            ball::BallPlugin,
            score::ScorePlugin,
            pong_input::PongInputPlugin,
        ));
    }
}
