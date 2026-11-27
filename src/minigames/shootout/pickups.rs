//! Contested centre pickups: Triple Spread, Rapid Fire and Shield.
//!
//! The host spawns one pickup at a time on one of [`PICKUP_SPOTS`] around the middle cover block
//! and replicates it to every peer. Walking onto a pickup collects it: Shield adds a hit point
//! immediately, while Spread and Rapid Fire grant a timed [`GunnerBuff`] tracked by its expiry
//! tick so it replicates once on collection with zero per-tick traffic.

use bevy::prelude::*;
use lightyear::prelude::*;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::config::{
    BUFF_DURATION_TICKS, GUNNER_RADIUS, MAX_HITS_TO_KNOCK_OUT, PICKUP_RADIUS, PICKUP_SPOTS,
};
use super::gunner::{Gunner, GunnerState, Health};
use super::round::ShootoutRules;
use super::{PHASE, ShootoutSystems};
use crate::network::is_host;

/// Which power-up a [`Pickup`] grants when collected.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupKind {
    /// Fires three paint balls in a fan per shot for a short time.
    Spread,
    /// Halves the cooldown between shots for a short time.
    RapidFire,
    /// Grants one extra hit point immediately (up to one above the max setting).
    Shield,
}

impl PickupKind {
    /// All pickup kinds, for random selection.
    pub const ALL: [Self; 3] = [Self::Spread, Self::RapidFire, Self::Shield];
}

/// A power-up waiting on the arena floor. Spawned and collected by the host, replicated to everyone.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Pickup {
    pub kind: PickupKind,
    pub position: Vec2,
}

/// Timed weapon buff active on a gunner.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuffKind {
    /// Fires three paint balls in a fan per shot.
    Spread,
    /// Halves the cooldown between shots.
    RapidFire,
}

/// An active timed buff and the tick it expires on (exclusive).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveBuff {
    pub kind: BuffKind,
    pub until: Tick,
}

/// Replicated on every gunner: whichever timed buff it currently holds, if any.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GunnerBuff(pub Option<ActiveBuff>);

impl GunnerBuff {
    /// The buff active on `tick`, or `None` if no buff is held or it has expired.
    pub fn active(self, tick: Tick) -> Option<BuffKind> {
        self.0
            .filter(|buff| tick < buff.until)
            .map(|buff| buff.kind)
    }

    /// Grant `kind` starting on `tick` for [`BUFF_DURATION_TICKS`].
    pub fn set(&mut self, kind: BuffKind, tick: Tick) {
        self.0 = Some(ActiveBuff {
            kind,
            until: tick + BUFF_DURATION_TICKS as i32,
        });
    }
}

/// Host only: paces pickup spawns during a shootout.
#[derive(Resource, Debug)]
struct PickupSpawner {
    timer: Timer,
    last_spot: Option<usize>,
}

/// Registers [`Pickup`] and [`GunnerBuff`] and runs host pickup spawning and collection.
pub struct PickupsPlugin;

impl Plugin for PickupsPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Pickup>().replicate();
        app.component::<GunnerBuff>().replicate();

        app.add_systems(OnEnter(PHASE), start_spawner.run_if(is_host));
        app.add_systems(OnExit(PHASE), |mut commands: Commands| {
            commands.remove_resource::<PickupSpawner>();
        });
        app.add_systems(
            FixedUpdate,
            (tick_pickup_spawner, collect_pickups)
                .chain()
                .in_set(ShootoutSystems::Pickups)
                .run_if(is_host)
                .run_if(resource_exists::<PickupSpawner>)
                .run_if(in_state(PHASE)),
        );
    }
}

fn start_spawner(rules: Res<ShootoutRules>, mut commands: Commands) {
    commands.insert_resource(PickupSpawner {
        timer: Timer::new(rules.pickup_first_delay, TimerMode::Once),
        last_spot: None,
    });
}

