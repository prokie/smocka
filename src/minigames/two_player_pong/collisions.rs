use bevy::math::Vec2;

use super::config::{BALL_MAX_BOUNCE_ANGLE, BALL_MAX_Y, BALL_SIZE, PADDLE_HEIGHT, PADDLE_WIDTH};
use crate::minigames::pong_paddle::{
    PaddleGeometry, PaddleShape, bounce_off_paddle_shape, hits_paddle_shape,
};

/// Bounce the ball off the top and bottom walls. The left and right edges are open (goals).
pub fn bounce_off_walls(mut position: Vec2, mut velocity: Vec2) -> (Vec2, Vec2) {
    if position.y.abs() > BALL_MAX_Y {
        position.y = position.y.clamp(-BALL_MAX_Y, BALL_MAX_Y);
        velocity.y = -velocity.y;
    }
    (position, velocity)
}

fn geometry(paddle: Vec2, shape: PaddleShape) -> PaddleGeometry {
    PaddleGeometry {
        centre: paddle,
        inward: Vec2::new(-paddle.x.signum(), 0.0),
        tangent: Vec2::Y,
        shape,
        base_length: PADDLE_HEIGHT,
        thickness: PADDLE_WIDTH,
    }
}

/// Whether the ball, which was at `previous` when this tick began and is now at `ball`, hits the
/// front face of a [`PaddleShape::Classic`] paddle at `paddle`.
#[cfg(test)]
pub fn hits_paddle(previous: Vec2, ball: Vec2, velocity: Vec2, paddle: Vec2) -> bool {
    hits_paddle_with_shape(previous, ball, velocity, paddle, PaddleShape::Classic)
}

/// Shape-aware version of [`hits_paddle`].
pub fn hits_paddle_with_shape(
    previous: Vec2,
    ball: Vec2,
    velocity: Vec2,
    paddle: Vec2,
    shape: PaddleShape,
) -> bool {
    hits_paddle_shape(geometry(paddle, shape), previous, ball, velocity, BALL_SIZE)
}

/// Same speed, aimed by where it hit a [`PaddleShape::Classic`] paddle: the centre sends it
/// straight across, the ends at up to [`BALL_MAX_BOUNCE_ANGLE`].
#[cfg(test)]
pub fn bounce_off_paddle(ball: Vec2, velocity: Vec2, paddle: Vec2) -> Vec2 {
    bounce_off_paddle_with_shape(ball, velocity, paddle, PaddleShape::Classic)
}

/// Shape-aware version of [`bounce_off_paddle`].
pub fn bounce_off_paddle_with_shape(
    ball: Vec2,
    velocity: Vec2,
    paddle: Vec2,
    shape: PaddleShape,
) -> Vec2 {
    bounce_off_paddle_shape(
        geometry(paddle, shape),
        ball,
        velocity,
        BALL_MAX_BOUNCE_ANGLE,
    )
}

