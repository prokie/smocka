//! Shootout: a top-down free-for-all in an arena with cover. The last player standing wins.
//!
//! Built a step at a time. So far: the arena, and a gunner per player that walks and aims around
//! it.

pub mod config;
pub mod gunner;
pub mod movement;
mod round;
mod shootout_input;
mod visuals;

pub use shootout_input::ShootoutInput;
pub use visuals::ShootoutVisualsPlugin;

use bevy::prelude::*;

use super::{Minigame, MinigameInfo};
use crate::session::SessionPhase;
use config::SPAWNS;

/// The phase a shootout is played in.
pub const PHASE: SessionPhase = SessionPhase::Playing(Minigame::Shootout);

/// What the session and the menus need to know about the shootout. Everyone plays, up to a
/// player per spawn point.
pub const INFO: MinigameInfo = MinigameInfo {
    name: "Shootout",
    min_players: 2,
    max_players: SPAWNS.len(),
    thumbnail: "minigames/shootout/thumbnail.png",
    settings: &[],
};

/// The order the shootout's game logic runs in each `FixedUpdate` tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShootoutSystems {
    /// Walk and aim the gunners from this tick's input.
    Walk,
}

/// Registers the shootout's replicated components, input and game logic.
pub struct ShootoutPlugin;

impl Plugin for ShootoutPlugin {
    fn build(&self, app: &mut App) {
        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((
            gunner::GunnerPlugin,
            shootout_input::ShootoutInputPlugin,
            movement::MovementPlugin,
            round::RoundPlugin,
        ));
    }
}
