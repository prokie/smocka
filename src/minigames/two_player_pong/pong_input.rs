use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use bevy_enhanced_input::prelude::{Action, InputAction, InputContextAppExt, TriggerState};
use lightyear::input::client::InputSystems;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PHASE;
use crate::controls::{ControlAction, Controls, fired, spawn_action};
use crate::minigames::two_player_pong::paddles::Paddle;

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

impl PongInput {
    /// The input for a player moving up, down, both or neither. Both at once stays still.
    pub fn from_actions(up: bool, down: bool) -> Self {
        match (up, down) {
            (true, false) => PongInput::Up,
            (false, true) => PongInput::Down,
            _ => PongInput::Idle,
        }
    }
}

/// Local only: this player's Pong [`Controls`], read into a [`PongInput`] each tick. Spawned on
/// every peer for the game; a spectator's is never read.
#[derive(Component)]
struct PongControls;

#[derive(InputAction)]
#[action_output(bool)]
struct PaddleUp;

#[derive(InputAction)]
#[action_output(bool)]
struct PaddleDown;

/// Buffers this player's paddle input for the host.
pub struct PongInputPlugin;

impl Plugin for PongInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_input_context::<PongControls>();
        app.add_systems(OnEnter(PHASE), spawn_pong_controls);
        app.add_systems(
            Update,
            spawn_pong_controls.run_if(
                in_state(PHASE)
                    .and_then(resource_changed::<Controls>)
                    .and_then(not(resource_added::<Controls>)),
            ),
        );
        app.add_systems(
            FixedPreUpdate,
            buffer_pong_input
                .in_set(InputSystems::WriteClientInputs)
                .run_if(in_state(PHASE)),
        );
        app.add_plugins(input::native::InputPlugin::<PongInput>::default());

        app.add_observer(mark_controlled_paddle);
    }
}

/// The game's [`PongControls`], bound to this player's current [`Controls`]. Rebuilt if controls
/// are changed mid-game.
fn spawn_pong_controls(
    controls: Res<Controls>,
    existing: Query<Entity, With<PongControls>>,
    mut commands: Commands,
) {
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let context = commands
        .spawn((
            Name::new("PongControls"),
            PongControls,
            DespawnOnExit(PHASE),
        ))
        .id();
    spawn_action::<PongControls, PaddleUp>(
        &mut commands,
        context,
        controls.get(ControlAction::PaddleUp),
    );
    spawn_action::<PongControls, PaddleDown>(
        &mut commands,
        context,
        controls.get(ControlAction::PaddleDown),
    );
}

/// Only the locally controlled paddle has an [`InputMarker`]; spectators skip this system.
fn buffer_pong_input(
    up: Single<&TriggerState, With<Action<PaddleUp>>>,
    down: Single<&TriggerState, With<Action<PaddleDown>>>,
    mut paddle: Single<&mut ActionState<PongInput>, With<InputMarker<PongInput>>>,
) {
    paddle.0 = PongInput::from_actions(fired(&up), fired(&down));
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