/// Host only: keep at most one pickup on the floor; spawn a fresh one when the timer runs out.
fn tick_pickup_spawner(
    time: Res<Time>,
    rules: Res<ShootoutRules>,
    mut spawner: ResMut<PickupSpawner>,
    existing: Query<(), With<Pickup>>,
    mut commands: Commands,
) {
    if !existing.is_empty() {
        return;
    }
    spawner.timer.tick(time.delta());
    if !spawner.timer.is_finished() {
        return;
    }

    let mut rng = rand::rng();
    let spot_index = pick_spot(spawner.last_spot, &mut rng);
    spawner.last_spot = Some(spot_index);
    let kind = PickupKind::ALL[rng.random_range(0..PickupKind::ALL.len())];
    let position = PICKUP_SPOTS[spot_index];
    spawner.timer = Timer::new(rules.pickup_interval, TimerMode::Once);

    info!("Spawned {kind:?} pickup at {position}");
    commands.spawn((
        Pickup { kind, position },
        DespawnOnExit(PHASE),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

fn pick_spot(last: Option<usize>, rng: &mut impl rand::Rng) -> usize {
    let count = PICKUP_SPOTS.len();
    if let Some(prev) = last
        && count > 1
    {
        let step = rng.random_range(1..count);
        return (prev + step) % count;
    }
    rng.random_range(0..count)
}

/// Host only: award each pickup to the closest gunner touching it, despawn it and start the next
/// spawn countdown.
fn collect_pickups(
    timeline: Res<LocalTimeline>,
    rules: Res<ShootoutRules>,
    mut spawner: ResMut<PickupSpawner>,
    pickups: Query<(Entity, &Pickup)>,
    mut gunners: Query<(Entity, &Gunner, &GunnerState, &mut Health, &mut GunnerBuff)>,
    mut commands: Commands,
) {
    let tick = timeline.tick();
    let reach = GUNNER_RADIUS + PICKUP_RADIUS;
    let reach_sq = reach * reach;

    for (pickup_entity, pickup) in &pickups {
        let collector = gunners
            .iter()
            .filter_map(|(entity, gunner, state, health, _)| {
                if health.remaining == 0 {
                    return None;
                }
                let dist_sq = state.position.distance_squared(pickup.position);
                (dist_sq <= reach_sq).then_some((entity, dist_sq, gunner.owner))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.2.cmp(&b.2)))
            .map(|(entity, _, _)| entity);

        if let Some(collector) = collector
            && let Ok((_, gunner, _, mut health, mut buff)) = gunners.get_mut(collector)
        {
            apply_pickup(pickup.kind, &mut health, &mut buff, tick);
            info!(
                "{} ({:?}) picked up {:?} (hp {}/{})",
                gunner.name, gunner.owner, pickup.kind, health.remaining, health.max
            );
            commands.entity(pickup_entity).despawn();
            spawner.timer = Timer::new(rules.pickup_interval, TimerMode::Once);
        }
    }
}

/// Apply a collected `kind` to a gunner's [`Health`] or [`GunnerBuff`] on `tick`.
pub fn apply_pickup(kind: PickupKind, health: &mut Health, buff: &mut GunnerBuff, tick: Tick) {
    match kind {
        PickupKind::Spread => buff.set(BuffKind::Spread, tick),
        PickupKind::RapidFire => buff.set(BuffKind::RapidFire, tick),
        PickupKind::Shield => {
            let cap = MAX_HITS_TO_KNOCK_OUT + 1;
            health.remaining = (health.remaining + 1).min(cap);
            health.max = health.max.max(health.remaining);
        }
    }
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::Tick;

    use super::{BuffKind, GunnerBuff, PickupKind, apply_pickup};
    use crate::minigames::shootout::config::{BUFF_DURATION_TICKS, MAX_HITS_TO_KNOCK_OUT};
    use crate::minigames::shootout::gunner::Health;

    #[test]
    fn weapon_buffs_activate_for_their_duration_and_then_expire() {
        let mut health = Health::full(3);
        let mut buff = GunnerBuff::default();
        let start = Tick(100);

        apply_pickup(PickupKind::Spread, &mut health, &mut buff, start);
        assert_eq!(buff.active(start), Some(BuffKind::Spread));
        assert_eq!(
            buff.active(start + BUFF_DURATION_TICKS as i32 - 1),
            Some(BuffKind::Spread)
        );
        assert_eq!(buff.active(start + BUFF_DURATION_TICKS as i32), None);

        apply_pickup(PickupKind::RapidFire, &mut health, &mut buff, start + 10);
        assert_eq!(buff.active(start + 10), Some(BuffKind::RapidFire));
        assert_eq!(health, Health::full(3));
    }

    #[test]
    fn shield_adds_a_hit_point_and_expands_max_up_to_cap() {
        let mut health = Health::full(3);
        let mut buff = GunnerBuff::default();

        apply_pickup(PickupKind::Shield, &mut health, &mut buff, Tick(10));
        assert_eq!(
            health,
            Health {
                remaining: 4,
                max: 4
            }
        );
        assert_eq!(buff.active(Tick(10)), None);

        // When already damaged, Shield heals without inflating max beyond the current max.
        health.remaining = 2;
        apply_pickup(PickupKind::Shield, &mut health, &mut buff, Tick(20));
        assert_eq!(
            health,
            Health {
                remaining: 3,
                max: 4
            }
        );

        // Never grows past MAX_HITS_TO_KNOCK_OUT + 1.
        let mut maxed = Health::full(MAX_HITS_TO_KNOCK_OUT + 1);
        apply_pickup(PickupKind::Shield, &mut maxed, &mut buff, Tick(30));
        assert_eq!(maxed.remaining, MAX_HITS_TO_KNOCK_OUT + 1);
    }
}
