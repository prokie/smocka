pub mod car;
pub mod progress;
pub mod race;
mod racing_input;
pub mod track;
mod visuals;

pub use racing_input::RacingInput;
pub use visuals::RacingVisualsPlugin;

use bevy::prelude::*;

/// The order racing's game logic runs in each `FixedUpdate` tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RacingSystems {
    /// Host only: run the countdown and the finish grace.
    Status,
    /// Move the cars from this tick's input.
    Drive,
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
        ));
    }
}
