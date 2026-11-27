pub mod ball;
mod collisions;
pub mod config;
pub mod paddles;
mod pong_input;
pub mod score;
pub mod sounds;
mod visuals;

pub use pong_input::PongInput;
pub use visuals::PongVisualsPlugin;

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;

use super::countdown::{START_COUNTDOWN, StartTick, has_started};
use super::settings::{Setting, SettingId};
use super::{Minigame, MinigameInfo};
use crate::network::is_host;
use crate::session::SessionPhase;
use config::{GOALS_TO_WIN, MAX_GOALS_TO_WIN, MIN_GOALS_TO_WIN};

/// The phase a game of Pong is played in.
pub const PHASE: SessionPhase = SessionPhase::Playing(Minigame::TwoPlayerPong);

/// What the session and the menus need to know about Pong. It seats two: with more players
/// here, it's played as a [`Tournament`](crate::tournament::Tournament).
pub const INFO: MinigameInfo = MinigameInfo {
    name: "Pong",
    min_players: 2,
    max_players: 2,
    thumbnail: "minigames/two_player_pong/thumbnail.png",
    settings: &[Setting {
        name: "Goals to win",
        min: MIN_GOALS_TO_WIN,
        max: MAX_GOALS_TO_WIN,
        default: GOALS_TO_WIN,
        labels: &[],
    }],
};

/// How many goals win a game, picked by the host (see [`settings`](super::settings)).
pub const GOALS_TO_WIN_SETTING: SettingId = SettingId::new(Minigame::TwoPlayerPong, 0);

/// Host only: how the next game of Pong is timed.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PongRules {
    /// How long the paddles and the ball wait before play starts. Defaults to
    /// [`START_COUNTDOWN`]; tests shorten it.
    pub countdown: Duration,
}

impl Default for PongRules {
    fn default() -> Self {
        Self {
            countdown: START_COUNTDOWN,
        }
    }
}

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
        // Neither the paddles nor the ball move until the countdown is over. Every game counts
        // down, so each match of a tournament does too.
        app.configure_sets(
            FixedUpdate,
            (PongSystems::MovePaddles, PongSystems::MoveBall).distributive_run_if(has_started),
        );
        app.init_resource::<PongRules>();
        app.add_systems(OnEnter(PHASE), start_game.run_if(is_host));

        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((
            paddles::PaddlePlugin,
            ball::BallPlugin,
            score::ScorePlugin,
            pong_input::PongInputPlugin,
        ));
    }
}

/// Host only: count down to the start of the game (see
/// [`countdown`](crate::minigames::countdown)).
fn start_game(
    rules: Res<PongRules>,
    timeline: Res<LocalTimeline>,
    fixed: Res<Time<Fixed>>,
    mut commands: Commands,
) {
    let start = StartTick::after(timeline.tick(), rules.countdown, fixed.timestep());
    info!(
        "Pong starts in {} s, on tick {}",
        rules.countdown.as_secs_f32().ceil() as u32,
        start.0.0
    );
    commands.spawn((
        start,
        DespawnOnExit(PHASE),
        Replicate::to_clients(NetworkTarget::All),
    ));
}
