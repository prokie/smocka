//! Reading this player's shootout controls into a [`ShootoutInput`] each tick, for the host.

use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_enhanced_input::prelude::{Action, InputAction, InputContextAppExt, TriggerState};
use lightyear::input::client::InputSystems;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PHASE;
use super::gunner::{Gunner, GunnerState, normalize_angle};
use crate::camera::GameplayCamera;
use crate::controls::{ControlAction, Controls, fired, spawn_action};

/// How far the right stick must be pushed to aim (0 to 1). Smaller than
/// [`ACTUATION`](crate::controls::ACTUATION) so aiming feels responsive, while ignoring drift at
/// rest.
pub const AIM_DEADZONE: f32 = 0.25;

/// How far the mouse cursor must move (in logical pixels) to take aim back from the right stick.
const MOUSE_WAKE_DISTANCE: f32 = 1.0;

/// How far the cursor must be from the gunner's centre (in world units) for mouse aim to pick a
/// direction. Closer than this, the gunner keeps its current aim instead of snapping right.
const MOUSE_MIN_DISTANCE: f32 = 0.5;

/// What a player is doing with their gunner this tick.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default, Reflect)]
pub struct ShootoutInput {
    /// Which way to walk: `1` right, `-1` left, `0` neither.
    pub walk_x: i8,
    /// `1` up the screen, `-1` down, `0` neither.
    pub walk_y: i8,
    /// Which way to aim, in radians anticlockwise from facing right (+x), or `None` to keep the
    /// gunner's current aim.
    pub aim: Option<f32>,
    /// Whether the Fire control is held this tick.
    pub fire: bool,
}

impl MapEntities for ShootoutInput {
    fn map_entities<M: EntityMapper>(&mut self, _entity_mapper: &mut M) {}
}

impl ShootoutInput {
    /// The input for a player holding these directions, aiming at `aim`, and optionally firing.
    /// Opposite directions at once cancel out.
    pub fn from_actions(
        up: bool,
        down: bool,
        left: bool,
        right: bool,
        aim: Option<f32>,
        fire: bool,
    ) -> Self {
        Self {
            walk_x: i8::from(right) - i8::from(left),
            walk_y: i8::from(up) - i8::from(down),
            aim,
            fire,
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

    /// The aim angle wrapped into `-π..π`, or `None` if absent or not finite.
    pub fn aim_angle(self) -> Option<f32> {
        self.aim
            .filter(|angle| angle.is_finite())
            .map(normalize_angle)
    }
}

/// Which input is currently aiming this player's gunner. Starts on the mouse, switches to the
/// right stick when it's pushed past [`AIM_DEADZONE`], and switches back to the mouse once the
/// cursor moves.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct AimTracker {
    mode: AimMode,
    /// Where the cursor was last seen in the window (logical pixels), anchored while on the right
    /// stick so even slow cumulative mouse movement wakes mouse aim.
    last_cursor: Option<Vec2>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AimMode {
    #[default]
    Mouse,
    RightStick,
}

impl AimTracker {
    /// Work out which way to aim this tick, in radians anticlockwise from +x, or `None` to keep
    /// the gunner's current aim.
    ///
    /// - `stick` is the active right-stick deflection (see [`right_stick`]).
    /// - `cursor_screen` is `Window::cursor_position()`, used to notice when the mouse moves.
    /// - `cursor_world` is the cursor's position in world units, if it's in the window and the
    ///   gameplay camera is ready.
    /// - `gunner_position` is where this player's gunner is in world units.
    pub fn aim(
        &mut self,
        stick: Vec2,
        cursor_screen: Option<Vec2>,
        cursor_world: Option<Vec2>,
        gunner_position: Vec2,
    ) -> Option<f32> {
        if stick.length() >= AIM_DEADZONE {
            self.mode = AimMode::RightStick;
            self.last_cursor = cursor_screen;
            return Some(stick.to_angle());
        }

        let cursor_moved = match (cursor_screen, self.last_cursor) {
            (Some(now), Some(previous)) => now.distance(previous) >= MOUSE_WAKE_DISTANCE,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if cursor_moved {
            self.mode = AimMode::Mouse;
            self.last_cursor = cursor_screen;
        } else if cursor_screen.is_none() || self.mode == AimMode::Mouse {
            self.last_cursor = cursor_screen;
        }

        match self.mode {
            AimMode::Mouse => {
                let to_cursor = cursor_world? - gunner_position;
                (to_cursor.length() >= MOUSE_MIN_DISTANCE).then(|| to_cursor.to_angle())
            }
            AimMode::RightStick => None,
        }
    }
}

/// The furthest-pushed right stick across `gamepads`, or `Vec2::ZERO` if none are connected.
pub fn right_stick<'a>(gamepads: impl IntoIterator<Item = &'a Gamepad>) -> Vec2 {
    gamepads
        .into_iter()
        .map(Gamepad::right_stick)
        .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))
        .unwrap_or(Vec2::ZERO)
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

#[derive(InputAction)]
#[action_output(bool)]
struct Fire;

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
            AimTracker::default(),
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
    spawn_action::<ShootoutControls, Fire>(&mut commands, context, bound(ControlAction::Fire));
}

