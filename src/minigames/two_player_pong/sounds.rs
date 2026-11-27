//! Pong's sounds: the ball hitting a paddle or a wall, and goals.
//!
//! Only the host simulates the ball, so every peer works bounces out from where it draws the ball:
//! a bounce is heard when the ball is seen to turn, even on a joiner whose ball is a moment behind
//! the host's. Goals come from the replicated [`PongScore`].

use bevy::prelude::*;

use super::PHASE;
use super::ball::Ball;
use super::config::{BALL_MAX_Y, PADDLE_X_OFFSET};
use super::score::PongScore;
use crate::minigames::sounds::play_sound;

const PADDLE_HIT_SOUND: &str = "minigames/two_player_pong/paddle_hit.wav";
const WALL_BOUNCE_SOUND: &str = "minigames/two_player_pong/wall_bounce.wav";
const GOAL_SOUND: &str = "minigames/two_player_pong/goal.wav";

const PADDLE_HIT_VOLUME: f32 = 0.5;
const WALL_BOUNCE_VOLUME: f32 = 0.4;
const GOAL_VOLUME: f32 = 0.5;

/// The ball only turns back along x at a paddle, so turning further out than this is a paddle hit.
/// Halfway leaves room for the ball being drawn a little short of the paddle on the frame it turns.
const PADDLE_ZONE_X: f32 = PADDLE_X_OFFSET / 2.0;
/// Likewise along y at a wall.
const WALL_ZONE_Y: f32 = BALL_MAX_Y / 2.0;

/// What the ball bounced off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bounce {
    Paddle,
    Wall,
}

/// Where the ball was drawn on the last frame, and which way it last moved along each axis: -1, 1,
/// or 0 before it has.
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
    /// frame, if anything. A ball that turned back at its last position bounced there: along x
    /// near a paddle, along y near a wall. A ball behind a paddle has got past it, so coming
    /// back from there is the ball being served again after a goal, not a bounce.
    pub fn moved_to(self, position: Vec2) -> (Self, Option<Bounce>) {
        let step = position - self.last;
        let turned = |step: f32, heading: f32| step * heading < 0.0;
        let bounce = if self.last.x.abs() > PADDLE_X_OFFSET {
            None
        } else if turned(step.x, self.heading.x) && self.last.x.abs() > PADDLE_ZONE_X {
            Some(Bounce::Paddle)
        } else if turned(step.y, self.heading.y) && self.last.y.abs() > WALL_ZONE_Y {
            Some(Bounce::Wall)
        } else {
            None
        };
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

#[derive(Resource)]
struct PongSounds {
    paddle_hit: Handle<AudioSource>,
    wall_bounce: Handle<AudioSource>,
    goal: Handle<AudioSource>,
}

/// Plays Pong's sounds on every peer.
pub struct PongSoundsPlugin;

impl Plugin for PongSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sounds);
        app.add_systems(
            Update,
            (play_bounce_sounds, play_goal_sound).run_if(in_state(PHASE)),
        );
    }
}

fn load_sounds(assets: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(PongSounds {
        paddle_hit: assets.load(PADDLE_HIT_SOUND),
        wall_bounce: assets.load(WALL_BOUNCE_SOUND),
        goal: assets.load(GOAL_SOUND),
    });
}

/// Follow where the ball is drawn (its `Transform`) and play a sound when it bounces.
fn play_bounce_sounds(
    mut balls: Query<(Entity, &Transform, Option<&mut BallTrack>), With<Ball>>,
    sounds: Res<PongSounds>,
    mut commands: Commands,
) {
    for (entity, transform, track) in &mut balls {
        let position = transform.translation.truncate();
        let Some(mut track) = track else {
            commands.entity(entity).insert(BallTrack::new(position));
            continue;
        };
        let bounce;
        (*track, bounce) = track.moved_to(position);
        match bounce {
            Some(Bounce::Paddle) => {
                play_sound(&mut commands, &sounds.paddle_hit, PADDLE_HIT_VOLUME);
            }
            Some(Bounce::Wall) => {
                play_sound(&mut commands, &sounds.wall_bounce, WALL_BOUNCE_VOLUME);
            }
            None => {}
        }
    }
}

