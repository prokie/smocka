//! Shootout: a top-down free-for-all in an arena with cover. The last player standing wins.
//!
//! Built a step at a time. So far: the arena, and a gunner per player at its own spawn point.

pub mod config;
pub mod gunner;
mod round;
mod visuals;

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

/// Registers the shootout's replicated components and game logic.
pub struct ShootoutPlugin;

impl Plugin for ShootoutPlugin {
    fn build(&self, app: &mut App) {
        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((gunner::GunnerPlugin, round::RoundPlugin));
    }
}
