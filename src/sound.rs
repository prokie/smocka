//! How loud things are, and how everything that makes a sound plays it.
//!
//! Every sound is spawned with a [`Sound`]: whether it's an effect or music, and how loud the game
//! wants it. The player's [`VolumeSettings`] scale that as the sound starts, and again whenever
//! they change, so turning a volume down also quietens a looping engine or music already playing.
//!
//! `--mute` silences everything for one run through Bevy's [`GlobalVolume`], leaving the saved
//! settings alone.

use bevy::audio::Volume;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Each volume goes from 0 (off) up to this many steps (full).
pub const VOLUME_STEPS: u8 = 10;

/// What a sound is, which decides which volume it follows besides the master volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundKind {
    /// Bounces, beeps, engines: anything the game does.
    Effect,
    Music,
}

/// One of the player's volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VolumeControl {
    /// Every sound.
    Master,
    Effects,
    Music,
}

impl VolumeControl {
    pub const ALL: [VolumeControl; 3] = [
        VolumeControl::Master,
        VolumeControl::Effects,
        VolumeControl::Music,
    ];

    /// The name the Sound screen shows.
    pub fn name(self) -> &'static str {
        match self {
            VolumeControl::Master => "Master",
            VolumeControl::Effects => "Effects",
            VolumeControl::Music => "Music",
        }
    }
}

/// How loud this player wants things, each in steps from 0 to [`VOLUME_STEPS`]. Saved between
/// launches; like [`Controls`](crate::controls::Controls), only the main menu loads and saves it.
#[derive(Resource, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(default)]
pub struct VolumeSettings {
    pub master: u8,
    pub effects: u8,
    pub music: u8,
}

impl Default for VolumeSettings {
    fn default() -> Self {
        Self {
            master: VOLUME_STEPS,
            effects: VOLUME_STEPS,
            music: VOLUME_STEPS,
        }
    }
}

impl VolumeSettings {
    pub fn get(self, control: VolumeControl) -> u8 {
        match control {
            VolumeControl::Master => self.master,
            VolumeControl::Effects => self.effects,
            VolumeControl::Music => self.music,
        }
    }

    fn get_mut(&mut self, control: VolumeControl) -> &mut u8 {
        match control {
            VolumeControl::Master => &mut self.master,
            VolumeControl::Effects => &mut self.effects,
            VolumeControl::Music => &mut self.music,
        }
    }

    /// How loud a sound of `kind` plays, as a factor of the volume it was given: the master
    /// volume times its kind's. Each step is squared, so turning down by a step sounds about as
    /// big a change near the top as near the bottom.
    pub fn level(self, kind: SoundKind) -> f32 {
        let kind_volume = match kind {
            SoundKind::Effect => self.effects,
            SoundKind::Music => self.music,
        };
        loudness(self.master) * loudness(kind_volume)
    }

    /// These settings with every volume at most [`VOLUME_STEPS`], for settings read from a file
    /// someone may have edited.
    pub fn sanitized(self) -> Self {
        Self {
            master: self.master.min(VOLUME_STEPS),
            effects: self.effects.min(VOLUME_STEPS),
            music: self.music.min(VOLUME_STEPS),
        }
    }
}

/// The factor `steps` of volume stand for, from 0 to 1.
fn loudness(steps: u8) -> f32 {
    (f32::from(steps.min(VOLUME_STEPS)) / f32::from(VOLUME_STEPS)).powi(2)
}

/// A volume as the Sound screen shows it, e.g. "80%".
pub fn volume_label(steps: u8) -> String {
    format!("{}%", u32::from(steps) * 100 / u32::from(VOLUME_STEPS))
}

/// Command: change one of the player's volumes by `steps`, keeping it from 0 to [`VOLUME_STEPS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeVolume {
    pub control: VolumeControl,
    pub steps: i32,
}

impl Command for ChangeVolume {
    type Out = ();

    fn apply(self, world: &mut World) {
        let Some(mut settings) = world.get_resource_mut::<VolumeSettings>() else {
            error!("Can't change the volume: there are no volume settings");
            return;
        };
        let volume = settings.get_mut(self.control);
        let before = *volume;
        let after = i32::from(before)
            .saturating_add(self.steps)
            .clamp(0, i32::from(VOLUME_STEPS)) as u8;
        if after == before {
            debug!(
                "{} volume stays at {}",
                self.control.name(),
                volume_label(before)
            );
            return;
        }
        info!(
            "{} volume {} -> {}",
            self.control.name(),
            volume_label(before),
            volume_label(after)
        );
        *volume = after;
    }
}

