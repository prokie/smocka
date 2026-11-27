//! Pong Royale: Pong for up to four, one player defending each side of a square arena. Letting
//! the ball into your goal costs a life; lose them all and a wall goes up where you stood. The
//! last player with lives left wins.
//!
//! With fewer than four players, the empty sides are walls from the start (see [`arena::seats`]).
//! Like Pong, the host simulates the ball, and each player predicts their own paddle.

pub mod arena;
pub mod config;
pub mod paddles;
pub mod play;
mod royale_input;
pub mod sounds;
mod visuals;

pub use royale_input::RoyaleInput;
pub use visuals::RoyaleVisualsPlugin;

use std::time::Duration;

use bevy::prelude::*;

use super::countdown::{START_COUNTDOWN, has_started};
use super::settings::{Setting, SettingId};
use super::{Minigame, MinigameInfo};
use crate::session::SessionPhase;
use arena::Side;
use config::{LIVES, MAX_LIVES, MIN_LIVES};

/// The phase a game of Pong Royale is played in.
pub const PHASE: SessionPhase = SessionPhase::Playing(Minigame::PongRoyale);

/// What the session and the menus need to know about Pong Royale. Up to a player per side; with
/// more here, the rest watch.
pub const INFO: MinigameInfo = MinigameInfo {
    name: "Pong Royale",
    min_players: 2,
    max_players: Side::ALL.len(),
    thumbnail: "minigames/pong_royale/thumbnail.png",
    settings: &[Setting {
        name: "Lives",
        min: MIN_LIVES,
        max: MAX_LIVES,
        default: LIVES,
        labels: &[],
    }],
};

/// How many lives each player starts with, picked by the host (see [`settings`](super::settings)).
pub const LIVES_SETTING: SettingId = SettingId::new(Minigame::PongRoyale, 0);

/// Host only: how the next game is timed.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoyaleRules {
    /// How long the paddles and the ball wait before play starts. Defaults to
    /// [`START_COUNTDOWN`]; tests shorten it.
    pub countdown: Duration,
}

impl Default for RoyaleRules {
    fn default() -> Self {
        Self {
            countdown: START_COUNTDOWN,
        }
    }
}

/// The order Pong Royale's game logic runs in each `FixedUpdate` tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RoyaleSystems {
    /// Move the paddles from this tick's input.
    MovePaddles,
    /// Host only: move the ball, and take a life from whoever let it into their goal.
    MoveBall,
    /// Host only: wall off sides nobody defends any more, and end the game once one player is
    /// left.
    Finish,
}

/// Registers Pong Royale's replicated components, input and game logic.
pub struct PongRoyalePlugin;

impl Plugin for PongRoyalePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedUpdate,
            (
                RoyaleSystems::MovePaddles,
                RoyaleSystems::MoveBall,
                RoyaleSystems::Finish,
            )
                .chain()
                .run_if(in_state(PHASE)),
        );
        // Neither the paddles nor the ball move until the countdown is over.
        app.configure_sets(
            FixedUpdate,
            (RoyaleSystems::MovePaddles, RoyaleSystems::MoveBall).distributive_run_if(has_started),
        );
        app.init_resource::<RoyaleRules>();

        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((
            paddles::PaddlePlugin,
            play::PlayPlugin,
            royale_input::RoyaleInputPlugin,
        ));
    }
}
