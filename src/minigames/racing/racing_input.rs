use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use bevy_enhanced_input::prelude::{Action, InputAction, InputContextAppExt, TriggerState};
use lightyear::input::client::InputSystems;
use lightyear::prelude::input::InputConfig;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PHASE;
use super::car::Car;
use crate::controls::{ControlAction, Controls, fired, spawn_action};

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

impl RacingInput {
    /// The input for a player doing these actions. Steering both ways at once goes straight.
    pub fn from_actions(throttle: bool, brake: bool, left: bool, right: bool) -> Self {
        Self {
            throttle,
            brake,
            steer: i8::from(left) - i8::from(right),
        }
    }
}

/// Local only: this player's racing [`Controls`], read into a [`RacingInput`] each tick. Spawned on
/// every peer for the race; a spectator's is never read.
#[derive(Component)]
struct RacingControls;

#[derive(InputAction)]
#[action_output(bool)]
struct Throttle;

#[derive(InputAction)]
#[action_output(bool)]
struct Brake;

#[derive(InputAction)]
#[action_output(bool)]
struct SteerLeft;

#[derive(InputAction)]
#[action_output(bool)]
struct SteerRight;

/// Buffers this player's driving, and has the host pass every driver's input on to the others, who
/// predict every car (see [`bumps`](super::bumps)).
pub struct RacingInputPlugin;

impl Plugin for RacingInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_input_context::<RacingControls>();
        app.add_systems(OnEnter(PHASE), spawn_racing_controls);
        app.add_systems(
            FixedPreUpdate,
            buffer_racing_input
                .in_set(InputSystems::WriteClientInputs)
                .run_if(in_state(PHASE)),
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

/// The race's [`RacingControls`], bound to this player's [`Controls`] as they are when it starts.
fn spawn_racing_controls(controls: Res<Controls>, mut commands: Commands) {
    let context = commands
        .spawn((
            Name::new("RacingControls"),
            RacingControls,
            DespawnOnExit(PHASE),
        ))
        .id();
    let bound = |action| controls.get(action);
    spawn_action::<RacingControls, Throttle>(
        &mut commands,
        context,
        bound(ControlAction::Throttle),
    );
    spawn_action::<RacingControls, Brake>(&mut commands, context, bound(ControlAction::Brake));
    spawn_action::<RacingControls, SteerLeft>(
        &mut commands,
        context,
        bound(ControlAction::SteerLeft),
    );
    spawn_action::<RacingControls, SteerRight>(
        &mut commands,
        context,
        bound(ControlAction::SteerRight),
    );
}

/// Only the locally controlled car has an [`InputMarker`]; without one this system is skipped.
fn buffer_racing_input(
    throttle: Single<&TriggerState, With<Action<Throttle>>>,
    brake: Single<&TriggerState, With<Action<Brake>>>,
    left: Single<&TriggerState, With<Action<SteerLeft>>>,
    right: Single<&TriggerState, With<Action<SteerRight>>>,
    mut car: Single<&mut ActionState<RacingInput>, With<InputMarker<RacingInput>>>,
) {
    car.0 = RacingInput::from_actions(fired(&throttle), fired(&brake), fired(&left), fired(&right));
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