/// A sound playing (or about to), and how loud the game wants it, before the player's
/// [`VolumeSettings`]. Spawn it with an [`AudioPlayer`] and [`PlaybackSettings`]: the volume set
/// there is replaced.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[require(PlaybackSettings)]
pub struct Sound {
    pub kind: SoundKind,
    /// 1 is as loud as the file.
    pub volume: f32,
}

impl Sound {
    pub fn effect(volume: f32) -> Self {
        Self {
            kind: SoundKind::Effect,
            volume,
        }
    }

    pub fn music(volume: f32) -> Self {
        Self {
            kind: SoundKind::Music,
            volume,
        }
    }
}

/// Everything needed to play `sound` once as an effect at `volume` (1 is as loud as the file). The
/// entity despawns when it's done, so the sound carries on over a change of phase.
pub fn sound_effect(sound: &Handle<AudioSource>, volume: f32) -> impl Bundle {
    (
        Sound::effect(volume),
        AudioPlayer::new(sound.clone()),
        PlaybackSettings::DESPAWN,
    )
}

/// Play `sound` once as an effect: see [`sound_effect`].
pub fn play_sound(commands: &mut Commands, sound: &Handle<AudioSource>, volume: f32) {
    commands.spawn(sound_effect(sound, volume));
}

/// The music playing. There's at most one: [`PlayMusic`] replaces it.
#[derive(Component, Debug)]
pub struct MusicTrack;

/// How loud music plays, before the player's volumes.
const MUSIC_VOLUME: f32 = 0.5;

/// Command: loop `.0` as music, in place of any music already playing. Music isn't tied to a
/// screen or phase: it plays until [`StopMusic`] or the next [`PlayMusic`].
#[derive(Debug, Clone)]
pub struct PlayMusic(pub Handle<AudioSource>);

impl Command for PlayMusic {
    type Out = ();

    fn apply(self, world: &mut World) {
        StopMusic.apply(world);
        match self.0.path() {
            Some(path) => info!("Playing music: {path}"),
            None => info!("Playing music"),
        }
        world.spawn((
            MusicTrack,
            Sound::music(MUSIC_VOLUME),
            AudioPlayer::new(self.0),
            PlaybackSettings::LOOP,
        ));
    }
}

/// Command: stop the music, if any is playing.
#[derive(Debug, Clone, Copy)]
pub struct StopMusic;

impl Command for StopMusic {
    type Out = ();

    fn apply(self, world: &mut World) {
        let tracks: Vec<Entity> = world
            .query_filtered::<Entity, With<MusicTrack>>()
            .iter(world)
            .collect();
        if !tracks.is_empty() {
            debug!("Stopping the music");
        }
        for track in tracks {
            world.despawn(track);
        }
    }
}

/// Keeps every [`Sound`] as loud as the player's [`VolumeSettings`] say.
pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VolumeSettings>();
        app.add_observer(set_starting_volume);
        app.add_systems(
            Update,
            follow_volume_settings.run_if(resource_changed::<VolumeSettings>),
        );
    }
}

/// A new sound starts at its own volume scaled by the settings. Bevy also scales it by the
/// [`GlobalVolume`] as it starts.
fn set_starting_volume(
    trigger: On<Add<Sound>>,
    settings: Res<VolumeSettings>,
    mut sounds: Query<(&Sound, &mut PlaybackSettings)>,
) {
    if let Ok((sound, mut playback)) = sounds.get_mut(trigger.entity) {
        playback.volume = Volume::Linear(sound.volume * settings.level(sound.kind));
    }
}

