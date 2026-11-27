//! Shootout's sounds: firing a paint ball, paint splatting on walls, gunner hits, knockouts,
//! centre pickups and the sudden-death border warning.
//!
//! Each peer plays them locally from what it sees: firing when a [`PaintBall`] spawns, wall hits
//! from [`PaintBallWallHit`], non-fatal gunner hits when a gunner's replicated [`Health`] drops,
//! knockouts from [`GunnerKnockedOut`], pickups when a [`Pickup`] is removed mid-round, and
//! sudden death when [`SafeZone::active`] turns true.

use bevy::prelude::*;

use super::PHASE;
use super::gunner::{Gunner, GunnerKnockedOut, Health};
use super::pickups::Pickup;
use super::shots::{PaintBall, PaintBallWallHit};
use super::zone::SafeZone;
use crate::session::SessionPhase;
use crate::sound::{play_sound, play_varied_sound};

const SHOT_SOUND: &str = "minigames/shootout/shot.wav";
const WALL_HIT_SOUND: &str = "minigames/shootout/wall_hit.wav";
const HIT_SOUND: &str = "minigames/shootout/hit.wav";
const KNOCKOUT_SOUND: &str = "minigames/shootout/knockout.wav";
const PICKUP_SOUND: &str = "minigames/shootout/pickup.wav";
const SUDDEN_DEATH_SOUND: &str = "minigames/shootout/sudden_death.wav";

const SHOT_VOLUME: f32 = 0.45;
const WALL_HIT_VOLUME: f32 = 0.3;
const HIT_VOLUME: f32 = 0.55;
const KNOCKOUT_VOLUME: f32 = 0.65;
const PICKUP_VOLUME: f32 = 0.55;
const SUDDEN_DEATH_VOLUME: f32 = 0.65;

/// The last [`Health::remaining`] this peer saw on a gunner, so it only plays a hit sound when
/// health drops after spawn.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct SeenHealth(u32);

/// What sound a gunner's health change from `before` to `now` calls for, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageSound {
    /// The gunner took a hit and is still standing.
    Hit,
}

/// Whether `health` dropping from `before` to `now` should play a non-fatal hit sound. Knockouts
/// (when the gunner is removed) have their own sound.
pub fn damage_sound(before: u32, now: u32) -> Option<DamageSound> {
    (now < before && now > 0).then_some(DamageSound::Hit)
}

#[derive(Resource)]
struct ShootoutSounds {
    shot: Handle<AudioSource>,
    wall_hit: Handle<AudioSource>,
    hit: Handle<AudioSource>,
    knockout: Handle<AudioSource>,
    pickup: Handle<AudioSource>,
    sudden_death: Handle<AudioSource>,
}

/// Whether this peer already played the sudden-death chime during the current round.
#[derive(Resource, Default)]
struct SuddenDeathAnnounced(bool);

/// Plays shootout's sounds on every peer.
pub struct ShootoutSoundsPlugin;

impl Plugin for ShootoutSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SuddenDeathAnnounced>();
        app.add_systems(Startup, load_sounds);
        app.add_systems(OnEnter(PHASE), reset_sudden_death_sound);
        app.add_observer(play_shot_sound);
        app.add_observer(play_wall_hit_sound);
        app.add_observer(play_knockout_sound);
        app.add_observer(play_pickup_sound);
        app.add_systems(
            Update,
            (play_hit_sounds, play_sudden_death_sound).run_if(in_state(PHASE)),
        );
    }
}

fn reset_sudden_death_sound(mut announced: ResMut<SuddenDeathAnnounced>) {
    announced.0 = false;
}

fn load_sounds(assets: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(ShootoutSounds {
        shot: assets.load(SHOT_SOUND),
        wall_hit: assets.load(WALL_HIT_SOUND),
        hit: assets.load(HIT_SOUND),
        knockout: assets.load(KNOCKOUT_SOUND),
        pickup: assets.load(PICKUP_SOUND),
        sudden_death: assets.load(SUDDEN_DEATH_SOUND),
    });
}

fn play_shot_sound(
    _trigger: On<Add<PaintBall>>,
    sounds: Option<Res<ShootoutSounds>>,
    mut commands: Commands,
) {
    if let Some(sounds) = sounds {
        play_varied_sound(&mut commands, &sounds.shot, SHOT_VOLUME);
    }
}

fn play_wall_hit_sound(
    _trigger: On<PaintBallWallHit>,
    sounds: Option<Res<ShootoutSounds>>,
    mut commands: Commands,
) {
    if let Some(sounds) = sounds {
        play_varied_sound(&mut commands, &sounds.wall_hit, WALL_HIT_VOLUME);
    }
}

fn play_hit_sounds(
    gunners: Query<(Entity, &Health, Option<&mut SeenHealth>), With<Gunner>>,
    sounds: Res<ShootoutSounds>,
    mut commands: Commands,
) {
    for (entity, health, seen) in gunners {
        let Some(mut seen) = seen else {
            commands.entity(entity).insert(SeenHealth(health.remaining));
            continue;
        };
        if damage_sound(seen.0, health.remaining) == Some(DamageSound::Hit) {
            play_varied_sound(&mut commands, &sounds.hit, HIT_VOLUME);
        }
        seen.0 = health.remaining;
    }
}

fn play_knockout_sound(
    _trigger: On<GunnerKnockedOut>,
    sounds: Option<Res<ShootoutSounds>>,
    mut commands: Commands,
) {
    if let Some(sounds) = sounds {
        play_sound(&mut commands, &sounds.knockout, KNOCKOUT_VOLUME);
    }
}

fn play_pickup_sound(
    _trigger: On<Remove<Pickup>>,
    phase: Option<Res<State<SessionPhase>>>,
    sounds: Option<Res<ShootoutSounds>>,
    mut commands: Commands,
) {
    if !phase.is_some_and(|phase| *phase.get() == PHASE) {
        return;
    }
    if let Some(sounds) = sounds {
        play_sound(&mut commands, &sounds.pickup, PICKUP_VOLUME);
    }
}

fn play_sudden_death_sound(
    zones: Query<&SafeZone>,
    mut announced: ResMut<SuddenDeathAnnounced>,
    sounds: Res<ShootoutSounds>,
    mut commands: Commands,
) {
    if announced.0 {
        return;
    }
    if zones.iter().any(|zone| zone.active) {
        announced.0 = true;
        play_sound(&mut commands, &sounds.sudden_death, SUDDEN_DEATH_VOLUME);
    }
}

#[cfg(test)]
mod tests {
    use super::{DamageSound, damage_sound};

    #[test]
    fn plays_hit_sound_only_on_non_fatal_damage() {
        assert_eq!(damage_sound(3, 3), None);
        assert_eq!(damage_sound(3, 2), Some(DamageSound::Hit));
        assert_eq!(damage_sound(2, 1), Some(DamageSound::Hit));
        // Knockouts play their own sound when the gunner is removed.
        assert_eq!(damage_sound(1, 0), None);
        assert_eq!(damage_sound(1, 3), None);
    }
}
