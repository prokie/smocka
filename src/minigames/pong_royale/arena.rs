//! The arena's shape, and the ball's physics in it: the same pure functions on every peer, and
//! simple enough to test without an app.
//!
//! Each [`Side`] is worked out the same way, in its own terms: how far out from the centre
//! something is towards that side's goal (along its [`normal`](Side::normal)), and how far along
//! the side (its [`tangent`](Side::tangent)).

use bevy::math::Vec2;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::config::{
    ARENA_HALF, BALL_HIT_SPEEDUP, BALL_MAX_BOUNCE_ANGLE, BALL_MAX_SPEED, BALL_SERVE_MAX_ANGLE,
    BALL_SERVE_MIN_ANGLE, BALL_SERVE_SPEED, BALL_SIZE, CORNER_SIZE, GOAL_DISTANCE, GOAL_HALF_WIDTH,
    PADDLE_DISTANCE, PADDLE_LENGTH, PADDLE_THICKNESS, WALL_THICKNESS,
};
use crate::minigames::pong_paddle::{
    PaddleGeometry, PaddleShape, bounce_off_paddle_shape, hits_paddle_shape,
};

/// A side of the square arena, each with a goal for one player to defend.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    Bottom,
    Top,
}

impl Side {
    /// Every side, in the order players are seated: two face each other across the wide screen
    /// first, then the bottom, then the top.
    pub const ALL: [Side; 4] = [Side::Left, Side::Right, Side::Bottom, Side::Top];

    /// The way out of the arena through this side's goal.
    pub fn normal(self) -> Vec2 {
        match self {
            Side::Left => Vec2::NEG_X,
            Side::Right => Vec2::X,
            Side::Bottom => Vec2::NEG_Y,
            Side::Top => Vec2::Y,
        }
    }

    /// The way along this side: up for the left and right, right for the bottom and top.
    pub fn tangent(self) -> Vec2 {
        match self {
            Side::Left | Side::Right => Vec2::Y,
            Side::Bottom | Side::Top => Vec2::X,
        }
    }

    /// Where the centre of this side's paddle is, `along` its side.
    pub fn paddle_centre(self, along: f32) -> Vec2 {
        self.normal() * PADDLE_DISTANCE + self.tangent() * along
    }

    /// An arrow pointing at this side, for labels.
    pub fn arrow(self) -> &'static str {
        match self {
            Side::Left => "←",
            Side::Right => "→",
            Side::Bottom => "↓",
            Side::Top => "↑",
        }
    }
}

/// The sides `players` players sit at, in [`Side::ALL`]'s order. The rest are walled off.
pub fn seats(players: usize) -> &'static [Side] {
    &Side::ALL[..players.min(Side::ALL.len())]
}

/// A [`PaddleShape::Classic`] paddle's position `along` its side after moving for `dt` seconds
/// with `input` (-1, 0 or 1 along its [`tangent`](Side::tangent)), kept between the corner blocks.
pub fn step_paddle(along: f32, input: f32, dt: f32) -> f32 {
    step_paddle_with_shape(along, PaddleShape::Classic, input, dt)
}

/// Shape-aware version of [`step_paddle`], clamping so the shape's outer ends stay between the
/// corner blocks.
pub fn step_paddle_with_shape(along: f32, shape: PaddleShape, input: f32, dt: f32) -> f32 {
    let max_along = GOAL_HALF_WIDTH - shape.half_length(PADDLE_LENGTH);
    (along + input.clamp(-1.0, 1.0) * super::config::PADDLE_SPEED * dt).clamp(-max_along, max_along)
}

/// The velocity of a ball served from the centre towards `side`'s goal: [`BALL_SERVE_SPEED`], at
/// a random angle between [`BALL_SERVE_MIN_ANGLE`] and [`BALL_SERVE_MAX_ANGLE`] off straight at
/// it, to either side.
pub fn serve_velocity(side: Side, rng: &mut impl RngExt) -> Vec2 {
    let angle = rng.random_range(BALL_SERVE_MIN_ANGLE..=BALL_SERVE_MAX_ANGLE);
    let either_way = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
    (side.normal() * angle.cos() + side.tangent() * either_way * angle.sin()) * BALL_SERVE_SPEED
}

/// What's at each side of the arena this tick: a paddle (`along`, shape), or a wall. A side with
/// neither is an open goal.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sides {
    paddles: [Option<(f32, PaddleShape)>; 4],
    walls: [bool; 4],
}

impl Sides {
    fn index(side: Side) -> usize {
        side as usize
    }

    pub fn with_paddle(self, side: Side, along: f32) -> Self {
        self.with_shaped_paddle(side, along, PaddleShape::Classic)
    }

    pub fn with_shaped_paddle(mut self, side: Side, along: f32, shape: PaddleShape) -> Self {
        self.paddles[Self::index(side)] = Some((along, shape));
        self
    }

