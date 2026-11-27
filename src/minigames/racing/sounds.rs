//! Racing's sounds: bumps, and for the car you drive, its engine, boosts, laps and finish. The
//! countdown before the start is the shared one (see [`countdown`](crate::minigames::countdown)).
//!
//! Each peer plays them from what it sees: laps and the finish from your car's replicated
//! [`RaceProgress`], bumps from [`CarsBumped`], and the engine and boosts from your car's
//! [`CarState`]. Bumps are only worked out by the host and drivers, so spectators don't hear them.

use bevy::prelude::*;
use lightyear::prelude::input::native::InputMarker;

use super::PHASE;
use super::RacingInput;
use super::bumps::CarsBumped;
use super::car::CarState;
use super::config::MAX_SPEED;
use super::progress::RaceProgress;
use crate::sound::{Sound, play_sound, play_varied_sound};

const LAP_SOUND: &str = "minigames/racing/lap.wav";
const FINISH_SOUND: &str = "minigames/racing/finish.wav";
const BUMP_SOUND: &str = "minigames/racing/bump.wav";
const BOOST_SOUND: &str = "minigames/racing/boost.wav";
/// Loops seamlessly, and is played faster (so higher) the faster the car goes.
const ENGINE_SOUND: &str = "minigames/racing/engine.wav";

const LAP_VOLUME: f32 = 0.5;
const FINISH_VOLUME: f32 = 0.5;
const BOOST_VOLUME: f32 = 0.45;
const ENGINE_VOLUME: f32 = 0.25;
/// The loudest bump, for two cars meeting head-on at top speed.
const BUMP_MAX_VOLUME: f32 = 0.8;
/// Bumps closing slower than this, in world units per second, are cars nudging each other: silent.
const BUMP_MIN_IMPACT: f32 = 20.0;

/// How fast the engine sound plays standing still, and at [`MAX_SPEED`].
const ENGINE_IDLE_SPEED: f32 = 0.7;
const ENGINE_TOP_SPEED: f32 = 2.0;
/// Changes to the engine sound's speed smaller than this aren't worth passing on.
const ENGINE_SPEED_STEP: f32 = 0.01;

/// A sound for your car getting further round the track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressSound {
    Lap,
    Finish,
}

/// The sound for a car's progress going from `before` to `now`: finishing, or else a lap.
pub fn progress_sound(before: RaceProgress, now: RaceProgress) -> Option<ProgressSound> {
    if before.place.is_none() && now.place.is_some() {
        Some(ProgressSound::Finish)
    } else if now.laps > before.laps {
        Some(ProgressSound::Lap)
    } else {
        None
    }
}

/// Whether a car's [`CarState::boost`] going from `before` to `now` means it just got onto a boost
/// pad. A boost only runs down, except when a pad tops it up.
pub fn boost_topped_up(before: f32, now: f32) -> bool {
    now > before
}

/// How fast to play the engine sound for a car going at `speed` (forwards or in reverse).
pub fn engine_speed(speed: f32) -> f32 {
    let throttle = (speed.abs() / MAX_SPEED).min(1.0);
    ENGINE_IDLE_SPEED + (ENGINE_TOP_SPEED - ENGINE_IDLE_SPEED) * throttle
}

/// How loud a bump closing at `impact` world units per second is, if it's loud enough to hear.
pub fn bump_volume(impact: f32) -> Option<f32> {
    // Two cars head-on at top speed close at twice it.
    let hardest = 2.0 * MAX_SPEED;
    (impact >= BUMP_MIN_IMPACT).then(|| BUMP_MAX_VOLUME * (impact / hardest).min(1.0).sqrt())
}

/// The looping engine sound for the car you drive.
#[derive(Component)]
struct EngineSound;

/// Your car: the one that takes this machine's input.
type OwnCar = With<InputMarker<RacingInput>>;

#[derive(Resource)]
struct RacingSounds {
    lap: Handle<AudioSource>,
    finish: Handle<AudioSource>,
    bump: Handle<AudioSource>,
    boost: Handle<AudioSource>,
    engine: Handle<AudioSource>,
}

/// Plays racing's sounds on every peer.
pub struct RacingSoundsPlugin;

impl Plugin for RacingSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sounds);
        app.add_observer(play_bump_sound);
        app.add_systems(
            Update,
            (
                play_progress_sounds,
                play_boost_sound,
                run_engine_sound,
                rev_engine_sound,
            )
                .run_if(in_state(PHASE)),
        );
    }
}

fn load_sounds(assets: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(RacingSounds {
        lap: assets.load(LAP_SOUND),
        finish: assets.load(FINISH_SOUND),
        bump: assets.load(BUMP_SOUND),
        boost: assets.load(BOOST_SOUND),
        engine: assets.load(ENGINE_SOUND),
    });
}

/// Chime when your car completes a lap or finishes. `seen` is its progress last frame: `None`
/// without a car.
fn play_progress_sounds(
    own_car: Query<&RaceProgress, OwnCar>,
    sounds: Res<RacingSounds>,
    mut seen: Local<Option<RaceProgress>>,
    mut commands: Commands,
) {
    let now = own_car.single().ok().copied();
    if let (Some(before), Some(now)) = (*seen, now) {
        match progress_sound(before, now) {
            Some(ProgressSound::Lap) => play_sound(&mut commands, &sounds.lap, LAP_VOLUME),
            Some(ProgressSound::Finish) => {
                play_sound(&mut commands, &sounds.finish, FINISH_VOLUME);
            }
            None => {}
        }
    }
    *seen = now;
}