#[cfg(test)]
mod tests {
    use super::{bounce_off_paddle, bounce_off_walls, hits_paddle};
    use crate::minigames::two_player_pong::config::{
        BALL_MAX_BOUNCE_ANGLE, BALL_MAX_Y, BALL_SIZE, GOAL_X, PADDLE_HEIGHT, PADDLE_WIDTH,
        PADDLE_X_OFFSET,
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
    fn passes_through_the_left_edge() {
        let position = Vec2::new(-GOAL_X - 1.0, 0.0);
        let velocity = Vec2::new(-100.0, 10.0);
        assert_eq!(bounce_off_walls(position, velocity), (position, velocity));
    }

    #[test]
    fn leaves_a_ball_inside_the_arena_alone() {
        let (position, velocity) = bounce_off_walls(Vec2::new(5.0, 5.0), Vec2::new(1.0, 1.0));
        assert_eq!(
            (position, velocity),
            (Vec2::new(5.0, 5.0), Vec2::new(1.0, 1.0))
        );
    }

    /// A step of 2 units to the left, for a ball arriving at the left paddle.
    const STEP_LEFT: Vec2 = Vec2::new(-2.0, 0.0);

    /// `ball` reached this tick, moving left from 2 units to its right.
    fn arriving_at(ball: Vec2) -> (Vec2, Vec2) {
        (ball - STEP_LEFT, ball)
    }

    #[test]
    fn ball_touching_the_paddle_face_hits_it() {
        // Ball just touching the right-hand face of the left paddle, moving left.
        let (previous, ball) =
            arriving_at(LEFT_PADDLE + Vec2::new((PADDLE_WIDTH + BALL_SIZE) / 2.0, 0.0));
        assert!(hits_paddle(
            previous,
            ball,
            Vec2::new(-100.0, 0.0),
            LEFT_PADDLE
        ));
    }

    #[test]
    fn ball_moving_away_does_not_hit() {
        assert!(!hits_paddle(
            LEFT_PADDLE + Vec2::new(10.0, 0.0),
            LEFT_PADDLE,
            Vec2::new(100.0, 0.0),
            LEFT_PADDLE
        ));
    }

    #[test]
    fn ball_above_the_paddle_misses() {
        let (previous, ball) =
            arriving_at(LEFT_PADDLE + Vec2::new(0.0, PADDLE_HEIGHT / 2.0 + BALL_SIZE));
        assert!(!hits_paddle(
            previous,
            ball,
            Vec2::new(-100.0, 0.0),
            LEFT_PADDLE
        ));
    }

    #[test]
    fn a_ball_already_past_the_face_is_not_hit_by_the_paddle_moving_into_it() {
        // Level with the middle of the paddle, and the one at its back.
        for x in [0.0, -(PADDLE_WIDTH + BALL_SIZE) / 2.0 + 1.0] {
            let ball = LEFT_PADDLE + Vec2::new(x, 0.0);
            assert!(
                !hits_paddle(ball - STEP_LEFT, ball, Vec2::new(-100.0, 0.0), LEFT_PADDLE),
                "ball {x} from the paddle's centre"
            );
        }
    }

    #[test]
    fn the_right_paddle_cant_hit_from_behind_either() {
        let right_paddle = -LEFT_PADDLE;
        let ball = right_paddle;
        let previous = ball - Vec2::new(2.0, 0.0);
        assert!(!hits_paddle(
            previous,
            ball,
            Vec2::new(100.0, 0.0),
            right_paddle
        ));
    }

    /// Where the ball is when it touches the left paddle's face, `y` above the paddle's centre.
    fn on_left_face(y: f32) -> Vec2 {
        LEFT_PADDLE + Vec2::new((PADDLE_WIDTH + BALL_SIZE) / 2.0, y)
    }

    /// The angle of `velocity` away from straight across, in radians (positive is upwards).
    fn angle_from_horizontal(velocity: Vec2) -> f32 {
        (velocity.y / velocity.x.abs()).atan()
    }

    #[test]
    fn a_centre_hit_goes_straight_back() {
        let velocity = bounce_off_paddle(on_left_face(0.0), Vec2::new(-100.0, 60.0), LEFT_PADDLE);
        assert!(velocity.x > 0.0);
        assert!(velocity.y.abs() < 1e-4);
    }

    #[test]
    fn an_end_hit_goes_off_at_the_max_angle() {
        let top = bounce_off_paddle(
            on_left_face(PADDLE_HEIGHT / 2.0),
            Vec2::new(-100.0, -60.0),
            LEFT_PADDLE,
        );
        assert!((angle_from_horizontal(top) - BALL_MAX_BOUNCE_ANGLE).abs() < 1e-4);

        let bottom = bounce_off_paddle(
            on_left_face(-PADDLE_HEIGHT / 2.0),
            Vec2::new(-100.0, 60.0),
            LEFT_PADDLE,
        );
        assert!((angle_from_horizontal(bottom) + BALL_MAX_BOUNCE_ANGLE).abs() < 1e-4);
    }

    #[test]
    fn a_ball_past_the_end_counts_as_hitting_the_end() {
        let past = on_left_face(PADDLE_HEIGHT / 2.0 + BALL_SIZE / 2.0);
        let velocity = bounce_off_paddle(past, Vec2::new(-100.0, 0.0), LEFT_PADDLE);
        assert!((angle_from_horizontal(velocity) - BALL_MAX_BOUNCE_ANGLE).abs() < 1e-4);
    }

    #[test]
    fn a_bounce_keeps_the_speed() {
        let incoming = Vec2::new(-120.0, 50.0);
        let velocity = bounce_off_paddle(on_left_face(10.0), incoming, LEFT_PADDLE);
        assert!((velocity.length() - incoming.length()).abs() < 1e-3);
    }

    #[test]
    fn the_right_paddle_sends_the_ball_left() {
        let right_paddle = -LEFT_PADDLE;
        let ball = right_paddle + Vec2::new(-(PADDLE_WIDTH + BALL_SIZE) / 2.0, 12.0);
        let velocity = bounce_off_paddle(ball, Vec2::new(100.0, 0.0), right_paddle);
        assert!(velocity.x < 0.0);
        assert!(velocity.y > 0.0);
    }

    #[test]
    fn works_for_the_right_paddle_too() {
        let right_paddle = -LEFT_PADDLE;
        let face = right_paddle - Vec2::new((PADDLE_WIDTH + BALL_SIZE) / 2.0, 0.0);
        let previous = face - Vec2::new(2.0, 0.0);
        assert!(hits_paddle(
            previous,
            face,
            Vec2::new(100.0, 0.0),
            right_paddle
        ));
        assert!(!hits_paddle(
            previous,
            face,
            Vec2::new(-100.0, 0.0),
            right_paddle
        ));
    }
}