    pub fn with_wall(mut self, side: Side) -> Self {
        self.walls[Self::index(side)] = true;
        self
    }

    pub fn paddle(&self, side: Side) -> Option<(f32, PaddleShape)> {
        self.paddles[Self::index(side)]
    }

    pub fn walled(&self, side: Side) -> bool {
        self.walls[Self::index(side)]
    }
}

/// The ball after a tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub position: Vec2,
    pub velocity: Vec2,
    /// The side whose goal the ball went into this tick, if it did.
    pub goal: Option<Side>,
}

/// The ball's position and velocity after moving for `dt` seconds from `position` at `velocity`,
/// bouncing off walls, corner blocks and paddles, and whether it went into a goal.
pub fn step_ball(position: Vec2, velocity: Vec2, sides: &Sides, dt: f32) -> Step {
    let previous = position;
    let mut position = position + velocity * dt;
    let mut velocity = velocity;

    for side in Side::ALL {
        if sides.walled(side) {
            (position, velocity) = bounce_off_wall(side, position, velocity);
        }
    }
    for corner in CORNERS {
        (position, velocity) = bounce_off_block(corner, position, velocity);
    }
    for side in Side::ALL {
        if let Some((along, shape)) = sides.paddle(side)
            && hits_paddle_shaped(side, along, shape, previous, position, velocity)
        {
            velocity = bounce_off_paddle_shaped(side, along, shape, position, velocity);
            velocity = (velocity * BALL_HIT_SPEEDUP).clamp_length_max(BALL_MAX_SPEED);
        }
    }
    let goal = Side::ALL
        .into_iter()
        .find(|&side| !sides.walled(side) && position.dot(side.normal()) > GOAL_DISTANCE);
    Step {
        position,
        velocity,
        goal,
    }
}

/// The furthest out the ball's centre gets towards a walled side: its edge against the wall.
const WALL_REACH: f32 = ARENA_HALF - WALL_THICKNESS - BALL_SIZE / 2.0;

/// Bounce the ball back off `side`'s wall if it has reached it.
pub fn bounce_off_wall(side: Side, position: Vec2, velocity: Vec2) -> (Vec2, Vec2) {
    let normal = side.normal();
    let out = position.dot(normal);
    if out <= WALL_REACH {
        return (position, velocity);
    }
    let position = position - normal * (out - WALL_REACH);
    let outwards = velocity.dot(normal);
    let velocity = if outwards > 0.0 {
        velocity - normal * 2.0 * outwards
    } else {
        velocity
    };
    (position, velocity)
}

/// An axis-aligned block the ball bounces off: its centre and half its size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub centre: Vec2,
    pub half_size: Vec2,
}

/// The four corner blocks, between neighbouring goals.
pub const CORNERS: [Block; 4] = {
    let centre = ARENA_HALF - CORNER_SIZE / 2.0;
    let half_size = Vec2::splat(CORNER_SIZE / 2.0);
    [
        Block {
            centre: Vec2::new(-centre, -centre),
            half_size,
        },
        Block {
            centre: Vec2::new(centre, -centre),
            half_size,
        },
        Block {
            centre: Vec2::new(-centre, centre),
            half_size,
        },
        Block {
            centre: Vec2::new(centre, centre),
            half_size,
        },
    ]
};

/// Push the ball out of `block` if it overlaps it, the shortest way, and bounce it off that face.
pub fn bounce_off_block(block: Block, position: Vec2, velocity: Vec2) -> (Vec2, Vec2) {
    let reach = block.half_size + Vec2::splat(BALL_SIZE / 2.0);
    let offset = position - block.centre;
    let overlap = reach - offset.abs();
    if overlap.x <= 0.0 || overlap.y <= 0.0 {
        return (position, velocity);
    }
    let (mut position, mut velocity) = (position, velocity);
    if overlap.x < overlap.y {
        let away = offset.x.signum();
        position.x += away * overlap.x;
        if velocity.x * away < 0.0 {
            velocity.x = -velocity.x;
        }
    } else {
        let away = offset.y.signum();
        position.y += away * overlap.y;
        if velocity.y * away < 0.0 {
            velocity.y = -velocity.y;
        }
    }
    (position, velocity)
}

fn paddle_geometry(side: Side, along: f32, shape: PaddleShape) -> PaddleGeometry {
    PaddleGeometry {
        centre: side.paddle_centre(along),
        inward: -side.normal(),
        tangent: side.tangent(),
        shape,
        base_length: PADDLE_LENGTH,
        thickness: PADDLE_THICKNESS,
    }
}

