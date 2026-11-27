use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use lightyear::input::client::InputSystems;
use lightyear::prelude::input::InputConfig;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::car::Car;
use crate::session::SessionPhase;

/// What a player is doing with their car this tick.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Reflect)]
pub struct RacingInput {
    pub throttle: bool,
    /// Slows down, then reverses once stopped.
    pub brake: bool,
    /// `1` steers left (anticlockwise), `-1` right, `0` straight. Anything else counts as the
    /// nearest of those.
    pub steer: i8,
}

impl MapEntities for RacingInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

/// Buffers this player's driving, and has the host pass every driver's input on to the others, who
/// predict every car (see [`bumps`](super::bumps)).
pub struct RacingInputPlugin;

impl Plugin for RacingInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedPreUpdate,
            buffer_racing_input
                .in_set(InputSystems::WriteClientInputs)
                .run_if(in_state(SessionPhase::Racing)),
        );
        app.add_plugins(input::native::InputPlugin::<RacingInput> {
            config: InputConfig {
                rebroadcast_inputs: true,
                ..default()
            },
        });

        app.add_observer(mark_controlled_car);
    }
}

/// Only the locally controlled car has an [`InputMarker`]; without one this system is skipped.
fn buffer_racing_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut car: Single<&mut ActionState<RacingInput>, With<InputMarker<RacingInput>>>,
) {
    let left = keys.any_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]);
    let right = keys.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD]);
    car.0 = RacingInput {
        throttle: keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]),
        brake: keys.any_pressed([KeyCode::ArrowDown, KeyCode::KeyS]),
        steer: i8::from(left) - i8::from(right),
    };
}

fn mark_controlled_car(
    trigger: On<Add<Controlled>>,
    cars: Query<(), With<Car>>,
    mut commands: Commands,
) {
    // `Controlled` is used for other things too (the lobby player, paddles), so only react to cars.
    if cars.contains(trigger.entity) {
        commands
            .entity(trigger.entity)
            .insert(InputMarker::<RacingInput>::default());
    }
}
