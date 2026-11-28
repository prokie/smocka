use bevy::prelude::*;
use lightyear::prelude::*;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::PHASE;
use super::PongSystems;
use super::collisions::{bounce_off_paddle_with_shape, bounce_off_walls, hits_paddle_with_shape};
use super::config::{
    BALL_HIT_SPEEDUP, BALL_MAX_SPEED, BALL_SERVE_MAX_ANGLE, BALL_SERVE_MIN_ANGLE, BALL_SERVE_SPEED,
};
use super::paddles::{Paddle, PaddlePosition, Side};
use crate::minigames::pong_paddle::PaddleShape;
use crate::network::is_host;

/// Marker for the Pong ball.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Ball;

/// Where the ball is, in world units. Interpolated on clients.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BallPosition(pub Vec2);

impl Ease for BallPosition {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            BallPosition(Vec2::lerp(start.0, end.0, t))
        })
    }
}

/// World units per second. Host only: not replicated, clients only need [`BallPosition`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BallVelocity(pub Vec2);

/// Registers the ball's replicated components and the host's ball systems.
pub struct BallPlugin;

impl Plugin for BallPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Ball>().replicate();

        app.component::<BallPosition>()
            .replicate()
            .add_linear_interpolation();

        app.add_systems(OnEnter(PHASE), spawn_ball.run_if(is_host));
        app.add_systems(
            FixedUpdate,
            move_ball
                .in_set(PongSystems::MoveBall)
                .run_if(is_host)
                .run_if(in_state(PHASE)),
        );
    }
}

/// Spawns the ball in the centre, served to a random side.
fn spawn_ball(mut commands: Commands) {
    let mut rng = rand::rng();
    let side = if rng.random_bool(0.5) {
        Side::Left
    } else {
        Side::Right
    };
    debug!("Pong: first serve goes to {side:?}");
    commands.spawn((
        Ball,
        DespawnOnExit(PHASE),
        BallPosition(Vec2::ZERO),
        BallVelocity(serve_velocity(side, &mut rng)),
        Replicate::to_clients(NetworkTarget::All),
        InterpolationTarget::to_clients(NetworkTarget::All),
    ));
}

fn move_ball(
    time: Res<Time>,
    mut balls: Query<(&mut BallPosition, &mut BallVelocity)>,
    paddles: Query<(&Paddle, &PaddlePosition)>,
) {
    let paddles = paddles
        .iter()
        .map(|(paddle, position)| (position.0, paddle.shape));
    for (mut position, mut velocity) in &mut balls {
        (position.0, velocity.0) =
            step_ball_shaped(position.0, velocity.0, paddles.clone(), time.delta_secs());
    }
}

/// The velocity of a ball served from the centre towards `side`'s paddle: [`BALL_SERVE_SPEED`],
/// at a random angle between [`BALL_SERVE_MIN_ANGLE`] and [`BALL_SERVE_MAX_ANGLE`], up or down.
pub fn serve_velocity(side: Side, rng: &mut impl RngExt) -> Vec2 {
    let angle = rng.random_range(BALL_SERVE_MIN_ANGLE..=BALL_SERVE_MAX_ANGLE);
    let up_or_down = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
    Vec2::new(
        side.x_pos().signum() * angle.cos(),
        up_or_down * angle.sin(),
    ) * BALL_SERVE_SPEED
}

/// The ball's next position and velocity after moving for `dt` seconds past shaped `paddles`.
fn step_ball_shaped(
    position: Vec2,
    velocity: Vec2,
    paddles: impl IntoIterator<Item = (Vec2, PaddleShape)>,
    dt: f32,
) -> (Vec2, Vec2) {
    let previous = position;
    let (position, mut velocity) = bounce_off_walls(position + velocity * dt, velocity);

    if let Some((paddle, shape)) = paddles.into_iter().find(|&(paddle, shape)| {
        hits_paddle_with_shape(previous, position, velocity, paddle, shape)
    }) {
        velocity = bounce_off_paddle_with_shape(position, velocity, paddle, shape);
        velocity = (velocity * BALL_HIT_SPEEDUP).clamp_length_max(BALL_MAX_SPEED);
    }

    (position, velocity)
}

#[cfg(test)]
fn step_ball(
    position: Vec2,
    velocity: Vec2,
    paddles: impl IntoIterator<Item = Vec2>,
    dt: f32,
) -> (Vec2, Vec2) {
    step_ball_shaped(
        position,
        velocity,
        paddles
            .into_iter()
            .map(|paddle| (paddle, PaddleShape::Classic)),
        dt,
    )
}

#[cfg(test)]
mod tests {
    use super::{serve_velocity, step_ball};
    use crate::config::network::TICK_RATE;
    use crate::minigames::two_player_pong::config::{
        BALL_HIT_SPEEDUP, BALL_MAX_SPEED, BALL_MAX_Y, BALL_SERVE_MAX_ANGLE, BALL_SERVE_MIN_ANGLE,
        BALL_SERVE_SPEED, GOAL_X, PADDLE_HEIGHT, PADDLE_X_OFFSET,
    };
    use crate::minigames::two_player_pong::paddles::Side;
    use bevy::math::Vec2;

