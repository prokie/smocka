//! Collisions between the ball and the arena walls or the paddles.
//!
//! Plain functions on positions and velocities, with no ECS, so they are easy to unit-test. The
//! ball systems in [`ball`](super::ball) call them every tick on the host.

use bevy::math::Vec2;
use bevy::shape::{Aabb2d, IntersectsVolume};

use crate::config::minigames::two_player_pong::{
    BALL_MAX_X, BALL_MAX_Y, BALL_SIZE, PADDLE_HEIGHT, PADDLE_WIDTH,
};

/// Bounce the ball off the edges of the arena.
///
/// If the ball has passed an edge, it is put back on that edge and its velocity along that axis
/// flips. The left and right edges bounce too for now; they will become goals once scoring exists.
pub fn bounce_off_walls(mut position: Vec2, mut velocity: Vec2) -> (Vec2, Vec2) {
    if position.y.abs() > BALL_MAX_Y {
        position.y = position.y.clamp(-BALL_MAX_Y, BALL_MAX_Y);
        velocity.y = -velocity.y;
    }
    if position.x.abs() > BALL_MAX_X {
        position.x = position.x.clamp(-BALL_MAX_X, BALL_MAX_X);
        velocity.x = -velocity.x;
    }
    (position, velocity)
}

/// Whether the ball at `ball` should bounce off the paddle at `paddle`.
///
/// It must overlap the paddle and be moving towards the edge that paddle guards (left paddle:
/// moving left, right paddle: moving right). The direction check stops a ball that is already
/// bouncing away from being caught by the same paddle again on the next tick, and lets a ball
/// that got behind a paddle pass through it instead of bouncing off its back.
pub fn hits_paddle(ball: Vec2, velocity: Vec2, paddle: Vec2) -> bool {
    let ball_box = Aabb2d::new(ball, Vec2::splat(BALL_SIZE / 2.0));
    let paddle_box = Aabb2d::new(paddle, Vec2::new(PADDLE_WIDTH, PADDLE_HEIGHT) / 2.0);
    let moving_towards_its_edge = velocity.x * paddle.x > 0.0;
    moving_towards_its_edge && ball_box.intersects(&paddle_box)
}

#[cfg(test)]
mod tests {
    use super::{bounce_off_walls, hits_paddle};
    use crate::config::minigames::two_player_pong::{
        BALL_MAX_X, BALL_MAX_Y, BALL_SIZE, PADDLE_HEIGHT, PADDLE_WIDTH, PADDLE_X_OFFSET,
    };
    use bevy::math::Vec2;

    /// Where the left paddle is when it is centred vertically.
    const LEFT_PADDLE: Vec2 = Vec2::new(-PADDLE_X_OFFSET, 0.0);

    #[test]
    fn bounces_off_the_top_wall() {
        let (position, velocity) =
            bounce_off_walls(Vec2::new(0.0, BALL_MAX_Y + 1.0), Vec2::new(10.0, 100.0));
        assert_eq!(position.y, BALL_MAX_Y);
        assert_eq!(velocity, Vec2::new(10.0, -100.0));
    }

    #[test]
    fn bounces_off_the_left_edge() {
        let (position, velocity) =
            bounce_off_walls(Vec2::new(-BALL_MAX_X - 1.0, 0.0), Vec2::new(-100.0, 10.0));
        assert_eq!(position.x, -BALL_MAX_X);
        assert_eq!(velocity, Vec2::new(100.0, 10.0));
    }

    #[test]
    fn leaves_a_ball_inside_the_arena_alone() {
        let (position, velocity) = bounce_off_walls(Vec2::new(5.0, 5.0), Vec2::new(1.0, 1.0));
        assert_eq!(
            (position, velocity),
            (Vec2::new(5.0, 5.0), Vec2::new(1.0, 1.0))
        );
    }

    #[test]
    fn ball_touching_the_paddle_face_hits_it() {
        // Ball just touching the right-hand face of the left paddle, moving left.
        let ball = LEFT_PADDLE + Vec2::new((PADDLE_WIDTH + BALL_SIZE) / 2.0, 0.0);
        assert!(hits_paddle(ball, Vec2::new(-100.0, 0.0), LEFT_PADDLE));
    }

    #[test]
    fn ball_moving_away_does_not_hit() {
        assert!(!hits_paddle(
            LEFT_PADDLE,
            Vec2::new(100.0, 0.0),
            LEFT_PADDLE
        ));
    }

    #[test]
    fn ball_above_the_paddle_misses() {
        let ball = LEFT_PADDLE + Vec2::new(0.0, PADDLE_HEIGHT / 2.0 + BALL_SIZE);
        assert!(!hits_paddle(ball, Vec2::new(-100.0, 0.0), LEFT_PADDLE));
    }

    #[test]
    fn works_for_the_right_paddle_too() {
        let right_paddle = -LEFT_PADDLE;
        assert!(hits_paddle(
            right_paddle,
            Vec2::new(100.0, 0.0),
            right_paddle
        ));
        assert!(!hits_paddle(
            right_paddle,
            Vec2::new(-100.0, 0.0),
            right_paddle
        ));
    }
}
