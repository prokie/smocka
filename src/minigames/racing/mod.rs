//! Racing: every player drives a car around a single-screen top-down track, and the first to
//! finish the laps wins.
//!
//! Not playable yet: starting it enters [`SessionPhase::Racing`](crate::session::SessionPhase),
//! which has nothing in it so far. Press `Esc` to leave.

use bevy::prelude::*;

/// Registers racing's replicated components, input and game logic.
pub struct RacingPlugin;

impl Plugin for RacingPlugin {
    fn build(&self, _app: &mut App) {}
}