/// This player's bound shootout actions, read by [`buffer_shootout_input`].
#[derive(SystemParam)]
struct ShootoutActions<'w, 's> {
    up: Single<'w, 's, &'static TriggerState, With<Action<MoveUp>>>,
    down: Single<'w, 's, &'static TriggerState, With<Action<MoveDown>>>,
    left: Single<'w, 's, &'static TriggerState, With<Action<MoveLeft>>>,
    right: Single<'w, 's, &'static TriggerState, With<Action<MoveRight>>>,
    fire: Single<'w, 's, &'static TriggerState, With<Action<Fire>>>,
}

pub struct AimReading {
    stick: Vec2,
    cursor_screen: Option<Vec2>,
    cursor_world: Option<Vec2>,
}

/// The local devices that can aim a gunner: the primary window's cursor (mapped into world units
/// through the gameplay camera) and any connected gamepads' right sticks.
#[derive(SystemParam)]
pub struct AimDevices<'w, 's> {
    window: Option<Single<'w, 's, &'static Window, With<PrimaryWindow>>>,
    camera:
        Option<Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<GameplayCamera>>>,
    gamepads: Query<'w, 's, &'static Gamepad>,
}

impl AimDevices<'_, '_> {
    /// This frame's right-stick deflection, cursor position in window pixels, and cursor position
    /// in world units (when inside the window and the gameplay camera is ready).
    pub fn read(&self) -> AimReading {
        let cursor_screen = self
            .window
            .as_ref()
            .and_then(|window| window.cursor_position());
        let cursor_world = cursor_screen.and_then(|cursor| {
            let (camera, transform) = self.camera.as_deref().copied()?;
            camera.viewport_to_world_2d(transform, cursor).ok()
        });
        AimReading {
            stick: right_stick(self.gamepads.iter()),
            cursor_screen,
            cursor_world,
        }
    }
}

