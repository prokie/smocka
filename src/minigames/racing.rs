pub mod bumps;
pub mod car;
pub mod config;
pub mod progress;
pub mod race;
mod racing_input;
mod skid_marks;
pub mod sounds;
pub mod track;
mod visuals;

pub use racing_input::RacingInput;
pub use visuals::RacingVisualsPlugin;

use bevy::prelude::*;

use super::settings::{Setting, SettingId};
use super::{Minigame, MinigameInfo};
use crate::session::SessionPhase;
use config::{GRID_SLOTS, LAPS, MAX_LAPS, MIN_LAPS};

/// The phase a race is played in.
pub const PHASE: SessionPhase = SessionPhase::Playing(Minigame::Racing);

/// What the session and the menus need to know about racing. Every player can race, up to the
/// starting grid's [`GRID_SLOTS`].
pub const INFO: MinigameInfo = MinigameInfo {
    name: "Racing",
    min_players: 2,
    max_players: GRID_SLOTS,
    settings: &[Setting {
        name: "Laps",
        min: MIN_LAPS,
        max: MAX_LAPS,
        default: LAPS,
    }],
};

/// How many laps a race has, picked by the host (see [`settings`](super::settings)).
pub const LAPS_SETTING: SettingId = SettingId::new(Minigame::Racing, 0);

/// The order racing's game logic runs in each `FixedUpdate` tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RacingSystems {
    /// Host only: run the countdown and the finish grace.
    Status,
    /// Move the cars from this tick's input.
    Drive,
    /// Push touching cars apart, and bounce cars that hit each other.
    Bump,
    /// Host only: count checkpoints, laps and finishes from where the cars ended up.
    Checkpoints,
    /// Host only: end the race when it's over.
    Finish,
}

/// Registers racing's replicated components, input and game logic.
pub struct RacingPlugin;

impl Plugin for RacingPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedUpdate,
            (
                RacingSystems::Status,
                RacingSystems::Drive,
                RacingSystems::Bump,
                RacingSystems::Checkpoints,
                RacingSystems::Finish,
            )
                .chain(),
        );

        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((
            car::CarPlugin,
            racing_input::RacingInputPlugin,
            progress::ProgressPlugin,
            race::RacePlugin,
            bumps::BumpPlugin,
        ));
    }
}