/// Whether the ball, which was at `previous` when this tick began and is now at `ball`, hits the
/// front face of `side`'s [`PaddleShape::Classic`] paddle, `along` its side.
pub fn hits_paddle(side: Side, along: f32, previous: Vec2, ball: Vec2, velocity: Vec2) -> bool {
    hits_paddle_shaped(side, along, PaddleShape::Classic, previous, ball, velocity)
}

/// Shape-aware version of [`hits_paddle`].
pub fn hits_paddle_shaped(
    side: Side,
    along: f32,
    shape: PaddleShape,
    previous: Vec2,
    ball: Vec2,
    velocity: Vec2,
) -> bool {
    hits_paddle_shape(
        paddle_geometry(side, along, shape),
        previous,
        ball,
        velocity,
        BALL_SIZE,
    )
}

/// The ball's velocity off `side`'s [`PaddleShape::Classic`] paddle, `along` its side: same speed,
/// back into the arena, aimed by where it hit.
pub fn bounce_off_paddle(side: Side, along: f32, ball: Vec2, velocity: Vec2) -> Vec2 {
    bounce_off_paddle_shaped(side, along, PaddleShape::Classic, ball, velocity)
}

/// Shape-aware version of [`bounce_off_paddle`].
pub fn bounce_off_paddle_shaped(
    side: Side,
    along: f32,
    shape: PaddleShape,
    ball: Vec2,
    velocity: Vec2,
) -> Vec2 {
    bounce_off_paddle_shape(
        paddle_geometry(side, along, shape),
        ball,
        velocity,
        BALL_MAX_BOUNCE_ANGLE,
    )
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;

    use super::{
        CORNERS, Side, Sides, bounce_off_block, bounce_off_paddle, hits_paddle, seats,
        serve_velocity, step_ball, step_paddle,
    };
    use crate::config::network::TICK_RATE;
    use crate::minigames::pong_royale::config::{
        ARENA_HALF, BALL_MAX_BOUNCE_ANGLE, BALL_MAX_SPEED, BALL_SERVE_MAX_ANGLE,
        BALL_SERVE_MIN_ANGLE, BALL_SERVE_SPEED, BALL_SIZE, GOAL_HALF_WIDTH, PADDLE_DISTANCE,
        PADDLE_LENGTH, PADDLE_MAX_ALONG, PADDLE_THICKNESS, WALL_THICKNESS,
    };

    const DT: f32 = 1.0 / TICK_RATE as f32;

    /// Where the ball's centre is when it just touches the face of `side`'s paddle, `along` it.
    fn at_face(side: Side, along: f32) -> Vec2 {
        side.paddle_centre(along) - side.normal() * (PADDLE_THICKNESS + BALL_SIZE) / 2.0
    }

    #[test]
    fn sides_point_out_and_along_at_right_angles() {
        for side in Side::ALL {
            assert_eq!(side.normal().length(), 1.0);
            assert_eq!(side.normal().dot(side.tangent()), 0.0, "{side:?}");
        }
    }

    #[test]
    fn two_players_face_each_other_and_four_fill_every_side() {
        assert_eq!(seats(2), [Side::Left, Side::Right]);
        assert_eq!(seats(3), [Side::Left, Side::Right, Side::Bottom]);
        assert_eq!(seats(4), Side::ALL);
        assert_eq!(seats(6), Side::ALL);
    }

    #[test]
    fn paddles_move_along_their_side_and_stop_at_the_corners() {
        assert!(step_paddle(0.0, 1.0, DT) > 0.0);
        assert!(step_paddle(0.0, -1.0, DT) < 0.0);
        assert_eq!(step_paddle(0.0, 0.0, DT), 0.0);
        assert_eq!(step_paddle(PADDLE_MAX_ALONG, 1.0, DT), PADDLE_MAX_ALONG);
        // At the end of its travel, the paddle's end meets the corner block.
        assert_eq!(PADDLE_MAX_ALONG + PADDLE_LENGTH / 2.0, GOAL_HALF_WIDTH);
    }

    #[test]
    fn a_serve_heads_for_the_sides_goal_at_an_allowed_angle() {
        let mut rng = rand::rng();
        for side in Side::ALL {
            for _ in 0..50 {
                let velocity = serve_velocity(side, &mut rng);
                assert!((velocity.length() - BALL_SERVE_SPEED).abs() < 1e-3);
                let angle = velocity.angle_to(side.normal()).abs();
                assert!(
                    (BALL_SERVE_MIN_ANGLE - 1e-4..=BALL_SERVE_MAX_ANGLE + 1e-4).contains(&angle),
                    "{side:?}: {angle}"
                );
            }
        }
    }

    #[test]
    fn the_ball_bounces_off_a_walled_side() {
        for side in Side::ALL {
            let sides = Sides::default().with_wall(side);
            let near_wall = side.normal() * (ARENA_HALF - WALL_THICKNESS - BALL_SIZE / 2.0 - 1.0);
            let velocity = side.normal() * 300.0;
            let step = step_ball(near_wall, velocity, &sides, DT);
            assert!(step.velocity.dot(side.normal()) < 0.0, "{side:?}");
            assert_eq!(step.goal, None, "{side:?}");
        }
    }

    #[test]
    fn a_ball_over_an_open_goal_line_is_a_goal_for_that_side() {
        for side in Side::ALL {
            let just_inside = side.normal() * (ARENA_HALF + BALL_SIZE / 2.0 - 1.0);
            let step = step_ball(just_inside, side.normal() * 300.0, &Sides::default(), DT);
            assert_eq!(step.goal, Some(side));
            // Not yet fully over.
            let step = step_ball(
                side.normal() * (ARENA_HALF - 10.0),
                side.normal() * 300.0,
                &Sides::default(),
                DT,
            );
            assert_eq!(step.goal, None);
        }
    }

    #[test]
    fn the_ball_bounces_off_a_corner_block() {
        let corner = CORNERS[3]; // Top right.
        // Coming up into its bottom face, from below.
        let below = corner.centre - Vec2::new(0.0, corner.half_size.y + BALL_SIZE / 2.0 - 1.0);
        let (position, velocity) = bounce_off_block(corner, below, Vec2::new(0.0, 100.0));
        assert_eq!(velocity, Vec2::new(0.0, -100.0));
        assert!(position.y < below.y);
        // Coming in from the left, into its left face.
        let left = corner.centre - Vec2::new(corner.half_size.x + BALL_SIZE / 2.0 - 1.0, 0.0);
        let (_, velocity) = bounce_off_block(corner, left, Vec2::new(100.0, 0.0));
        assert_eq!(velocity, Vec2::new(-100.0, 0.0));
        // Clear of it.
        let clear = Vec2::ZERO;
        assert_eq!(
            bounce_off_block(corner, clear, Vec2::ONE),
            (clear, Vec2::ONE)
        );
    }

    #[test]
    fn every_sides_paddle_sends_the_ball_back_into_the_arena() {
        for side in Side::ALL {
            let ball = at_face(side, 0.0);
            let previous = ball - side.normal() * 2.0;
            let velocity = side.normal() * 300.0;
            assert!(hits_paddle(side, 0.0, previous, ball, velocity), "{side:?}");
            let bounced = bounce_off_paddle(side, 0.0, ball, velocity);
            assert!(
                (bounced + side.normal() * 300.0).length() < 1e-3,
                "{side:?}"
            );
        }
    }

    #[test]
    fn hitting_a_paddles_end_angles_the_ball_towards_that_end() {
        for side in Side::ALL {
            let along = 20.0;
            let ball = at_face(side, along) + side.tangent() * PADDLE_LENGTH / 2.0;
            let bounced = bounce_off_paddle(side, along, ball, side.normal() * 300.0);
            assert!(bounced.dot(side.tangent()) > 0.0, "{side:?}");
            let angle = bounced.angle_to(-side.normal()).abs();
            assert!((angle - BALL_MAX_BOUNCE_ANGLE).abs() < 1e-4, "{side:?}");
        }
    }

    #[test]
    fn a_ball_beyond_the_paddles_end_or_already_past_it_misses() {
        for side in Side::ALL {
            let velocity = side.normal() * 300.0;
            let beyond = at_face(side, 0.0) + side.tangent() * (PADDLE_LENGTH + BALL_SIZE);
            assert!(!hits_paddle(
                side,
                0.0,
                beyond - side.normal() * 2.0,
                beyond,
                velocity
            ));
            let behind = side.paddle_centre(0.0);
            assert!(!hits_paddle(
                side,
                0.0,
                behind - side.normal() * 2.0,
                behind,
                velocity
            ));
            // Heading back in, just after a bounce.
            let ball = at_face(side, 0.0);
            assert!(!hits_paddle(side, 0.0, ball, ball, -velocity));
        }
    }

    #[test]
    fn a_paddle_hit_speeds_the_ball_up_to_its_limit() {
        let sides = Sides::default().with_paddle(Side::Left, 0.0);
        let ball = at_face(Side::Left, 0.0) + Vec2::new(1.0, 0.0);
        for speed in [200.0, BALL_MAX_SPEED] {
            let step = step_ball(ball, Vec2::new(-speed, 0.0), &sides, DT);
            assert!(step.velocity.x > 0.0);
            assert!(step.velocity.length() > speed.min(BALL_MAX_SPEED - 1.0));
            assert!(step.velocity.length() <= BALL_MAX_SPEED + 1e-3);
        }
        // The paddle is where `paddle_centre` says.
        assert_eq!(
            Side::Left.paddle_centre(0.0),
            Vec2::new(-PADDLE_DISTANCE, 0.0)
        );
    }
}
