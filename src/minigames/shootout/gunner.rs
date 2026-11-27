//! Each player's figure in the arena: a gunner. The host spawns one per participant, at its own
//! spawn point. A joiner predicts their own gunner and interpolates everyone else's. The host
//! does neither: it holds every gunner as it really is.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PHASE;
use super::config::SPAWNS;
use crate::minigames::outcome::Participants;
use crate::network::is_host;
use crate::players::{Player, local_peer_id};

/// Who plays this gunner. Set once at spawn, copied from the [`Player`] so it keeps them even if
/// the player leaves.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Gunner {
    pub owner: PeerId,
    pub name: String,
    /// The player's [`Player::color`], an index for [`player_color`](crate::players::player_color).
    pub color: u8,
}

/// Where a gunner is and which way it aims. Predicted by its own player, interpolated for
/// everyone else.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct GunnerState {
    /// Centre of the gunner, in world units.
    pub position: Vec2,
    /// Which way it aims, in radians anticlockwise from facing right (+x). In `-π..π`.
    pub aim: f32,
}

impl GunnerState {
    /// A gunner standing at `position`, aiming at the middle of the arena.
    pub fn at(position: Vec2) -> Self {
        Self {
            position,
            aim: (-position).to_angle(),
        }
    }
}

impl Ease for GunnerState {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| GunnerState {
            position: start.position.lerp(end.position, t),
            aim: lerp_angle(start.aim, end.aim, t),
        })
    }
}

/// Wrap `angle` into `-π..π`.
pub fn normalize_angle(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

/// From `start` to `end` the short way round, `t` of the way (0 to 1). In `-π..π`.
pub fn lerp_angle(start: f32, end: f32, t: f32) -> f32 {
    let turn = normalize_angle(end - start);
    normalize_angle(start + turn * t)
}

/// Registers the replicated gunner components and spawns the gunners.
pub struct GunnerPlugin;

impl Plugin for GunnerPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Gunner>().replicate();
        app.component::<GunnerState>()
            .replicate()
            .predict()
            .add_linear_interpolation();

        app.add_systems(OnEnter(PHASE), spawn_gunners.run_if(is_host));
    }
}

/// Host only: one gunner per participant still here, each on its own spawn point.
///
/// The host's own player is left out of [`InterpolationTarget`]: otherwise the host marks other
/// players' gunners [`Interpolated`] for itself, though it never interpolates them, and systems
/// that skip interpolated gunners (walking, smoothing) would skip them on the host too.
fn spawn_gunners(
    mut commands: Commands,
    participants: Single<&Participants>,
    players: Query<(&Player, &ControlledBy)>,
    host: Query<&LocalId, (With<Client>, With<Connected>)>,
) {
    let host = local_peer_id(&host);
    let here: Vec<_> = participants
        .0
        .iter()
        .filter_map(|id| players.iter().find(|(player, _)| player.id == *id))
        .collect();
    info!(
        "Shootout between {} players: {:?}",
        here.len(),
        here.iter()
            .map(|(player, _)| player.name())
            .collect::<Vec<_>>()
    );
    for ((player, link), &spawn) in here.into_iter().zip(SPAWNS.iter()) {
        let owner = player.id;
        commands.spawn((
            Gunner {
                owner,
                name: player.name.clone(),
                color: player.color,
            },
            GunnerState::at(spawn),
            DespawnOnExit(PHASE),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::Single(owner)),
            InterpolationTarget::to_clients(NetworkTarget::from_exclude(
                [Some(owner), host].into_iter().flatten(),
            )),
            // Despawned when its player's client disconnects.
            ControlledBy {
                owner: link.owner,
                lifetime: Lifetime::default(),
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    use bevy::math::Vec2;

    use super::{GunnerState, lerp_angle, normalize_angle};

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn angles_wrap_into_range() {
        assert!(close(normalize_angle(0.0), 0.0));
        assert!(close(normalize_angle(TAU + FRAC_PI_2), FRAC_PI_2));
        assert!(close(normalize_angle(-TAU - FRAC_PI_2), -FRAC_PI_2));
        assert!(close(normalize_angle(PI), -PI));
    }

    #[test]
    fn a_new_gunner_aims_at_the_middle() {
        assert!(close(GunnerState::at(Vec2::new(-100.0, 0.0)).aim, 0.0));
        assert!(close(GunnerState::at(Vec2::new(0.0, -50.0)).aim, FRAC_PI_2));
    }

    #[test]
    fn aim_turns_the_short_way_round() {
        // From just below "left" to just above it: through left, not through right.
        let halfway = lerp_angle(PI - 0.1, -PI + 0.1, 0.5);
        assert!(close(halfway.abs(), PI), "{halfway}");
        assert!(close(lerp_angle(0.0, FRAC_PI_2, 0.5), FRAC_PI_2 / 2.0));
    }
}
