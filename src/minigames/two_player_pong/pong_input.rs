use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use lightyear::input::client::InputSystems;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use crate::minigames::two_player_pong::paddles::Paddle;
use crate::session::SessionPhase;

/// The input a player sends each tick to control their paddle.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub enum PongInput {
    Up,
    Down,
    #[default]
    Idle,
}

impl MapEntities for PongInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

pub struct PongInputPlugin;

impl Plugin for PongInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedPreUpdate,
            buffer_pong_input
                .in_set(InputSystems::WriteClientInputs)
                .run_if(in_state(SessionPhase::TwoPlayerPong)),
        );
        app.add_plugins(input::native::InputPlugin::<PongInput>::default());

        app.add_observer(mark_controlled_paddle);
    }
}

/// Only the locally controlled paddle has an [`InputMarker`]; spectators skip this system.
fn buffer_pong_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut paddle: Single<&mut ActionState<PongInput>, With<InputMarker<PongInput>>>,
) {
    let up = keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]);
    let down = keys.any_pressed([KeyCode::ArrowDown, KeyCode::KeyS]);
    paddle.0 = match (up, down) {
        (true, false) => PongInput::Up,
        (false, true) => PongInput::Down,
        _ => PongInput::Idle,
    };
}

fn mark_controlled_paddle(
    trigger: On<Add<Controlled>>,
    paddles: Query<(), With<Paddle>>,
    mut commands: Commands,
) {
    // `Controlled` is used for other things too (the lobby player, for example), so only react to paddles.
    if paddles.contains(trigger.entity) {
        commands
            .entity(trigger.entity)
            .insert(InputMarker::<PongInput>::default());
    }
}