/// Only the locally controlled gunner has an [`InputMarker`]; without one (a spectator, or a
/// player knocked out) this system is skipped.
fn buffer_shootout_input(
    actions: ShootoutActions,
    mut aim_tracker: Single<&mut AimTracker, With<ShootoutControls>>,
    aim_devices: AimDevices,
    mut gunner: Single<
        (&GunnerState, &mut ActionState<ShootoutInput>),
        With<InputMarker<ShootoutInput>>,
    >,
) {
    let AimReading {
        stick,
        cursor_screen,
        cursor_world,
    } = aim_devices.read();
    let (state, ref mut input) = *gunner;
    let aim = aim_tracker.aim(stick, cursor_screen, cursor_world, state.position);
    input.0 = ShootoutInput::from_actions(
        fired(&actions.up),
        fired(&actions.down),
        fired(&actions.left),
        fired(&actions.right),
        aim,
        fired(&actions.fire),
    );
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
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    use bevy::math::Vec2;
    use bevy::prelude::*;

    use super::{AimTracker, ShootoutInput, right_stick};

    fn close(a: Option<f32>, b: Option<f32>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-5,
            (None, None) => true,
            _ => false,
        }
    }

    #[test]
    fn held_directions_walk_that_way_and_opposites_cancel_out() {
        let walk = |up, down, left, right| {
            ShootoutInput::from_actions(up, down, left, right, None, false).walk()
        };
        assert_eq!(walk(true, false, false, false), Vec2::new(0.0, 1.0));
        assert_eq!(walk(false, true, true, false), Vec2::new(-1.0, -1.0));
        assert_eq!(walk(true, true, false, true), Vec2::new(1.0, 0.0));
        assert_eq!(walk(true, true, true, true), Vec2::ZERO);
    }

    #[test]
    fn aim_angle_wraps_into_range_and_ignores_non_finite() {
        let with = |aim| ShootoutInput { aim, ..default() }.aim_angle();
        assert!(close(with(Some(TAU + FRAC_PI_2)), Some(FRAC_PI_2)));
        assert!(close(with(Some(PI)), Some(-PI)));
        assert_eq!(with(None), None);
        assert_eq!(with(Some(f32::NAN)), None);
        assert_eq!(with(Some(f32::INFINITY)), None);
    }

    #[test]
    fn mouse_aims_from_the_gunner_to_the_cursor() {
        let mut tracker = AimTracker::default();
        let gunner = Vec2::new(10.0, 20.0);
        let aim = tracker.aim(
            Vec2::ZERO,
            Some(Vec2::new(300.0, 200.0)),
            Some(Vec2::new(10.0, 70.0)),
            gunner,
        );
        assert!(close(aim, Some(FRAC_PI_2)), "{aim:?}");

        // Walking past a stationary cursor keeps tracking it.
        let aim = tracker.aim(
            Vec2::ZERO,
            Some(Vec2::new(300.0, 200.0)),
            Some(Vec2::new(10.0, 70.0)),
            Vec2::new(-40.0, 70.0),
        );
        assert!(close(aim, Some(0.0)), "{aim:?}");

        // Cursor right on top of the gunner keeps its current aim instead of snapping right.
        let aim = tracker.aim(
            Vec2::ZERO,
            Some(Vec2::new(300.0, 200.0)),
            Some(gunner + Vec2::new(0.1, 0.0)),
            gunner,
        );
        assert_eq!(aim, None);
    }

    #[test]
    fn right_stick_aims_past_the_deadzone_and_keeps_aim_when_released() {
        let mut tracker = AimTracker::default();
        let screen = Some(Vec2::new(200.0, 150.0));
        let world = Some(Vec2::new(100.0, 0.0));

        // Slight stick drift doesn't take over from the mouse.
        let aim = tracker.aim(Vec2::new(0.1, 0.1), screen, world, Vec2::ZERO);
        assert!(close(aim, Some(0.0)), "{aim:?}");

        // Pushing the right stick up aims up and takes over from the mouse.
        let aim = tracker.aim(Vec2::new(0.0, 0.8), screen, world, Vec2::ZERO);
        assert!(close(aim, Some(FRAC_PI_2)), "{aim:?}");

        // Releasing the stick keeps its aim instead of snapping back to the resting mouse.
        let aim = tracker.aim(Vec2::ZERO, screen, world, Vec2::ZERO);
        assert_eq!(aim, None);
    }

    #[test]
    fn moving_the_mouse_takes_aim_back_from_the_right_stick() {
        let mut tracker = AimTracker::default();
        let resting = Vec2::new(200.0, 150.0);
        let world = Some(Vec2::new(-100.0, 0.0));

        tracker.aim(Vec2::new(0.0, 1.0), Some(resting), world, Vec2::ZERO);
        // Tiny cumulative movement crosses the wake threshold instead of resetting each tick.
        assert_eq!(
            tracker.aim(
                Vec2::ZERO,
                Some(resting + Vec2::new(0.6, 0.0)),
                world,
                Vec2::ZERO
            ),
            None
        );
        let aim = tracker.aim(
            Vec2::ZERO,
            Some(resting + Vec2::new(1.2, 0.0)),
            world,
            Vec2::ZERO,
        );
        assert!(close(aim, Some(PI)), "{aim:?}");
    }

    #[test]
    fn right_stick_picks_the_furthest_pushed_gamepad() {
        assert_eq!(right_stick(std::iter::empty()), Vec2::ZERO);

        let idle = Gamepad::default();
        let mut active = Gamepad::default();
        active.analog_mut().set(GamepadAxis::RightStickX, 0.0);
        active.analog_mut().set(GamepadAxis::RightStickY, -0.9);
        assert_eq!(right_stick([&idle, &active]), Vec2::new(0.0, -0.9));
    }
}