/// Whoosh when your car gets onto a boost pad. `seen` is its boost last frame: `None` without a
/// car.
fn play_boost_sound(
    own_car: Query<&CarState, OwnCar>,
    sounds: Res<RacingSounds>,
    mut seen: Local<Option<f32>>,
    mut commands: Commands,
) {
    let now = own_car.single().ok().map(|state| state.boost);
    if let (Some(before), Some(now)) = (*seen, now)
        && boost_topped_up(before, now)
    {
        debug!("Your car hit a boost pad");
        play_sound(&mut commands, &sounds.boost, BOOST_VOLUME);
    }
    *seen = now;
}

/// Thud when two cars bump, louder the harder they hit, and a little higher or lower each time.
fn play_bump_sound(
    bumped: On<CarsBumped>,
    sounds: Option<Res<RacingSounds>>,
    mut commands: Commands,
) {
    if let (Some(sounds), Some(volume)) = (sounds, bump_volume(bumped.impact)) {
        play_varied_sound(&mut commands, &sounds.bump, volume);
    }
}

/// Run the engine sound while you have a car, and stop it if your car goes (say, because you
/// left). It stops with the race either way.
fn run_engine_sound(
    own_car: Query<(), OwnCar>,
    engine: Query<Entity, With<EngineSound>>,
    sounds: Res<RacingSounds>,
    mut commands: Commands,
) {
    match (own_car.is_empty(), engine.single()) {
        (false, Err(_)) => {
            commands.spawn((
                EngineSound,
                Sound::effect(ENGINE_VOLUME),
                AudioPlayer::new(sounds.engine.clone()),
                PlaybackSettings::LOOP.with_speed(ENGINE_IDLE_SPEED),
                DespawnOnExit(PHASE),
            ));
        }
        (true, Ok(engine)) => commands.entity(engine).despawn(),
        _ => {}
    }
}

/// Play the engine sound faster the faster your car goes. Only once it has started playing (has
/// an [`AudioSink`]).
fn rev_engine_sound(
    own_car: Query<&CarState, OwnCar>,
    engine: Query<&AudioSink, With<EngineSound>>,
) {
    let (Ok(car), Ok(engine)) = (own_car.single(), engine.single()) else {
        return;
    };
    let speed = engine_speed(car.speed);
    if (engine.speed() - speed).abs() >= ENGINE_SPEED_STEP {
        engine.set_speed(speed);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BUMP_MAX_VOLUME, BUMP_MIN_IMPACT, ENGINE_IDLE_SPEED, ENGINE_TOP_SPEED, ProgressSound,
        boost_topped_up, bump_volume, engine_speed, progress_sound,
    };
    use crate::minigames::racing::config::{BOOST_TIME, MAX_REVERSE_SPEED, MAX_SPEED};
    use crate::minigames::racing::progress::RaceProgress;

    fn progress(laps: u32, place: Option<u32>) -> RaceProgress {
        RaceProgress {
            next_checkpoint: 1,
            laps,
            place,
        }
    }

    #[test]
    fn chimes_for_a_lap_and_plays_the_finish_on_the_last() {
        assert_eq!(
            progress_sound(progress(0, None), progress(1, None)),
            Some(ProgressSound::Lap)
        );
        assert_eq!(
            progress_sound(progress(2, None), progress(3, Some(1))),
            Some(ProgressSound::Finish)
        );
        assert_eq!(progress_sound(progress(1, None), progress(1, None)), None);
        assert_eq!(
            progress_sound(progress(3, Some(1)), progress(3, Some(1))),
            None
        );
    }

    #[test]
    fn whooshes_when_a_pad_tops_the_boost_up_but_not_as_it_runs_down() {
        assert!(boost_topped_up(0.0, BOOST_TIME));
        // A second pad before the first boost is over.
        assert!(boost_topped_up(0.3, BOOST_TIME));
        assert!(!boost_topped_up(BOOST_TIME, 0.9));
        // Still on the pad: topped up to the same each tick.
        assert!(!boost_topped_up(0.99, 0.99));
        assert!(!boost_topped_up(0.0, 0.0));
    }

    #[test]
    fn the_engine_revs_with_speed_either_way() {
        assert_eq!(engine_speed(0.0), ENGINE_IDLE_SPEED);
        assert_eq!(engine_speed(MAX_SPEED), ENGINE_TOP_SPEED);
        assert_eq!(engine_speed(2.0 * MAX_SPEED), ENGINE_TOP_SPEED);
        let reversing = engine_speed(-MAX_REVERSE_SPEED);
        assert!(ENGINE_IDLE_SPEED < reversing && reversing < engine_speed(MAX_SPEED / 2.0));
    }

    #[test]
    fn harder_bumps_are_louder_and_nudges_are_silent() {
        assert_eq!(bump_volume(BUMP_MIN_IMPACT / 2.0), None);
        let soft = bump_volume(BUMP_MIN_IMPACT).unwrap();
        let hard = bump_volume(MAX_SPEED).unwrap();
        assert!(0.0 < soft && soft < hard && hard < BUMP_MAX_VOLUME);
        assert_eq!(bump_volume(2.0 * MAX_SPEED), Some(BUMP_MAX_VOLUME));
        assert_eq!(bump_volume(10.0 * MAX_SPEED), Some(BUMP_MAX_VOLUME));
    }
}
