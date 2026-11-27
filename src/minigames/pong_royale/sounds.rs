//! Pong Royale's sounds: the ball bouncing off a paddle or a wall, a lost life, and a player going
//! out. They're Pong's and the shootout's.
//!
//! Each peer plays them from what it sees: bounces when the ball it draws turns, lost lives from
//! the replicated [`Lives`], and a player going out when a [`Wall`] goes up once play has started.

use bevy::prelude::*;
use lightyear::prelude::LocalTimeline;

use super::PHASE;
use super::config::{BALL_MAX_SPEED, BALL_SIZE, PADDLE_LENGTH};
use super::paddles::{Lives, Paddle};
use super::play::{Ball, Wall};
use crate::config::network::TICK_RATE;
use crate::minigames::countdown::StartTick;
use crate::sound::{play_sound, play_varied_sound};

const PADDLE_HIT_SOUND: &str = "minigames/two_player_pong/paddle_hit.wav";
const WALL_BOUNCE_SOUND: &str = "minigames/two_player_pong/wall_bounce.wav";
const LIFE_LOST_SOUND: &str = "minigames/two_player_pong/goal.wav";
const OUT_SOUND: &str = "minigames/shootout/knockout.wav";

const PADDLE_HIT_VOLUME: f32 = 0.5;
const WALL_BOUNCE_VOLUME: f32 = 0.4;
const LIFE_LOST_VOLUME: f32 = 0.5;
const OUT_VOLUME: f32 = 0.6;

/// A ball that turned this close to a paddle's centre bounced off it; anywhere else, off a wall or
/// a corner block.
const NEAR_PADDLE: f32 = PADDLE_LENGTH / 2.0 + BALL_SIZE * 2.0;
/// A ball that moved further than this in a frame was served again from the centre: not a bounce.
/// Twice as far as it can move in a tick.
const SERVE_JUMP: f32 = 2.0 * BALL_MAX_SPEED / TICK_RATE as f32;

/// What the ball bounced off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bounce {
    Paddle,
    Wall,
}

/// Where the ball was drawn on the last frame, and the way it last moved.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct BallTrack {
    last: Vec2,
    heading: Vec2,
}

impl BallTrack {
    pub fn new(position: Vec2) -> Self {
        Self {
            last: position,
            heading: Vec2::ZERO,
        }
    }

    /// The track once the ball is drawn at `position`, and what it bounced off since the last
    /// frame, if it turned back along either axis: a paddle if one of `paddles` (their centres) is
    /// near where it turned, otherwise a wall.
    pub fn moved_to(self, position: Vec2, paddles: &[Vec2]) -> (Self, Option<Bounce>) {
        let step = position - self.last;
        if step.length() > SERVE_JUMP {
            return (Self::new(position), None);
        }
        let turned = step.x * self.heading.x < 0.0 || step.y * self.heading.y < 0.0;
        let bounce = turned.then(|| {
            let near_paddle = paddles
                .iter()
                .any(|paddle| paddle.distance(self.last) < NEAR_PADDLE);
            if near_paddle {
                Bounce::Paddle
            } else {
                Bounce::Wall
            }
        });
        let heading = |step: f32, heading: f32| {
            if step == 0.0 { heading } else { step.signum() }
        };
        let track = Self {
            last: position,
            heading: Vec2::new(
                heading(step.x, self.heading.x),
                heading(step.y, self.heading.y),
            ),
        };
        (track, bounce)
    }
}

/// The lives a paddle had when this peer last looked, to hear when one is lost.
#[derive(Component, Debug)]
struct SeenLives(u32);

#[derive(Resource)]
struct RoyaleSounds {
    paddle_hit: Handle<AudioSource>,
    wall_bounce: Handle<AudioSource>,
    life_lost: Handle<AudioSource>,
    out: Handle<AudioSource>,
}

/// Plays Pong Royale's sounds on every peer.
pub struct RoyaleSoundsPlugin;

impl Plugin for RoyaleSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sounds);
        app.add_observer(play_out_sound);
        app.add_systems(
            Update,
            (play_bounce_sounds, play_life_lost_sounds).run_if(in_state(PHASE)),
        );
    }
}

