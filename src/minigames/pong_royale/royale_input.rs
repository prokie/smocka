//! Reading this player's Pong Royale controls into a [`RoyaleInput`] each tick, for the host.

use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use bevy_enhanced_input::prelude::{Action, InputAction, InputContextAppExt, TriggerState};
use lightyear::input::client::InputSystems;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PHASE;
use super::arena::Side;
use super::paddles::Paddle;
use crate::controls::{ControlAction, Controls, fired, spawn_action};

/// Which way a player is pushing, each -1, 0 or 1: `x` right, `y` up. A paddle on the left or
/// right moves with `y`, one on the bottom or top with `x`, so players use whichever keys go
/// along their side.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Reflect)]
pub struct RoyaleInput {
    pub x: i8,
    pub y: i8,
}

impl MapEntities for RoyaleInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

impl RoyaleInput {
    /// The input for a player holding up, down, left and right (or not). Opposite ways at once
    /// cancel out.
    pub fn from_actions(up: bool, down: bool, left: bool, right: bool) -> Self {
        let axis = |positive: bool, negative: bool| i8::from(positive) - i8::from(negative);
        Self {
            x: axis(right, left),
            y: axis(up, down),
        }
    }

    /// Which way to move a paddle on `side`: along its [`tangent`](Side::tangent).
    pub fn along(self, side: Side) -> f32 {
        let along = match side {
            Side::Left | Side::Right => self.y,
            Side::Bottom | Side::Top => self.x,
        };
        f32::from(along.clamp(-1, 1))
    }
}

/// Local only: this player's Pong Royale [`Controls`], read into a [`RoyaleInput`] each tick.
/// Spawned on every peer for the game; a spectator's is never read.
#[derive(Component)]
struct RoyaleControls;

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

/// Buffers this player's paddle input for the host.
pub struct RoyaleInputPlugin;

impl Plugin for RoyaleInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_input_context::<RoyaleControls>();
        app.add_systems(OnEnter(PHASE), spawn_royale_controls);
        app.add_systems(
            Update,
            spawn_royale_controls.run_if(
                in_state(PHASE)
                    .and_then(resource_changed::<Controls>)
                    .and_then(not(resource_added::<Controls>)),
            ),
        );
        app.add_systems(
            FixedPreUpdate,
            buffer_royale_input
                .in_set(InputSystems::WriteClientInputs)
                .run_if(in_state(PHASE)),
        );
        app.add_plugins(input::native::InputPlugin::<RoyaleInput>::default());

        app.add_observer(mark_controlled_paddle);
    }
}

/// The game's [`RoyaleControls`], bound to this player's current [`Controls`]. Rebuilt if controls
/// are changed mid-game.
fn spawn_royale_controls(
    controls: Res<Controls>,
    existing: Query<Entity, With<RoyaleControls>>,
    mut commands: Commands,
) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let context = commands
        .spawn((
            Name::new("RoyaleControls"),
            RoyaleControls,
            DespawnOnExit(PHASE),
        ))
        .id();
    let bound = |action| controls.get(action);
    spawn_action::<RoyaleControls, MoveUp>(&mut commands, context, bound(ControlAction::RoyaleUp));
    spawn_action::<RoyaleControls, MoveDown>(
        &mut commands,
        context,
        bound(ControlAction::RoyaleDown),
    );
    spawn_action::<RoyaleControls, MoveLeft>(
        &mut commands,
        context,
        bound(ControlAction::RoyaleLeft),
    );
    spawn_action::<RoyaleControls, MoveRight>(
        &mut commands,
        context,
        bound(ControlAction::RoyaleRight),
    );
}

/// Only this player's own paddle has an [`InputMarker`]; spectators skip this system.
fn buffer_royale_input(
    up: Single<&TriggerState, With<Action<MoveUp>>>,
    down: Single<&TriggerState, With<Action<MoveDown>>>,
    left: Single<&TriggerState, With<Action<MoveLeft>>>,
    right: Single<&TriggerState, With<Action<MoveRight>>>,
    mut paddle: Single<&mut ActionState<RoyaleInput>, With<InputMarker<RoyaleInput>>>,
) {
    paddle.0 = RoyaleInput::from_actions(fired(&up), fired(&down), fired(&left), fired(&right));
}

fn mark_controlled_paddle(
    trigger: On<Add<Controlled>>,
    paddles: Query<(), With<Paddle>>,
    mut commands: Commands,
) {
    // `Controlled` is used for other things too (the lobby player, for example), so only react to
    // paddles.
    if paddles.contains(trigger.entity) {
        commands
            .entity(trigger.entity)
            .insert(InputMarker::<RoyaleInput>::default());
    }
}

#[cfg(test)]
mod tests {
    use super::RoyaleInput;
    use crate::minigames::pong_royale::arena::Side;

    #[test]
    fn opposite_ways_at_once_cancel_out() {
        assert_eq!(
            RoyaleInput::from_actions(true, true, true, false),
            RoyaleInput { x: -1, y: 0 }
        );
    }

    #[test]
    fn side_paddles_move_up_and_down_and_end_paddles_left_and_right() {
        let up_right = RoyaleInput::from_actions(true, false, false, true);
        let up = RoyaleInput::from_actions(true, false, false, false);
        for side in [Side::Left, Side::Right] {
            assert_eq!(up.along(side), 1.0);
        }
        for side in [Side::Bottom, Side::Top] {
            assert_eq!(up.along(side), 0.0);
            assert_eq!(up_right.along(side), 1.0);
        }
    }
}
