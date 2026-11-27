//! Sound shared by every minigame, and how minigames play sounds.
//!
//! Sounds are local: each peer plays them for what it sees on its own screen, worked out from
//! replicated state it already has, so nothing about them goes over the network. The files are
//! made by `tools/make_sounds.py`.

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::session::SessionPhase;

/// Played when a minigame ends with a winner.
const MINIGAME_WON_SOUND: &str = "minigames/minigame_won.wav";
const MINIGAME_WON_VOLUME: f32 = 0.5;

/// Play `sound` once at `volume` (1 is as loud as the file). The entity playing it despawns when
/// it's done, so the sound carries on over a change of phase.
pub fn play_sound(commands: &mut Commands, sound: &Handle<AudioSource>, volume: f32) {
    commands.spawn((
        AudioPlayer::new(sound.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
    ));
}

/// Whether the session going from `exited` to `entered` means a minigame just ended with a
/// winner: it goes on to the results, or to a tournament's next match. A walkover counts, since a
/// player goes through. An abandoned minigame goes to the lobby or the next round instead.
pub fn minigame_won(exited: Option<SessionPhase>, entered: Option<SessionPhase>) -> bool {
    exited.is_some_and(SessionPhase::is_minigame)
        && matches!(
            entered,
            Some(SessionPhase::Results | SessionPhase::NextMatch)
        )
}

#[derive(Resource)]
struct MinigameSounds {
    won: Handle<AudioSource>,
}

/// Plays a jingle on every peer when a minigame is won.
pub struct MinigameSoundsPlugin;

impl Plugin for MinigameSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sounds);
        app.add_systems(Update, play_won_sound);
    }
}

fn load_sounds(assets: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(MinigameSounds {
        won: assets.load(MINIGAME_WON_SOUND),
    });
}

fn play_won_sound(
    mut transitions: MessageReader<StateTransitionEvent<SessionPhase>>,
    sounds: Res<MinigameSounds>,
    mut commands: Commands,
) {
    for transition in transitions.read() {
        if minigame_won(transition.exited, transition.entered) {
            play_sound(&mut commands, &sounds.won, MINIGAME_WON_VOLUME);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::minigame_won;
    use crate::minigames::Minigame;
    use crate::session::SessionPhase;

    const PONG: Option<SessionPhase> = Some(SessionPhase::Playing(Minigame::TwoPlayerPong));

    #[test]
    fn a_minigame_going_to_its_results_or_the_next_match_was_won() {
        assert!(minigame_won(PONG, Some(SessionPhase::Results)));
        assert!(minigame_won(PONG, Some(SessionPhase::NextMatch)));
    }

    #[test]
    fn an_abandoned_minigame_or_another_phase_change_was_not() {
        assert!(!minigame_won(PONG, Some(SessionPhase::Lobby)));
        assert!(!minigame_won(PONG, Some(SessionPhase::NextRound)));
        assert!(!minigame_won(PONG, None));
        assert!(!minigame_won(
            Some(SessionPhase::Lobby),
            Some(SessionPhase::NextMatch)
        ));
        assert!(!minigame_won(
            Some(SessionPhase::Results),
            Some(SessionPhase::Standings)
        ));
    }
}
