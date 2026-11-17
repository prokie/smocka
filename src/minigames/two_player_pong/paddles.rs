//! Paddles: who owns them, where they are, and how input moves them.

use bevy::prelude::*;
use lightyear::prelude::input::native::ActionState;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::pong_input::PongInput;
use crate::config::minigames::two_player_pong::{PADDLE_MAX_Y, PADDLE_SPEED, PADDLE_X_OFFSET};
use crate::lobby::Player;

/// Which player controls this paddle. Set once at spawn.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Paddle {
    pub owner: PeerId,
    pub side: Side,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Side {
    Left,
    Right,
}

/// Changes every tick. Predicted for the owner, interpolated for everyone else.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct PaddlePosition(pub Vec2);

impl Ease for PaddlePosition {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            PaddlePosition(Vec2::lerp(start.0, end.0, t))
        })
    }
}

pub struct PaddlePlugin;

impl Plugin for PaddlePlugin {
    fn build(&self, app: &mut App) {
        app.component::<Paddle>().replicate();

        app.component::<PaddlePosition>()
            .replicate()
            .predict()
            .add_linear_interpolation();

        app.add_systems(Update, spawn_paddles);
        app.add_systems(FixedUpdate, move_paddles);
    }
}

impl Side {
    /// Returns the x position of the paddle based on its side.
    pub fn x_pos(self) -> f32 {
        match self {
            Side::Left => -PADDLE_X_OFFSET,
            Side::Right => PADDLE_X_OFFSET,
        }
    }
}

/// Host only: once two players are in the session, give each a paddle.
fn spawn_paddles(
    mut commands: Commands,
    players: Query<(&Player, &ControlledBy)>,
    paddles: Query<(), With<Paddle>>,
) {
    if !paddles.is_empty() || players.iter().len() < 2 {
        return;
    }
    let mut players: Vec<_> = players.iter().collect();
    players.sort_by_key(|(player, _)| player.id);
    let [(left, left_link), (right, right_link), ..] = players[..] else {
        return;
    };
    spawn_paddle(&mut commands, left.id, left_link.owner, Side::Left);
    spawn_paddle(&mut commands, right.id, right_link.owner, Side::Right);
}

fn spawn_paddle(commands: &mut Commands, owner: PeerId, link: Entity, side: Side) {
    commands.spawn((
        Paddle { owner, side },
        PaddlePosition(Vec2::new(side.x_pos(), 0.0)),
        Replicate::to_clients(NetworkTarget::All),
        PredictionTarget::to_clients(NetworkTarget::Single(owner)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(owner)),
        ControlledBy {
            owner: link,
            lifetime: Lifetime::default(),
        },
    ));
}

/// Move every paddle that has input according to this tick's [`PongInput`].
///
/// Runs on the host for every paddle (inputs from remote players arrive as [`ActionState`]) and on
/// each client for its own predicted paddle. Lightyear re-runs it during rollback, so it must only
/// depend on the input and the current position. Other players' paddles on a client have no
/// `ActionState` and are interpolated instead.
fn move_paddles(
    time: Res<Time>,
    mut paddles: Query<(&mut PaddlePosition, &ActionState<PongInput>), With<Paddle>>,
) {
    for (mut position, input) in &mut paddles {
        position.0.y = step(position.0.y, input.0, time.delta_secs());
    }
}

/// The paddle's next `y` after moving for `dt` seconds with `input`, kept inside the arena.
fn step(y: f32, input: PongInput, dt: f32) -> f32 {
    let direction = match input {
        PongInput::Up => 1.0,
        PongInput::Down => -1.0,
        PongInput::Idle => 0.0,
    };
    (y + direction * PADDLE_SPEED * dt).clamp(-PADDLE_MAX_Y, PADDLE_MAX_Y)
}

#[cfg(test)]
mod tests {
    use super::{PongInput, Side, step};
    use crate::config::display::VIEW_WIDTH;
    use crate::config::minigames::two_player_pong::{PADDLE_MAX_Y, PADDLE_SPEED};

    #[test]
    fn paddles_are_inside_the_view() {
        for side in [Side::Left, Side::Right] {
            assert!(side.x_pos().abs() < VIEW_WIDTH / 2.0);
        }
    }

    #[test]
    fn step_moves_in_the_input_direction() {
        assert_eq!(step(0.0, PongInput::Up, 0.5), PADDLE_SPEED * 0.5);
        assert_eq!(step(0.0, PongInput::Down, 0.5), -PADDLE_SPEED * 0.5);
        assert_eq!(step(10.0, PongInput::Idle, 0.5), 10.0);
    }

    #[test]
    fn step_stays_inside_the_arena() {
        assert_eq!(step(PADDLE_MAX_Y, PongInput::Up, 1.0), PADDLE_MAX_Y);
        assert_eq!(step(-PADDLE_MAX_Y, PongInput::Down, 1.0), -PADDLE_MAX_Y);
    }
}
