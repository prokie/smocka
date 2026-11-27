//! Shootout: a top-down free-for-all in an arena with cover. Players walk, aim and shoot
//! dodgeable paint balls until one gunner is left standing.

pub mod config;
pub mod gunner;
pub mod movement;
pub mod pickups;
pub mod round;
mod shootout_input;
pub mod shots;
pub mod sounds;
mod visuals;
pub mod zone;

pub use shootout_input::ShootoutInput;
pub use visuals::ShootoutVisualsPlugin;

use bevy::prelude::*;

use super::countdown::has_started;
use super::settings::{Setting, SettingId};
use super::{Minigame, MinigameInfo};
use crate::session::SessionPhase;
use config::{HITS_TO_KNOCK_OUT, MAX_HITS_TO_KNOCK_OUT, MIN_HITS_TO_KNOCK_OUT, SPAWNS};

/// The phase a shootout is played in.
pub const PHASE: SessionPhase = SessionPhase::Playing(Minigame::Shootout);

/// What the session and the menus need to know about the shootout. Everyone plays, up to a
/// player per spawn point.
pub const INFO: MinigameInfo = MinigameInfo {
    name: "Shootout",
    min_players: 2,
    max_players: SPAWNS.len(),
    thumbnail: "minigames/shootout/thumbnail.png",
    settings: &[Setting {
        name: "Hits to knock out",
        min: MIN_HITS_TO_KNOCK_OUT,
        max: MAX_HITS_TO_KNOCK_OUT,
        default: HITS_TO_KNOCK_OUT,
        labels: &[],
    }],
};

/// How many hits knock a gunner out, picked by the host (see [`settings`](super::settings)).
pub const HITS_TO_KNOCK_OUT_SETTING: SettingId = SettingId::new(Minigame::Shootout, 0);

/// The order the shootout's game logic runs in each `FixedUpdate` tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShootoutSystems {
    /// Walk and aim the gunners from this tick's input.
    Walk,
    /// Spawn centre pickups and award them to gunners walking over them.
    Pickups,
    /// Spawn paint balls for gunners firing this tick.
    Fire,
    /// Move paint balls along their lines and resolve wall and gunner hits.
    MoveBalls,
    /// Shrink the sudden-death safe zone and damage gunners outside it.
    Zone,
    /// End the shootout when at most one gunner remains.
    Finish,
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
            pickups::PickupsPlugin,
            shots::ShotsPlugin,
            zone::ZonePlugin,
            round::RoundPlugin,
        ));
        app.configure_sets(
            FixedUpdate,
            (
                ShootoutSystems::Walk,
                ShootoutSystems::Pickups,
                ShootoutSystems::Fire,
                ShootoutSystems::MoveBalls,
                ShootoutSystems::Zone,
                ShootoutSystems::Finish,
            )
                .chain()
                .run_if(in_state(PHASE)),
        );
        // Nobody walks or shoots, and the pickup and sudden-death clocks wait, until the
        // countdown is over. The rest keeps running, so a shootout can still be abandoned then.
        app.configure_sets(
            FixedUpdate,
            (
                ShootoutSystems::Walk,
                ShootoutSystems::Pickups,
                ShootoutSystems::Fire,
                ShootoutSystems::Zone,
            )
                .distributive_run_if(has_started),
        );
    }
}
