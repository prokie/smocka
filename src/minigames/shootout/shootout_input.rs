//! Reading this player's shootout controls into a [`ShootoutInput`] each tick, for the host.

use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use bevy_enhanced_input::prelude::{Action, InputAction, InputContextAppExt, TriggerState};
use lightyear::input::client::InputSystems;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PHASE;
use super::gunner::Gunner;
use crate::controls::{ControlAction, Controls, fired, spawn_action};

/// What a player is doing with their gunner this tick.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Reflect)]
pub struct ShootoutInput {
    /// Which way to walk: `1` right, `-1` left, `0` neither.
    pub walk_x: i8,
    /// `1` up the screen, `-1` down, `0` neither.
    pub walk_y: i8,
}

impl MapEntities for ShootoutInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

impl ShootoutInput {
    /// The input for a player holding these directions. Opposite directions at once cancel out.
    pub fn from_actions(up: bool, down: bool, left: bool, right: bool) -> Self {
        Self {
            walk_x: i8::from(right) - i8::from(left),
            walk_y: i8::from(up) - i8::from(down),
        }
    }

    /// Which way to walk, as a vector: each part `-1`, `0` or `1` (anything else counts as its
    /// sign). Not normalized.
    pub fn walk(self) -> Vec2 {
        Vec2::new(
            f32::from(self.walk_x.signum()),
            f32::from(self.walk_y.signum()),
        )
    }
}

/// Local only: this player's shootout [`Controls`], read into a [`ShootoutInput`] each tick.
/// Spawned on every peer for the shootout; a spectator's is never read.
#[derive(Component)]
struct ShootoutControls;

#[derive(InputAction)]
#[action_output(bool)]
struct MoveUp;

#[derive(InputAction)]
#[action_output(bool)]
struct MoveDown;

#[derive(InputAction)]
#[action_output(bool)]
struct MoveLeft;

#[derive(InputAction)]
#[action_output(bool)]
struct MoveRight;

/// Buffers this player's input for the host. Nobody else predicts a player's gunner, so the host
/// doesn't pass inputs on.
pub struct ShootoutInputPlugin;

impl Plugin for ShootoutInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_input_context::<ShootoutControls>();
        app.add_systems(OnEnter(PHASE), spawn_shootout_controls);
        app.add_systems(
            FixedPreUpdate,
            buffer_shootout_input
                .in_set(InputSystems::WriteClientInputs)
                .run_if(in_state(PHASE)),
        );
        app.add_plugins(input::native::InputPlugin::<ShootoutInput>::default());

        app.add_observer(mark_controlled_gunner);
    }
}

/// The shootout's [`ShootoutControls`], bound to this player's [`Controls`] as they are when it
/// starts.
fn spawn_shootout_controls(controls: Res<Controls>, mut commands: Commands) {
    let context = commands
        .spawn((
            Name::new("ShootoutControls"),
            ShootoutControls,
            DespawnOnExit(PHASE),
        ))
        .id();
    let bound = |action| controls.get(action);
    spawn_action::<ShootoutControls, MoveUp>(&mut commands, context, bound(ControlAction::MoveUp));
    spawn_action::<ShootoutControls, MoveDown>(
        &mut commands,
        context,
        bound(ControlAction::MoveDown),
    );
    spawn_action::<ShootoutControls, MoveLeft>(
        &mut commands,
        context,
        bound(ControlAction::MoveLeft),
    );
    spawn_action::<ShootoutControls, MoveRight>(
        &mut commands,
        context,
        bound(ControlAction::MoveRight),
    );
}

/// Only the locally controlled gunner has an [`InputMarker`]; without one (a spectator, or a
/// player knocked out) this system is skipped.
fn buffer_shootout_input(
    up: Single<&TriggerState, With<Action<MoveUp>>>,
    down: Single<&TriggerState, With<Action<MoveDown>>>,
    left: Single<&TriggerState, With<Action<MoveLeft>>>,
    right: Single<&TriggerState, With<Action<MoveRight>>>,
    mut gunner: Single<&mut ActionState<ShootoutInput>, With<InputMarker<ShootoutInput>>>,
) {
    gunner.0 = ShootoutInput::from_actions(fired(&up), fired(&down), fired(&left), fired(&right));
}

fn mark_controlled_gunner(
    trigger: On<Add<Controlled>>,
    gunners: Query<(), With<Gunner>>,
    mut commands: Commands,
) {
    // `Controlled` is used for other things too (the lobby player, cars), so only react to
    // gunners.
    if gunners.contains(trigger.entity) {
        commands
            .entity(trigger.entity)
            .insert(InputMarker::<ShootoutInput>::default());
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;

    use super::ShootoutInput;

    #[test]
    fn held_directions_walk_that_way_and_opposites_cancel_out() {
        let walk =
            |up, down, left, right| ShootoutInput::from_actions(up, down, left, right).walk();
        assert_eq!(walk(true, false, false, false), Vec2::new(0.0, 1.0));
        assert_eq!(walk(false, true, true, false), Vec2::new(-1.0, -1.0));
        assert_eq!(walk(true, true, false, true), Vec2::new(1.0, 0.0));
        assert_eq!(walk(true, true, true, true), Vec2::ZERO);
    }
}
