use bevy::prelude::*;
use lightyear::prelude::input::native::ActionState;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PHASE;
use super::PongSystems;
use super::config::{PADDLE_MAX_Y, PADDLE_SPEED, PADDLE_X_OFFSET};
use super::pong_input::PongInput;
use crate::minigames::outcome::Participants;
use crate::network::is_host;
use crate::players::Player;

/// Which player controls this paddle. Set once at spawn.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Paddle {
    pub owner: PeerId,
    pub side: Side,
    /// The owner's [`Player::color`], an index for [`player_color`](crate::players::player_color).
    pub color: u8,
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

        app.add_systems(OnEnter(PHASE), spawn_paddles.run_if(is_host));
        app.add_systems(
            FixedUpdate,
            move_paddles
                .in_set(PongSystems::MovePaddles)
                .run_if(in_state(PHASE)),
        );
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

    /// The other side of the arena.
    pub fn opponent(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// The two [`Participants`] get a paddle, left then right; everyone else watches. If one has left
/// already, Pong is abandoned for lack of paddles.
fn spawn_paddles(
    mut commands: Commands,
    participants: Single<&Participants>,
    players: Query<(&Player, &ControlledBy)>,
) {
    let seated: Vec<_> = participants
        .0
        .iter()
        .filter_map(|id| players.iter().find(|(player, _)| player.id == *id))
        .collect();
    let [(left, left_link), (right, right_link)] = seated[..] else {
        info!(
            "Pong needs 2 players, but {} of its {} participants are here",
            seated.len(),
            participants.0.len()
        );
        return;
    };
    spawn_paddle(&mut commands, left, left_link.owner, Side::Left);
    spawn_paddle(&mut commands, right, right_link.owner, Side::Right);
}

fn spawn_paddle(commands: &mut Commands, player: &Player, link: Entity, side: Side) {
    let owner = player.id;
    commands.spawn((
        Paddle {
            owner,
            side,
            color: player.color,
        },
        DespawnOnExit(PHASE),
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

/// Runs on the host for every paddle and on each client for its own predicted one. Lightyear
/// re-runs it during rollback, so it must only depend on the input and the current position.
fn move_paddles(
    time: Res<Time>,
    mut paddles: Query<(&mut PaddlePosition, &ActionState<PongInput>), With<Paddle>>,
) {
    for (mut position, input) in &mut paddles {
        let y = step(position.0.y, input.0, time.delta_secs());
        position.set_if_neq(PaddlePosition(Vec2::new(position.0.x, y)));
    }
}

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
    use crate::minigames::two_player_pong::config::{PADDLE_MAX_Y, PADDLE_SPEED};

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
