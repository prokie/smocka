//! Sudden-death closing border: once a round runs long, the safe zone shrinks inward toward the
//! centre cover and damages gunners who linger outside it.

use std::collections::HashMap;
use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::config::{
    ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH, SAFE_ZONE_MIN_HALF_HEIGHT, SAFE_ZONE_MIN_HALF_WIDTH,
};
use super::gunner::{Gunner, GunnerState, Health};
use super::round::{ShootoutDirector, ShootoutRules};
use super::{PHASE, ShootoutSystems};
use crate::network::is_host;

/// The current sudden-death safe rectangle, centred on `(0, 0)`. One replicated entity per
/// shootout, spawned by the host.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SafeZone {
    pub half_width: f32,
    pub half_height: f32,
    /// `false` until sudden death begins.
    pub active: bool,
}

impl SafeZone {
    /// The full arena floor, inactive until sudden death starts.
    pub fn full() -> Self {
        Self {
            half_width: ARENA_HALF_WIDTH,
            half_height: ARENA_HALF_HEIGHT,
            active: false,
        }
    }

    /// The active safe zone at shrink `progress` (`0.0` at the outer wall, `1.0` at minimum size).
    pub fn at_progress(progress: f32) -> Self {
        let t = progress.clamp(0.0, 1.0);
        Self {
            half_width: ARENA_HALF_WIDTH.lerp(SAFE_ZONE_MIN_HALF_WIDTH, t),
            half_height: ARENA_HALF_HEIGHT.lerp(SAFE_ZONE_MIN_HALF_HEIGHT, t),
            active: true,
        }
    }

    /// Whether a gunner centred on `position` is safely inside the zone.
    pub fn contains(self, position: Vec2) -> bool {
        position.x.abs() <= self.half_width && position.y.abs() <= self.half_height
    }
}

/// Host only: elapsed round time and how long each gunner has continuously stood outside the safe
/// zone.
#[derive(Resource, Debug)]
struct ZoneState {
    elapsed: Duration,
    delay: Duration,
    shrink: Duration,
    damage_interval: Duration,
    outside_time: HashMap<Entity, Duration>,
}

impl ZoneState {
    fn from_rules(rules: &ShootoutRules) -> Self {
        Self {
            elapsed: Duration::ZERO,
            delay: rules.sudden_death_delay,
            shrink: rules.sudden_death_shrink,
            damage_interval: rules.zone_damage_interval,
            outside_time: HashMap::new(),
        }
    }
}

/// Registers [`SafeZone`] and runs sudden-death shrinking and zone damage on the host.
pub struct ZonePlugin;

impl Plugin for ZonePlugin {
    fn build(&self, app: &mut App) {
        app.component::<SafeZone>().replicate();

        app.add_systems(OnEnter(PHASE), start_zone.run_if(is_host));
        app.add_systems(OnExit(PHASE), |mut commands: Commands| {
            commands.remove_resource::<ZoneState>();
        });
        app.add_systems(
            FixedUpdate,
            step_safe_zone
                .in_set(ShootoutSystems::Zone)
                .run_if(is_host)
                .run_if(resource_exists::<ZoneState>)
                .run_if(in_state(PHASE)),
        );
    }
}