    const NO_PADDLES: [Vec2; 0] = [];

    #[test]
    fn moves_in_a_straight_line() {
        let (position, velocity) = step_ball(Vec2::ZERO, Vec2::new(100.0, -50.0), NO_PADDLES, 0.5);
        assert_eq!(position, Vec2::new(50.0, -25.0));
        assert_eq!(velocity, Vec2::new(100.0, -50.0));
    }

    #[test]
    fn bounces_off_a_wall() {
        let start = Vec2::new(0.0, BALL_MAX_Y - 1.0);
        let (_, velocity) = step_ball(start, Vec2::new(10.0, 100.0), NO_PADDLES, 0.1);
        assert_eq!(velocity, Vec2::new(10.0, -100.0));
    }

    #[test]
    fn bounces_off_a_paddle_it_reaches_and_speeds_up() {
        let paddle = Vec2::new(-PADDLE_X_OFFSET, 0.0);
        let start = paddle + Vec2::new(10.0, 0.0);
        let incoming = Vec2::new(-100.0, 20.0);
        let (_, velocity) = step_ball(start, incoming, [paddle], 0.1);
        assert!(velocity.x > 0.0);
        assert!((velocity.length() - incoming.length() * BALL_HIT_SPEEDUP).abs() < 1e-3);
    }

    #[test]
    fn a_hit_above_the_centre_goes_up() {
        let paddle = Vec2::new(-PADDLE_X_OFFSET, 0.0);
        // Moving straight left, it reaches the paddle 20 units above its centre.
        let start = paddle + Vec2::new(10.0, 20.0);
        let (_, velocity) = step_ball(start, Vec2::new(-100.0, 0.0), [paddle], 0.1);
        assert!(velocity.x > 0.0);
        assert!(velocity.y > 0.0);
    }

    #[test]
    fn a_paddle_hit_never_exceeds_the_max_speed() {
        let paddle = Vec2::new(-PADDLE_X_OFFSET, 0.0);
        let start = paddle + Vec2::new(10.0, 0.0);
        let (_, velocity) = step_ball(start, Vec2::new(-BALL_MAX_SPEED, 0.0), [paddle], 0.01);
        assert!((velocity.length() - BALL_MAX_SPEED).abs() < 1e-3);
        assert!(velocity.x > 0.0);
    }

    #[test]
    fn passes_a_paddle_it_misses() {
        let paddle = Vec2::new(-PADDLE_X_OFFSET, 100.0);
        let start = Vec2::new(-PADDLE_X_OFFSET + 10.0, 0.0);
        let (_, velocity) = step_ball(start, Vec2::new(-100.0, 20.0), [paddle], 0.1);
        assert_eq!(velocity, Vec2::new(-100.0, 20.0));
    }

    #[test]
    fn a_paddle_moved_into_a_ball_that_got_past_it_lets_it_through() {
        // The ball goes past the top end of the left paddle, then the paddle moves up into it.
        let dt = 1.0 / TICK_RATE as f32;
        let velocity = Vec2::new(-BALL_MAX_SPEED, 0.0);
        let mut ball = Vec2::new(-PADDLE_X_OFFSET + 40.0, PADDLE_HEIGHT);
        let mut ball_velocity = velocity;
        let mut paddle = Vec2::new(-PADDLE_X_OFFSET, 0.0);
        while ball.x > -GOAL_X {
            if ball.x < -PADDLE_X_OFFSET {
                paddle.y = ball.y;
            }
            (ball, ball_velocity) = step_ball(ball, ball_velocity, [paddle], dt);
            assert_eq!(ball_velocity, velocity, "bounced back at {ball}");
        }
    }

    #[test]
    fn serves_towards_the_given_side_at_an_allowed_angle() {
        let mut rng = rand::rng();
        let (mut ups, mut downs) = (0, 0);
        for side in [Side::Left, Side::Right].repeat(100) {
            let velocity = serve_velocity(side, &mut rng);
            assert_eq!(velocity.x.signum(), side.x_pos().signum(), "{velocity}");
            assert!((velocity.length() - BALL_SERVE_SPEED).abs() < 1e-3);
            let angle = velocity.y.abs().atan2(velocity.x.abs());
            assert!(
                (BALL_SERVE_MIN_ANGLE - 1e-5..=BALL_SERVE_MAX_ANGLE + 1e-5).contains(&angle),
                "{angle}"
            );
            if velocity.y > 0.0 {
                ups += 1;
            } else {
                downs += 1;
            }
        }
        // 200 coin flips all landing the same way is about as likely as a 1 in 10^60 event.
        assert!(ups > 0 && downs > 0, "{ups} up, {downs} down");
    }
}
