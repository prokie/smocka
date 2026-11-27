//! When a shootout ends. So far only when players leave: with fewer than two gunners left there's
//! nobody to play against, and the host abandons it.

use bevy::prelude::*;

use super::PHASE;
use super::gunner::Gunner;
use crate::minigames::outcome::MinigameEnded;
use crate::network::is_host;

/// Host only: ends the shootout.
pub struct RoundPlugin;

impl Plugin for RoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            abandon_without_two_gunners
                .run_if(is_host)
                .run_if(in_state(PHASE)),
        );
    }
}

/// A gunner is despawned when its player leaves. With fewer than two left, abandon the shootout.
/// Triggering [`MinigameEnded`] again on later frames, before the phase changes, does nothing.
fn abandon_without_two_gunners(
    gunners: Query<&Gunner>,
    mut done: Local<bool>,
    mut commands: Commands,
) {
    let left = gunners.iter().len();
    if left >= 2 {
        *done = false;
        return;
    }
    if *done {
        return;
    }
    *done = true;
    match gunners.iter().next() {
        Some(gunner) => info!(
            "Abandoning the shootout: every other player left, leaving {} ({:?}) alone",
            gunner.name, gunner.owner
        ),
        None => info!("Abandoning the shootout: every player has left"),
    }
    commands.trigger(MinigameEnded::Abandoned);
}
