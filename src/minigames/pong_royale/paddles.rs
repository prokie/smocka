//! Each player's paddle: whose it is, which side it defends, how far along that side it is, and
//! how many lives its player has left.

use bevy::prelude::*;
use lightyear::prelude::input::native::ActionState;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::arena::{Side, step_paddle_with_shape};
use super::royale_input::RoyaleInput;
use super::{PHASE, RoyaleSystems};
use crate::minigames::pong_paddle::PaddleShape;
use crate::players::Player;

/// Who plays this paddle, which side it defends, and what shape it has. Set once at spawn, and
/// copied from the player's [`Player`] so a result can still name them if they leave.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Paddle {
    pub owner: PeerId,
    pub name: String,
    /// The owner's [`Player::color`], an index for [`player_color`](crate::players::player_color).
    pub color: u8,
    pub side: Side,
    pub shape: PaddleShape,
}

/// How far along its side the paddle is (see [`Side::paddle_centre`]). Changes every tick:
/// predicted for its owner, interpolated for everyone else.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct PaddlePosition(pub f32);

impl Ease for PaddlePosition {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            PaddlePosition(start.0.lerp(end.0, t))
        })
    }
}

/// How many lives the paddle's player has left. Only changes on the host. At none, the paddle
/// goes and a wall takes its place.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lives(pub u32);

/// Registers the paddles' replicated components, and moves them.
pub struct PaddlePlugin;

impl Plugin for PaddlePlugin {
    fn build(&self, app: &mut App) {
        app.component::<Paddle>().replicate();
        app.component::<PaddlePosition>()
            .replicate()
            .predict()
            .add_linear_interpolation();
        app.component::<Lives>().replicate();

        app.add_systems(FixedUpdate, move_paddles.in_set(RoyaleSystems::MovePaddles));
    }
}

/// Host only: a paddle for `player` on `side`, with `lives`. `link` is the player's link to the
/// host, and `host` the host's own player, if it has one.
///
/// The host's own player is left out of [`InterpolationTarget`]: the host holds every paddle as
/// it really is, and would otherwise mark the others interpolated and never move them smoothly.
pub fn spawn_paddle(
    commands: &mut Commands,
    player: &Player,
    link: Entity,
    side: Side,
    shape: PaddleShape,
    lives: u32,
    host: Option<PeerId>,
) {
    let owner = player.id;
    commands.spawn((
        Paddle {
            owner,
            name: player.name.clone(),
            color: player.color,
            side,
            shape,
        },
        PaddlePosition(0.0),
        Lives(lives),
        DespawnOnExit(PHASE),
        Replicate::to_clients(NetworkTarget::All),
        PredictionTarget::to_clients(NetworkTarget::Single(owner)),
        InterpolationTarget::to_clients(NetworkTarget::from_exclude(
            [Some(owner), host].into_iter().flatten(),
        )),
        // Despawned when its player's client disconnects.
        ControlledBy {
            owner: link,
            lifetime: Lifetime::default(),
        },
    ));
}

/// Runs on the host for every paddle, and on each player's machine for their own (the one it
/// predicts). Everyone else's is interpolated there, so it's left alone. Lightyear re-runs it
/// during rollback, so it only reads the input and the position.
fn move_paddles(
    time: Res<Time>,
    mut paddles: Query<
        (&Paddle, &mut PaddlePosition, &ActionState<RoyaleInput>),
        Without<Interpolated>,
    >,
) {
    for (paddle, mut position, input) in &mut paddles {
        let along = step_paddle_with_shape(
            position.0,
            paddle.shape,
            input.0.along(paddle.side),
            time.delta_secs(),
        );
        position.set_if_neq(PaddlePosition(along));
    }
}
