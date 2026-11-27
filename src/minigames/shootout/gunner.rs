//! Each player's figure in the arena: a gunner. The host spawns one per participant, at its own
//! spawn point. A joiner predicts their own gunner and interpolates everyone else's. The host
//! does neither: it holds every gunner as it really is.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::config::SPAWNS;
use super::pickups::GunnerBuff;
use super::{HITS_TO_KNOCK_OUT_SETTING, PHASE};
use crate::minigames::outcome::Participants;
use crate::minigames::settings::MinigameSettings;
use crate::network::is_host;
use crate::players::{Player, local_peer_id};
use crate::session::SessionPhase;

/// Local only: triggered on every peer when a gunner is knocked out mid-round (rather than
/// removed on phase exit or when its player disconnects), so audio and visual effects can react
/// without duplicating session or disconnect checks.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct GunnerKnockedOut {
    pub position: Vec2,
    pub color: u8,
}

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

/// How many hits this gunner can still take before being knocked out. Authoritative on the host,
/// replicated to everyone (never predicted: only the host decides hits).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    pub remaining: u32,
    pub max: u32,
}

impl Health {
    /// Full health for a gunner that takes `max` hits to knock out.
    pub fn full(max: u32) -> Self {
        Self {
            remaining: max,
            max,
        }
    }
}

/// The earliest tick on which this gunner may fire again (`None` before its first shot).
/// Predicted by the gunner's owner so it enforces the same cooldown as the host without dirtying
/// the component every tick.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NextShot(pub Option<Tick>);

impl NextShot {
    /// Whether the gunner is off cooldown on `tick`.
    pub fn ready(self, tick: Tick) -> bool {
        self.0.is_none_or(|next| tick >= next)
    }
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
        app.component::<Health>().replicate();
        app.component::<NextShot>().replicate().predict();

        app.add_observer(notify_knockout);
        app.add_systems(OnEnter(PHASE), spawn_gunners.run_if(is_host));
    }
}

/// Trigger [`GunnerKnockedOut`] when a gunner is removed mid-round while its player is still in
/// the session and its health was at knockout threshold (skipping disconnects and phase exit).
fn notify_knockout(
    trigger: On<Remove<Gunner>>,
    phase: Option<Res<State<SessionPhase>>>,
    gunners: Query<(&Gunner, Option<&GunnerState>, Option<&Health>)>,
    players: Query<&Player>,
    mut commands: Commands,
) {
    if !phase.is_some_and(|phase| *phase.get() == PHASE) {
        return;
    }
    let Ok((gunner, state, health)) = gunners.get(trigger.entity) else {
        return;
    };
    let still_here = players.iter().any(|player| player.id == gunner.owner);
    let fatal_health = health.is_none_or(|health| health.remaining <= 1);
    if still_here && fatal_health {
        commands.trigger(GunnerKnockedOut {
            position: state.map_or(Vec2::ZERO, |state| state.position),
            color: gunner.color,
        });
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
    settings: Res<MinigameSettings>,
    players: Query<(&Player, &ControlledBy)>,
    host: Query<&LocalId, (With<Client>, With<Connected>)>,
) {
    let host = local_peer_id(&host);
    let hits = settings.get(HITS_TO_KNOCK_OUT_SETTING);
    let here: Vec<_> = participants
        .0
        .iter()
        .filter_map(|id| players.iter().find(|(player, _)| player.id == *id))
        .collect();
    info!(
        "Shootout ({hits} hits to knock out) between {} players: {:?}",
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
            Health::full(hits),
            NextShot::default(),
            GunnerBuff::default(),
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

    #[test]
    fn knockout_event_fires_only_for_mid_round_knockouts() {
        use bevy::prelude::*;
        use lightyear::prelude::PeerId;

        use super::{Gunner, GunnerKnockedOut, Health, PHASE, notify_knockout};
        use crate::players::Player;
        use crate::session::SessionPhase;

        #[derive(Resource, Default)]
        struct Seen(Vec<GunnerKnockedOut>);

        let mut world = World::new();
        world.insert_resource(State::new(PHASE));
        world.init_resource::<Seen>();
        world.add_observer(notify_knockout);
        world.add_observer(|trigger: On<GunnerKnockedOut>, mut seen: ResMut<Seen>| {
            seen.0.push(*trigger.event());
        });

        let owner = PeerId::Netcode(1);
        let player = world
            .spawn(Player {
                id: owner,
                name: "Ada".into(),
                number: 1,
                color: 2,
            })
            .id();

        // Removing a gunner with fatal health while its player is still here fires the event.
        let gunner = world
            .spawn((
                Gunner {
                    owner,
                    name: "Ada".into(),
                    color: 2,
                },
                GunnerState::at(Vec2::new(12.0, -4.0)),
                Health {
                    remaining: 1,
                    max: 3,
                },
            ))
            .id();
        world.despawn(gunner);
        assert_eq!(
            world.resource::<Seen>().0,
            vec![GunnerKnockedOut {
                position: Vec2::new(12.0, -4.0),
                color: 2,
            }]
        );

        // Removing a gunner whose player already left does not fire the event.
        world.despawn(player);
        let disconnected = world
            .spawn((
                Gunner {
                    owner,
                    name: "Ada".into(),
                    color: 2,
                },
                GunnerState::at(Vec2::ZERO),
                Health::full(1),
            ))
            .id();
        world.despawn(disconnected);
        assert_eq!(world.resource::<Seen>().0.len(), 1);

        // Outside the shootout phase, removing a gunner does not fire the event.
        world.insert_resource(State::new(SessionPhase::Results));
        world.spawn(Player {
            id: owner,
            name: "Ada".into(),
            number: 1,
            color: 2,
        });
        let exited = world
            .spawn((
                Gunner {
                    owner,
                    name: "Ada".into(),
                    color: 2,
                },
                GunnerState::at(Vec2::ZERO),
                Health::full(1),
            ))
            .id();
        world.despawn(exited);
        assert_eq!(world.resource::<Seen>().0.len(), 1);
    }
}
