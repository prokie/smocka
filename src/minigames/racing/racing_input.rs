//! The input a player sends each tick to drive their car, and the keyboard reading that fills it.

use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use lightyear::input::client::InputSystems;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::car::Car;
use crate::session::SessionPhase;

/// What a player is doing with their car this tick.
///
/// A struct rather than an enum (like Pong's input), because a driver can hold the throttle and
/// steer at the same time.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Reflect)]
pub struct RacingInput {
    /// Speed up forwards.
    pub throttle: bool,
    /// Slow down, then reverse once stopped.
    pub brake: bool,
    /// `1` steers left (anticlockwise), `-1` right, `0` straight. Anything else counts as the
    /// nearest of those.
    pub steer: i8,
}

impl MapEntities for RacingInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

/// Registers [`RacingInput`] with Lightyear and reads the keyboard into it.
pub struct RacingInputPlugin;

impl Plugin for RacingInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedPreUpdate,
            buffer_racing_input
                .in_set(InputSystems::WriteClientInputs)
                .run_if(in_state(SessionPhase::Racing)),
        );
        app.add_plugins(input::native::InputPlugin::<RacingInput>::default());

        app.add_observer(mark_controlled_car);
    }
}

/// Read the keyboard and record this tick's [`RacingInput`] for the car this machine controls.
///
/// Only the locally controlled car has an [`InputMarker`], so `Single` matches at most one entity.
/// If there is none (this player has no car), the system is skipped.
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

/// When this machine gains control of a car, mark it as the one that receives local keyboard input.
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