fn start_zone(rules: Res<ShootoutRules>, mut commands: Commands) {
    commands.insert_resource(ZoneState::from_rules(&rules));
    commands.spawn((
        SafeZone::full(),
        DespawnOnExit(PHASE),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

/// Host only: shrink [`SafeZone`] once sudden death starts and deal periodic hits to gunners
/// outside it.
fn step_safe_zone(
    time: Res<Time>,
    timeline: Res<LocalTimeline>,
    mut state: ResMut<ZoneState>,
    mut director: Option<ResMut<ShootoutDirector>>,
    mut zones: Query<&mut SafeZone>,
    mut gunners: Query<(Entity, &Gunner, &GunnerState, &mut Health)>,
    mut commands: Commands,
) {
    let dt = time.delta();
    state.elapsed += dt;
    if state.elapsed < state.delay {
        return;
    }

    let Ok(mut zone) = zones.single_mut() else {
        return;
    };
    if !zone.active {
        info!("Shootout sudden death: safe zone is closing in");
    }

    let progress = if state.shrink.is_zero() {
        1.0
    } else {
        ((state.elapsed - state.delay).as_secs_f32() / state.shrink.as_secs_f32()).clamp(0.0, 1.0)
    };
    let next = SafeZone::at_progress(progress);
    if !zone.active
        || (zone.half_width - next.half_width).abs() > 1e-4
        || (zone.half_height - next.half_height).abs() > 1e-4
    {
        *zone = next;
    }

    let current_zone = *zone;
    let tick = timeline.tick();
    let interval = state.damage_interval;
    for (entity, gunner, gunner_state, mut health) in &mut gunners {
        if health.remaining == 0 || current_zone.contains(gunner_state.position) {
            state.outside_time.remove(&entity);
            continue;
        }
        let outside = state.outside_time.entry(entity).or_default();
        *outside += dt;
        while *outside >= interval && health.remaining > 0 {
            *outside -= interval;
            health.remaining = health.remaining.saturating_sub(1);
            debug!(
                "{} ({:?}) took sudden-death zone damage ({}/{} hp left)",
                gunner.name, gunner.owner, health.remaining, health.max
            );
            if health.remaining == 0 {
                info!(
                    "{} ({:?}) knocked out by the sudden-death zone on tick {}",
                    gunner.name, gunner.owner, tick.0
                );
                if let Some(director) = director.as_deref_mut() {
                    director.record_knockout(gunner.clone(), tick);
                }
                commands.entity(entity).despawn();
                state.outside_time.remove(&entity);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use lightyear::prelude::{LocalTimeline, PeerId};

    use super::{SafeZone, ZoneState, step_safe_zone};
    use crate::minigames::shootout::config::{
        ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH, SAFE_ZONE_MIN_HALF_HEIGHT, SAFE_ZONE_MIN_HALF_WIDTH,
    };
    use crate::minigames::shootout::gunner::{Gunner, GunnerState, Health};
    use crate::minigames::shootout::round::{ShootoutDirector, ShootoutRules};

    #[test]
    fn safe_zone_shrinks_linearly_to_minimum_and_checks_containment() {
        let start = SafeZone::at_progress(0.0);
        assert!(start.active);
        assert!((start.half_width - ARENA_HALF_WIDTH).abs() < 1e-4);
        assert!((start.half_height - ARENA_HALF_HEIGHT).abs() < 1e-4);

        let end = SafeZone::at_progress(1.0);
        assert!((end.half_width - SAFE_ZONE_MIN_HALF_WIDTH).abs() < 1e-4);
        assert!((end.half_height - SAFE_ZONE_MIN_HALF_HEIGHT).abs() < 1e-4);
        assert!(end.contains(Vec2::new(SAFE_ZONE_MIN_HALF_WIDTH, 0.0)));
        assert!(!end.contains(Vec2::new(SAFE_ZONE_MIN_HALF_WIDTH + 1.0, 0.0)));
    }

    #[test]
    fn standing_outside_the_shrunk_zone_deals_damage_and_knocks_out() {
        let mut world = World::new();
        let rules = ShootoutRules {
            sudden_death_delay: Duration::ZERO,
            sudden_death_shrink: Duration::ZERO,
            zone_damage_interval: Duration::from_millis(100),
            ..ShootoutRules::default()
        };
        world.insert_resource(ZoneState::from_rules(&rules));
        world.insert_resource(ShootoutDirector::default());
        world.insert_resource(LocalTimeline::default());
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_millis(100));
        world.insert_resource(time);

        world.spawn(SafeZone::full());
        let outside = world
            .spawn((
                Gunner {
                    owner: PeerId::Netcode(1),
                    name: "Ada".into(),
                    color: 0,
                },
                GunnerState::at(Vec2::new(250.0, 0.0)),
                Health::full(2),
            ))
            .id();
        let inside = world
            .spawn((
                Gunner {
                    owner: PeerId::Netcode(2),
                    name: "Bo".into(),
                    color: 1,
                },
                GunnerState::at(Vec2::new(40.0, 0.0)),
                Health::full(2),
            ))
            .id();

        // First tick: outside gunner drops from 2 to 1 HP; inside gunner is untouched.
        world.run_system_once(step_safe_zone).unwrap();
        assert_eq!(world.get::<Health>(outside).unwrap().remaining, 1);
        assert_eq!(world.get::<Health>(inside).unwrap().remaining, 2);

        // Second tick: outside gunner reaches 0 HP and is despawned.
        world.run_system_once(step_safe_zone).unwrap();
        assert!(world.get_entity(outside).is_err());
        assert_eq!(world.get::<Health>(inside).unwrap().remaining, 2);
    }
}
