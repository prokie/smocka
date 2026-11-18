//! The ball: spawned and moved by the host, replicated to every client.
//!
//! The host is the only peer that decides where the ball is. Clients interpolate it: they show
//! it slightly in the past, smoothly moving between the positions the host sends.

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PongSystems;
use super::collisions::{bounce_off_walls, hits_paddle};
use super::paddles::{Paddle, PaddlePosition, Side};
use crate::config::minigames::two_player_pong::{
    BALL_HIT_SPEEDUP, BALL_MAX_SPEED, BALL_SERVE_VELOCITY,
};
use crate::network::is_host;
use crate::session::SessionPhase;

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

/// How fast and in which direction the ball moves, in world units per second.
///
/// Host only: it is not registered for replication, so clients never receive it. They only need
/// the [`BallPosition`] to draw the ball.
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

        app.add_systems(
            OnEnter(SessionPhase::TwoPlayerPong),
            spawn_ball.run_if(is_host),
        );
        app.add_systems(
            FixedUpdate,
            move_ball
                .in_set(PongSystems::MoveBall)
                .run_if(is_host)
                .run_if(in_state(SessionPhase::TwoPlayerPong)),
        );
    }
}

/// Host only: when Pong starts, spawn the ball in the centre of the arena, served to the right.
fn spawn_ball(mut commands: Commands) {
    commands.spawn((
        Ball,
        DespawnOnExit(SessionPhase::TwoPlayerPong),
        BallPosition(Vec2::ZERO),
        BallVelocity(serve_velocity(Side::Right)),
        Replicate::to_clients(NetworkTarget::All),
        InterpolationTarget::to_clients(NetworkTarget::All),
    ));
}

/// Host only: move the ball one tick and bounce it off the walls and paddles.
fn move_ball(
    time: Res<Time>,
    mut balls: Query<(&mut BallPosition, &mut BallVelocity)>,
    paddles: Query<&PaddlePosition, With<Paddle>>,
) {
    let paddles = paddles.iter().map(|paddle| paddle.0);
    for (mut position, mut velocity) in &mut balls {
        (position.0, velocity.0) =
            step_ball(position.0, velocity.0, paddles.clone(), time.delta_secs());
    }
}

/// The velocity of a ball served from the centre towards `side`'s paddle.
pub fn serve_velocity(side: Side) -> Vec2 {
    Vec2::new(
        BALL_SERVE_VELOCITY.x * side.x_pos().signum(),
        BALL_SERVE_VELOCITY.y,
    )
}

/// The ball's next position and velocity after moving for `dt` seconds past `paddles`.
///
/// The ball moves in a straight line, then bounces off the walls (see [`bounce_off_walls`]) and
/// off the first paddle it hits (see [`hits_paddle`]). A paddle hit flips the ball's horizontal
/// direction and makes it [`BALL_HIT_SPEEDUP`] times faster, up to [`BALL_MAX_SPEED`].
fn step_ball(
    position: Vec2,
    velocity: Vec2,
    paddles: impl IntoIterator<Item = Vec2>,
    dt: f32,
) -> (Vec2, Vec2) {
    let (position, mut velocity) = bounce_off_walls(position + velocity * dt, velocity);

    if paddles
        .into_iter()
        .any(|paddle| hits_paddle(position, velocity, paddle))
    {
        velocity.x = -velocity.x;
        velocity = (velocity * BALL_HIT_SPEEDUP).clamp_length_max(BALL_MAX_SPEED);
    }

    (position, velocity)
}

#[cfg(test)]
mod tests {
    use super::{serve_velocity, step_ball};
    use crate::config::minigames::two_player_pong::{
        BALL_HIT_SPEEDUP, BALL_MAX_SPEED, BALL_MAX_Y, PADDLE_X_OFFSET,
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
        let (_, velocity) = step_ball(start, Vec2::new(-100.0, 20.0), [paddle], 0.1);
        assert_eq!(velocity, Vec2::new(100.0, 20.0) * BALL_HIT_SPEEDUP);
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
    fn serves_towards_the_given_side() {
        assert!(serve_velocity(Side::Left).x < 0.0);
        assert!(serve_velocity(Side::Right).x > 0.0);
    }
}