/// When the settings change, change the volume of every sound already playing. Bevy only applies
/// the [`GlobalVolume`] as a sound starts, so it's applied here too.
fn follow_volume_settings(
    settings: Res<VolumeSettings>,
    global: Option<Res<GlobalVolume>>,
    mut playing: Query<(&Sound, &mut AudioSink)>,
) {
    let global = global.map_or(1.0, |global| global.volume.to_linear());
    for (sound, mut sink) in &mut playing {
        sink.set_volume(Volume::Linear(
            sound.volume * settings.level(sound.kind) * global,
        ));
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::{
        ChangeVolume, MusicTrack, PlayMusic, Sound, SoundKind, SoundPlugin, StopMusic,
        VOLUME_STEPS, VolumeControl, VolumeSettings, sound_effect, volume_label,
    };

    #[test]
    fn full_volume_plays_sounds_as_given_and_zero_silences_them() {
        let full = VolumeSettings::default();
        assert_eq!(full.level(SoundKind::Effect), 1.0);
        assert_eq!(full.level(SoundKind::Music), 1.0);
        let no_music = VolumeSettings { music: 0, ..full };
        assert_eq!(no_music.level(SoundKind::Music), 0.0);
        assert_eq!(no_music.level(SoundKind::Effect), 1.0);
        let no_master = VolumeSettings { master: 0, ..full };
        assert_eq!(no_master.level(SoundKind::Effect), 0.0);
    }

    #[test]
    fn master_and_kind_volumes_multiply_and_lower_steps_are_quieter() {
        let half = VolumeSettings {
            master: VOLUME_STEPS / 2,
            effects: VOLUME_STEPS / 2,
            music: VOLUME_STEPS,
        };
        assert_eq!(half.level(SoundKind::Music), 0.25);
        assert_eq!(half.level(SoundKind::Effect), 0.0625);
        let levels: Vec<f32> = (0..=VOLUME_STEPS)
            .map(|effects| VolumeSettings { effects, ..half }.level(SoundKind::Effect))
            .collect();
        assert!(
            levels.windows(2).all(|pair| pair[0] < pair[1]),
            "{levels:?}"
        );
    }

    #[test]
    fn edited_settings_are_made_safe() {
        let edited = VolumeSettings {
            master: 200,
            effects: 3,
            music: VOLUME_STEPS + 1,
        };
        assert_eq!(
            edited.sanitized(),
            VolumeSettings {
                master: VOLUME_STEPS,
                effects: 3,
                music: VOLUME_STEPS,
            }
        );
    }

    #[test]
    fn settings_missing_from_the_file_get_their_defaults() {
        let settings: VolumeSettings = toml::from_str("music = 4").unwrap();
        assert_eq!(
            settings,
            VolumeSettings {
                music: 4,
                ..VolumeSettings::default()
            }
        );
    }

    #[test]
    fn volumes_show_as_percentages() {
        assert_eq!(volume_label(0), "0%");
        assert_eq!(volume_label(VOLUME_STEPS / 2), "50%");
        assert_eq!(volume_label(VOLUME_STEPS), "100%");
    }

    #[test]
    fn the_volume_changes_within_its_limits() {
        let mut world = World::new();
        world.insert_resource(VolumeSettings::default());
        let mut change = |control, steps| {
            world.commands().queue(ChangeVolume { control, steps });
            world.flush();
            *world.resource::<VolumeSettings>()
        };
        assert_eq!(change(VolumeControl::Effects, -3).effects, VOLUME_STEPS - 3);
        assert_eq!(change(VolumeControl::Effects, 1).effects, VOLUME_STEPS - 2);
        assert_eq!(change(VolumeControl::Music, -100).music, 0);
        assert_eq!(change(VolumeControl::Master, 100).master, VOLUME_STEPS);
        assert_eq!(change(VolumeControl::Master, 0).effects, VOLUME_STEPS - 2);
    }

    fn app(settings: VolumeSettings) -> App {
        let mut app = App::new();
        app.add_plugins(SoundPlugin);
        app.insert_resource(settings);
        app
    }

    fn starting_volume(app: &mut App, entity: Entity) -> f32 {
        app.world()
            .get::<PlaybackSettings>(entity)
            .unwrap()
            .volume
            .to_linear()
    }

    #[test]
    fn a_sound_starts_at_its_volume_scaled_by_the_settings() {
        let mut app = app(VolumeSettings {
            master: VOLUME_STEPS,
            effects: VOLUME_STEPS / 2,
            music: 0,
        });
        let effect = app
            .world_mut()
            .spawn(sound_effect(&Handle::default(), 0.8))
            .id();
        assert!((starting_volume(&mut app, effect) - 0.2).abs() < 1e-6);
        let music = app
            .world_mut()
            .spawn((
                Sound::music(0.5),
                AudioPlayer::new(Handle::default()),
                PlaybackSettings::LOOP,
            ))
            .id();
        assert_eq!(starting_volume(&mut app, music), 0.0);
    }

    #[test]
    fn new_music_replaces_the_old_and_stop_ends_it() {
        let mut app = app(VolumeSettings::default());
        let tracks = |app: &mut App| {
            app.world_mut()
                .query_filtered::<(), With<MusicTrack>>()
                .iter(app.world())
                .count()
        };
        app.world_mut()
            .commands()
            .queue(PlayMusic(Handle::default()));
        app.world_mut().flush();
        app.world_mut()
            .commands()
            .queue(PlayMusic(Handle::default()));
        app.world_mut().flush();
        assert_eq!(tracks(&mut app), 1);
        app.world_mut().commands().queue(StopMusic);
        app.world_mut().flush();
        assert_eq!(tracks(&mut app), 0);
    }
}