/// Play the goal sound when either side's score goes up. `seen` is the score this peer last saw,
/// so a new game's 0 – 0 makes no sound.
fn play_goal_sound(
    score: Option<Single<&PongScore>>,
    sounds: Res<PongSounds>,
    mut seen: Local<Option<PongScore>>,
    mut commands: Commands,
) {
    let Some(score) = score.map(|score| **score) else {
        return;
    };
    let goals = |score: PongScore| score.left + score.right;
    if seen.is_some_and(|seen| goals(score) > goals(seen)) {
        play_sound(&mut commands, &sounds.goal, GOAL_VOLUME);
    }
    *seen = Some(score);
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;

    use super::{BallTrack, Bounce};
    use crate::minigames::two_player_pong::config::{BALL_MAX_Y, GOAL_X, PADDLE_X_OFFSET};

    /// What `BallTrack` heard while the ball was drawn at each of `positions` in turn.
    fn bounces(positions: &[Vec2]) -> Vec<Option<Bounce>> {
        let mut track = BallTrack::new(positions[0]);
        positions[1..]
            .iter()
            .map(|&position| {
                let bounce;
                (track, bounce) = track.moved_to(position);
                bounce
            })
            .collect()
    }

    #[test]
    fn a_ball_moving_straight_on_makes_no_sound() {
        let path = [Vec2::ZERO, Vec2::new(5.0, 2.0), Vec2::new(10.0, 4.0)];
        assert_eq!(bounces(&path), [None, None]);
    }

    #[test]
    fn turning_back_at_a_paddle_is_a_paddle_hit() {
        let x = PADDLE_X_OFFSET - 10.0;
        let path = [
            Vec2::new(x - 10.0, 0.0),
            Vec2::new(x, 2.0),
            Vec2::new(x - 10.0, 4.0),
            Vec2::new(x - 20.0, 6.0),
        ];
        assert_eq!(bounces(&path), [None, Some(Bounce::Paddle), None]);
    }

    #[test]
    fn turning_back_at_a_wall_is_a_wall_bounce() {
        let y = -BALL_MAX_Y;
        let path = [
            Vec2::new(0.0, y + 5.0),
            Vec2::new(5.0, y),
            Vec2::new(10.0, y + 5.0),
        ];
        assert_eq!(bounces(&path), [None, Some(Bounce::Wall)]);
    }

    #[test]
    fn a_frame_where_the_ball_stays_put_doesnt_count_as_turning() {
        let x = PADDLE_X_OFFSET - 10.0;
        let path = [
            Vec2::new(x - 10.0, 0.0),
            Vec2::new(x, 0.0),
            Vec2::new(x, 0.0),
            Vec2::new(x - 10.0, 0.0),
        ];
        assert_eq!(bounces(&path), [None, None, Some(Bounce::Paddle)]);
    }

    #[test]
    fn being_served_again_after_a_goal_is_not_a_bounce() {
        // The host puts the ball back in the centre on the tick it goes out, so it's last drawn
        // short of the goal line, down by the wall. Then it's served from the centre: going
        // the other way along x, and along y.
        let near_goal = |x: f32| Vec2::new(x, -BALL_MAX_Y + 5.0);
        let served = [Vec2::ZERO, Vec2::new(-5.0, 2.0), Vec2::new(-10.0, 4.0)];
        let on_host = [near_goal(GOAL_X - 10.0), near_goal(GOAL_X - 2.0)];
        assert_eq!(bounces(&[on_host.as_slice(), &served].concat()), [None; 4]);
        // A joiner draws it sliding back to the centre.
        let on_joiner = [
            near_goal(GOAL_X - 10.0),
            near_goal(GOAL_X - 2.0),
            Vec2::new(GOAL_X / 2.0, -BALL_MAX_Y / 2.0),
        ];
        assert_eq!(
            bounces(&[on_joiner.as_slice(), &served].concat()),
            [None; 5]
        );
    }
}