fn load_sounds(assets: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(RoyaleSounds {
        paddle_hit: assets.load(PADDLE_HIT_SOUND),
        wall_bounce: assets.load(WALL_BOUNCE_SOUND),
        life_lost: assets.load(LIFE_LOST_SOUND),
        out: assets.load(OUT_SOUND),
    });
}

/// Follow where the ball is drawn and play a sound when it bounces, a little higher or lower each
/// time so a long rally doesn't repeat one sound.
fn play_bounce_sounds(
    mut balls: Query<(Entity, &Transform, Option<&mut BallTrack>), With<Ball>>,
    paddles: Query<&Transform, (With<Paddle>, Without<Ball>)>,
    sounds: Res<RoyaleSounds>,
    mut commands: Commands,
) {
    let paddles: Vec<Vec2> = paddles
        .iter()
        .map(|transform| transform.translation.truncate())
        .collect();
    for (entity, transform, track) in &mut balls {
        let position = transform.translation.truncate();
        let Some(mut track) = track else {
            commands.entity(entity).insert(BallTrack::new(position));
            continue;
        };
        let bounce;
        (*track, bounce) = track.moved_to(position, &paddles);
        match bounce {
            Some(Bounce::Paddle) => {
                play_varied_sound(&mut commands, &sounds.paddle_hit, PADDLE_HIT_VOLUME);
            }
            Some(Bounce::Wall) => {
                play_varied_sound(&mut commands, &sounds.wall_bounce, WALL_BOUNCE_VOLUME);
            }
            None => {}
        }
    }
}

/// Play a sound when a player loses a life (but not their last: going out has its own).
fn play_life_lost_sounds(
    mut paddles: Query<(Entity, &Lives, Option<&mut SeenLives>), With<Paddle>>,
    sounds: Res<RoyaleSounds>,
    mut commands: Commands,
) {
    for (entity, lives, seen) in &mut paddles {
        match seen {
            Some(mut seen) => {
                if lives.0 < seen.0 && lives.0 > 0 {
                    play_sound(&mut commands, &sounds.life_lost, LIFE_LOST_VOLUME);
                }
                seen.0 = lives.0;
            }
            None => {
                commands.entity(entity).insert(SeenLives(lives.0));
            }
        }
    }
}

/// A wall going up once play has started is a player going out (the walls across empty sides go
/// up before).
fn play_out_sound(
    _wall: On<Add<Wall>>,
    start: Option<Single<&StartTick>>,
    timeline: Option<Res<LocalTimeline>>,
    sounds: Option<Res<RoyaleSounds>>,
    mut commands: Commands,
) {
    let started = match (start, timeline) {
        (Some(start), Some(timeline)) => start.has_started(timeline.tick()),
        _ => false,
    };
    if let (true, Some(sounds)) = (started, sounds) {
        play_sound(&mut commands, &sounds.out, OUT_VOLUME);
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;

    use super::{BallTrack, Bounce};

    /// The track after the ball is drawn at each of `positions` in turn, and what it bounced off at
    /// each, with paddles at `paddles`.
    fn follow(positions: &[Vec2], paddles: &[Vec2]) -> Vec<Option<Bounce>> {
        let mut track = BallTrack::new(positions[0]);
        positions[1..]
            .iter()
            .map(|&position| {
                let bounce;
                (track, bounce) = track.moved_to(position, paddles);
                bounce
            })
            .collect()
    }

    #[test]
    fn turning_near_a_paddle_is_a_paddle_hit_and_elsewhere_a_wall() {
        let paddle = Vec2::new(-158.0, 0.0);
        let towards_paddle = [
            Vec2::new(-140.0, 0.0),
            Vec2::new(-145.0, 1.0),
            Vec2::new(-150.0, 2.0),
            Vec2::new(-145.0, 3.0),
        ];
        assert_eq!(
            follow(&towards_paddle, &[paddle]),
            [None, None, Some(Bounce::Paddle)]
        );
        // The same turn with no paddle there.
        assert_eq!(
            follow(&towards_paddle, &[]),
            [None, None, Some(Bounce::Wall)]
        );
    }

    #[test]
    fn a_serve_from_the_centre_is_not_a_bounce() {
        let positions = [
            Vec2::new(160.0, 0.0),
            Vec2::new(165.0, 0.0),
            Vec2::ZERO,
            Vec2::new(-2.0, 0.0),
        ];
        assert_eq!(follow(&positions, &[]), [None, None, None]);
    }
}
