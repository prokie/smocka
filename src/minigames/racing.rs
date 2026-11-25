pub mod car;
mod racing_input;
pub mod track;
mod visuals;

pub use racing_input::RacingInput;
pub use visuals::RacingVisualsPlugin;

use bevy::prelude::*;

/// The order racing's game logic runs in each `FixedUpdate` tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RacingSystems {
    /// Move the cars from this tick's input.
    Drive,
}

/// Registers racing's replicated components, input and game logic.
pub struct RacingPlugin;

impl Plugin for RacingPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(FixedUpdate, RacingSystems::Drive);

        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((car::CarPlugin, racing_input::RacingInputPlugin));
    }
}
