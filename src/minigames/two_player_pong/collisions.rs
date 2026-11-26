use bevy::math::Vec2;
use bevy::shape::{Aabb2d, IntersectsVolume};

use crate::config::minigames::two_player_pong::{
    BALL_MAX_BOUNCE_ANGLE, BALL_MAX_Y, BALL_SIZE, PADDLE_HEIGHT, PADDLE_WIDTH,
};

/// Bounce the ball off the top and bottom walls. The left and right edges are open (goals).
pub fn bounce_off_walls(mut position: Vec2, mut velocity: Vec2) -> (Vec2, Vec2) {
    if position.y.abs() > BALL_MAX_Y {
        position.y = position.y.clamp(-BALL_MAX_Y, BALL_MAX_Y);
        velocity.y = -velocity.y;
    }
    (position, velocity)
}

/// Whether the ball, which was at `previous` when this tick began and is now at `ball`, hits the
/// paddle's front face. It must overlap the paddle, move towards the edge the paddle guards, and
/// have been fully in front of the face when the tick began.
///
/// That last check means a ball that already got past the face goes on into the goal, even when
/// the paddle moves into it from above or below: a paddle can't hit with its ends or its back.
/// The direction check stops a ball that just bounced from being caught again.
pub fn hits_paddle(previous: Vec2, ball: Vec2, velocity: Vec2, paddle: Vec2) -> bool {
    let ball_box = Aabb2d::new(ball, Vec2::splat(BALL_SIZE / 2.0));
    let paddle_box = Aabb2d::new(paddle, Vec2::new(PADDLE_WIDTH, PADDLE_HEIGHT) / 2.0);
    let moving_towards_its_edge = velocity.x * paddle.x > 0.0;
    // From the paddle towards the middle of the arena: +1 for the left paddle.
    let inwards = -paddle.x.signum();
    let face = paddle.x + inwards * PADDLE_WIDTH / 2.0;
    let leading_edge_before = previous.x - inwards * BALL_SIZE / 2.0;
    let was_in_front = (leading_edge_before - face) * inwards >= 0.0;
    moving_towards_its_edge && was_in_front && ball_box.intersects(&paddle_box)
}

/// Same speed, aimed by where it hit: the centre sends it straight across, the ends at up to
/// [`BALL_MAX_BOUNCE_ANGLE`].
pub fn bounce_off_paddle(ball: Vec2, velocity: Vec2, paddle: Vec2) -> Vec2 {
    // -1 at the bottom end of the paddle, 0 at its centre, 1 at the top end. A ball overlapping
    // just past an end counts as hitting that end.
    let offset = ((ball.y - paddle.y) / (PADDLE_HEIGHT / 2.0)).clamp(-1.0, 1.0);
    let angle = offset * BALL_MAX_BOUNCE_ANGLE;
    // Away from the edge this paddle guards: the left paddle sends the ball right.
    let away = -paddle.x.signum();
    Vec2::new(away * angle.cos(), angle.sin()) * velocity.length()
}

#[cfg(test)]
mod tests {
    use super::{bounce_off_paddle, bounce_off_walls, hits_paddle};
    use crate::config::minigames::two_player_pong::{
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
