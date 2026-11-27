//! This player's controls: which keys, gamepad buttons and stick directions do what in each
//! minigame.
//!
//! They're read with [`bevy_enhanced_input`] on this machine only. Each minigame turns its actions
//! into its own small input ([`PongInput`](crate::minigames::two_player_pong::PongInput),
//! [`RacingInput`](crate::minigames::racing::RacingInput),
//! [`ShootoutInput`](crate::minigames::shootout::ShootoutInput)) and sends that to the host, so
//! the network never sees which controls anyone uses.
//!
//! Players change them on the main menu's Controls screen, and they're remembered between launches
//! (see [`Controls::load`]).

use bevy::input::gamepad::{GamepadConnection, GamepadConnectionEvent};
use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use serde::{Deserialize, Serialize};

use crate::minigames::Minigame;

/// How far a gamepad trigger must be pressed, or a stick pushed, to count. Keys are either fully
/// pressed or not.
pub const ACTUATION: f32 = 0.5;

/// How many controls each action can have.
pub const SLOTS: usize = 3;

/// The key that leaves a session, and cancels picking a control. It can't be bound to anything.
pub const RESERVED_KEY: KeyCode = KeyCode::Escape;

/// Something a player can do in a minigame, and so bind controls to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlAction {
    /// Pong: move the paddle up.
    PaddleUp,
    /// Pong: move the paddle down.
    PaddleDown,
    /// Racing: speed up.
    Throttle,
    /// Racing: slow down, then reverse once stopped.
    Brake,
    /// Racing: turn anticlockwise.
    SteerLeft,
    /// Racing: turn clockwise.
    SteerRight,
    /// Shootout: walk up the screen.
    MoveUp,
    /// Shootout: walk down the screen.
    MoveDown,
    /// Shootout: walk left.
    MoveLeft,
    /// Shootout: walk right.
    MoveRight,
    /// Shootout: shoot a paint ball.
    Fire,
}

impl ControlAction {
    /// Every action, in the order the Controls screen lists them.
    pub const ALL: [ControlAction; 11] = [
        ControlAction::PaddleUp,
        ControlAction::PaddleDown,
        ControlAction::Throttle,
        ControlAction::Brake,
        ControlAction::SteerLeft,
        ControlAction::SteerRight,
        ControlAction::MoveUp,
        ControlAction::MoveDown,
        ControlAction::MoveLeft,
        ControlAction::MoveRight,
        ControlAction::Fire,
    ];

    /// The minigame it's for. One control can do one thing per minigame.
    pub fn minigame(self) -> Minigame {
        match self {
            ControlAction::PaddleUp | ControlAction::PaddleDown => Minigame::TwoPlayerPong,
            ControlAction::Throttle
            | ControlAction::Brake
            | ControlAction::SteerLeft
            | ControlAction::SteerRight => Minigame::Racing,
            ControlAction::MoveUp
            | ControlAction::MoveDown
            | ControlAction::MoveLeft
            | ControlAction::MoveRight
            | ControlAction::Fire => Minigame::Shootout,
        }
    }

    /// What the Controls screen calls it.
    pub fn name(self) -> &'static str {
        match self {
            ControlAction::PaddleUp => "Up",
            ControlAction::PaddleDown => "Down",
            ControlAction::Throttle => "Accelerate",
            ControlAction::Brake => "Brake/reverse",
            ControlAction::SteerLeft => "Steer left",
            ControlAction::SteerRight => "Steer right",
            ControlAction::MoveUp => "Move up",
            ControlAction::MoveDown => "Move down",
            ControlAction::MoveLeft => "Move left",
            ControlAction::MoveRight => "Move right",
            ControlAction::Fire => "Fire",
        }
    }
}

/// One input an action can be bound to.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Key(KeyCode),
    /// A mouse button.
    Mouse(MouseButton),
    /// A gamepad button. Triggers count from [`ACTUATION`].
    Button(GamepadButton),
    /// Pushing a gamepad stick at least [`ACTUATION`] along `axis`: towards positive values (right
    /// or up) if `positive`, otherwise the other way.
    Stick {
        axis: GamepadAxis,
        positive: bool,
    },
}

impl Control {
    /// What the Controls screen calls it, short enough for its buttons: `W`, `Up`, `Mouse left`,
    /// `Pad RT`, `LStick left`.
    pub fn name(self) -> String {
        match self {
            Control::Key(key) => key_name(key),
            Control::Mouse(button) => mouse_name(button),
            Control::Button(button) => format!("Pad {}", button_name(button)),
            Control::Stick { axis, positive } => stick_name(axis, positive),
        }
    }
}

fn mouse_name(button: MouseButton) -> String {
    let name = match button {
        MouseButton::Left => "Mouse left",
        MouseButton::Right => "Mouse right",
        MouseButton::Middle => "Mouse middle",
        MouseButton::Back => "Mouse back",
        MouseButton::Forward => "Mouse fwd",
        MouseButton::Other(number) => return format!("Mouse #{number}"),
    };
    name.into()
}

fn key_name(key: KeyCode) -> String {
    let name = match key {
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::ShiftLeft => "LShift",
        KeyCode::ShiftRight => "RShift",
        KeyCode::ControlLeft => "LCtrl",
        KeyCode::ControlRight => "RCtrl",
        KeyCode::AltLeft => "LAlt",
        KeyCode::AltRight => "RAlt",
        KeyCode::SuperLeft => "LSuper",
        KeyCode::SuperRight => "RSuper",
        KeyCode::Unidentified(_) => "Unknown key",
        _ => {
            // "KeyW" -> "W", "Digit1" -> "1", "Numpad8" -> "Num 8", "Space" stays "Space".
            let name = format!("{key:?}");
            return match (name.strip_prefix("Key"), name.strip_prefix("Digit")) {
                (Some(letter), _) if letter.len() == 1 => letter.into(),
                (_, Some(digit)) if digit.len() == 1 => digit.into(),
                _ => match name.strip_prefix("Numpad") {
                    Some(rest) => format!("Num {rest}"),
                    None => name,
                },
            };
        }
    };
    name.into()
}

/// Xbox-style names, the ones most PC players know.
fn button_name(button: GamepadButton) -> String {
    let name = match button {
        GamepadButton::South => "A",
        GamepadButton::East => "B",
        GamepadButton::North => "Y",
        GamepadButton::West => "X",
        GamepadButton::LeftTrigger => "LB",
        GamepadButton::RightTrigger => "RB",
        GamepadButton::LeftTrigger2 => "LT",
        GamepadButton::RightTrigger2 => "RT",
        GamepadButton::Select => "Back",
        GamepadButton::Start => "Start",
        GamepadButton::Mode => "Guide",
        GamepadButton::LeftThumb => "L3",
        GamepadButton::RightThumb => "R3",
        GamepadButton::DPadUp => "D-up",
        GamepadButton::DPadDown => "D-down",
        GamepadButton::DPadLeft => "D-left",
        GamepadButton::DPadRight => "D-right",
        GamepadButton::Other(number) => return format!("#{number}"),
        other => return format!("{other:?}"),
    };
    name.into()
}

fn stick_name(axis: GamepadAxis, positive: bool) -> String {
    let (stick, horizontal) = match axis {
        GamepadAxis::LeftStickX => ("LStick", true),
        GamepadAxis::LeftStickY => ("LStick", false),
        GamepadAxis::RightStickX => ("RStick", true),
        GamepadAxis::RightStickY => ("RStick", false),
        other => return format!("Pad {other:?}{}", if positive { "+" } else { "-" }),
    };
    let direction = match (horizontal, positive) {
        (true, true) => "right",
        (true, false) => "left",
        (false, true) => "up",
        (false, false) => "down",
    };
    format!("{stick} {direction}")
}

/// This player's controls for every action, up to [`SLOTS`] each. Defaults until the main menu
/// loads the saved ones (see [`Controls::load`]), so headless tests always play with the defaults.
#[derive(Resource, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
// A field missing from the saved file, such as an action added since, takes its default.
#[serde(default)]
pub struct Controls {
    paddle_up: Vec<Control>,
    paddle_down: Vec<Control>,
    throttle: Vec<Control>,
    brake: Vec<Control>,
    steer_left: Vec<Control>,
    steer_right: Vec<Control>,
    move_up: Vec<Control>,
    move_down: Vec<Control>,
    move_left: Vec<Control>,
    move_right: Vec<Control>,
    fire: Vec<Control>,
}

impl Default for Controls {
    /// WASD and the arrow keys, mouse click for firing, and a gamepad's left stick and triggers.
    fn default() -> Self {
        let stick = |axis, positive| Control::Stick { axis, positive };
        Self {
            paddle_up: vec![
                Control::Key(KeyCode::KeyW),
                Control::Key(KeyCode::ArrowUp),
                stick(GamepadAxis::LeftStickY, true),
            ],
            paddle_down: vec![
                Control::Key(KeyCode::KeyS),
                Control::Key(KeyCode::ArrowDown),
                stick(GamepadAxis::LeftStickY, false),
            ],
            throttle: vec![
                Control::Key(KeyCode::KeyW),
                Control::Key(KeyCode::ArrowUp),
                Control::Button(GamepadButton::RightTrigger2),
            ],
            brake: vec![
                Control::Key(KeyCode::KeyS),
                Control::Key(KeyCode::ArrowDown),
                Control::Button(GamepadButton::LeftTrigger2),
            ],
            steer_left: vec![
                Control::Key(KeyCode::KeyA),
                Control::Key(KeyCode::ArrowLeft),
                stick(GamepadAxis::LeftStickX, false),
            ],
            steer_right: vec![
                Control::Key(KeyCode::KeyD),
                Control::Key(KeyCode::ArrowRight),
                stick(GamepadAxis::LeftStickX, true),
            ],
            move_up: vec![
                Control::Key(KeyCode::KeyW),
                Control::Key(KeyCode::ArrowUp),
                stick(GamepadAxis::LeftStickY, true),
            ],
            move_down: vec![
                Control::Key(KeyCode::KeyS),
                Control::Key(KeyCode::ArrowDown),
                stick(GamepadAxis::LeftStickY, false),
            ],
            move_left: vec![
                Control::Key(KeyCode::KeyA),
                Control::Key(KeyCode::ArrowLeft),
                stick(GamepadAxis::LeftStickX, false),
            ],
            move_right: vec![
                Control::Key(KeyCode::KeyD),
                Control::Key(KeyCode::ArrowRight),
                stick(GamepadAxis::LeftStickX, true),
            ],
            fire: vec![
                Control::Mouse(MouseButton::Left),
                Control::Key(KeyCode::Space),
                Control::Button(GamepadButton::RightTrigger2),
            ],
        }
    }
}

impl Controls {
    /// The controls bound to `action`, in slot order.
    pub fn get(&self, action: ControlAction) -> &[Control] {
        match action {
            ControlAction::PaddleUp => &self.paddle_up,
            ControlAction::PaddleDown => &self.paddle_down,
            ControlAction::Throttle => &self.throttle,
            ControlAction::Brake => &self.brake,
            ControlAction::SteerLeft => &self.steer_left,
            ControlAction::SteerRight => &self.steer_right,
            ControlAction::MoveUp => &self.move_up,
            ControlAction::MoveDown => &self.move_down,
            ControlAction::MoveLeft => &self.move_left,
            ControlAction::MoveRight => &self.move_right,
            ControlAction::Fire => &self.fire,
        }
    }

    fn get_mut(&mut self, action: ControlAction) -> &mut Vec<Control> {
        match action {
            ControlAction::PaddleUp => &mut self.paddle_up,
            ControlAction::PaddleDown => &mut self.paddle_down,
            ControlAction::Throttle => &mut self.throttle,
            ControlAction::Brake => &mut self.brake,
            ControlAction::SteerLeft => &mut self.steer_left,
            ControlAction::SteerRight => &mut self.steer_right,
            ControlAction::MoveUp => &mut self.move_up,
            ControlAction::MoveDown => &mut self.move_down,
            ControlAction::MoveLeft => &mut self.move_left,
            ControlAction::MoveRight => &mut self.move_right,
            ControlAction::Fire => &mut self.fire,
        }
    }

    /// Bind `control` to `action`'s `slot` (from 0 to [`SLOTS`]), in place of what was there, or
    /// after the others if that slot is empty. One control does one thing per minigame, so if
    /// another of the minigame's actions had it, that one loses it: its name is returned.
    pub fn bind(
        &mut self,
        action: ControlAction,
        slot: usize,
        control: Control,
    ) -> Option<ControlAction> {
        let list = self.get_mut(action);
        let kept = if slot < list.len() {
            list[slot] = control;
            slot
        } else {
            list.push(control);
            list.len() - 1
        };
        let mut index = 0;
        list.retain(|bound| {
            let keep = *bound != control || index == kept;
            index += 1;
            keep
        });
        let mut taken_from = None;
        for other in ControlAction::ALL {
            if other != action && other.minigame() == action.minigame() {
                let list = self.get_mut(other);
                let before = list.len();
                list.retain(|bound| *bound != control);
                if list.len() != before {
                    taken_from = Some(other);
                }
            }
        }
        taken_from
    }

    /// Unbind whatever is in `action`'s `slot`, and return it. The controls after it move up.
    pub fn clear(&mut self, action: ControlAction, slot: usize) -> Option<Control> {
        let list = self.get_mut(action);
        (slot < list.len()).then(|| list.remove(slot))
    }

    /// These controls made safe to use, for ones read from a file someone may have edited: at most
    /// [`SLOTS`] per action, none doing two things in one minigame (the first keeps it), and
    /// never the [`RESERVED_KEY`].
    pub fn sanitized(mut self) -> Self {
        let mut seen: Vec<(Minigame, Control)> = Vec::new();
        for action in ControlAction::ALL {
            let game = action.minigame();
            let list = self.get_mut(action);
            list.retain(|&control| {
                let keep =
                    control != Control::Key(RESERVED_KEY) && !seen.contains(&(game, control));
                if keep {
                    seen.push((game, control));
                }
                keep
            });
            list.truncate(SLOTS);
        }
        self
    }
}

/// Reads input with [`bevy_enhanced_input`] and keeps this player's [`Controls`]. Must be added
/// before the minigames, which register their input contexts with it.
pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EnhancedInputPlugin);
        app.init_resource::<Controls>();
        app.add_systems(
            PreUpdate,
            log_gamepad_connections.run_if(on_message::<GamepadConnectionEvent>),
        );
    }
}

/// Spawn an `A` action for the input context `context` (a `C`), bound to `controls`. Each binding
/// only counts from [`ACTUATION`], so a resting trigger or stick doesn't, and a stick binding only
/// counts in its own direction.
pub fn spawn_action<C: Component, A: InputAction>(
    commands: &mut Commands,
    context: Entity,
    controls: &[Control],
) {
    let action = commands
        .spawn((ActionOf::<C>::new(context), Action::<A>::new()))
        .id();
    for &control in controls {
        let mut binding = commands.spawn((BindingOf(action), Down::new(ACTUATION)));
        match control {
            Control::Key(key) => binding.insert(Binding::from(key)),
            Control::Mouse(button) => binding.insert(Binding::from(button)),
            Control::Button(button) => binding.insert(Binding::from(button)),
            // Clamped before the threshold: the threshold looks at how far the stick is pushed
            // either way.
            Control::Stick { axis, positive } => binding.insert((
                Binding::from(axis),
                if positive { Clamp::pos() } else { Clamp::neg() },
            )),
        };
    }
}

/// Whether an action is being done this frame.
pub fn fired(state: &TriggerState) -> bool {
    *state == TriggerState::Fired
}

/// Gamepads come and go while the game runs, so say which and when.
fn log_gamepad_connections(mut connections: MessageReader<GamepadConnectionEvent>) {
    for event in connections.read() {
        match &event.connection {
            GamepadConnection::Connected { name, .. } => {
                info!("Gamepad connected: {name} ({})", event.gamepad);
            }
            GamepadConnection::Disconnected => {
                info!("Gamepad disconnected ({})", event.gamepad);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::{Control, ControlAction, Controls, RESERVED_KEY, SLOTS};

    const W: Control = Control::Key(KeyCode::KeyW);
    const UP: Control = Control::Key(KeyCode::ArrowUp);
    const K: Control = Control::Key(KeyCode::KeyK);
    const RT: Control = Control::Button(GamepadButton::RightTrigger2);

    #[test]
    fn every_action_has_keys_and_a_gamepad_control_by_default() {
        let controls = Controls::default();
        for action in ControlAction::ALL {
            let bound = controls.get(action);
            assert_eq!(bound.len(), SLOTS, "{action:?}");
            assert!(
                matches!(bound[0], Control::Key(_) | Control::Mouse(_)),
                "{action:?}"
            );
            assert!(
                matches!(bound[2], Control::Button(_) | Control::Stick { .. }),
                "{action:?}"
            );
        }
        assert_eq!(controls.clone().sanitized(), controls, "defaults clash");
    }

    #[test]
    fn binding_a_slot_replaces_what_was_there() {
        let mut controls = Controls::default();
        assert_eq!(controls.bind(ControlAction::Throttle, 2, K), None);
        assert_eq!(controls.get(ControlAction::Throttle), [W, UP, K]);
    }

    #[test]
    fn binding_an_empty_slot_adds_after_the_others() {
        let mut controls = Controls::default();
        controls.clear(ControlAction::Throttle, 0);
        controls.clear(ControlAction::Throttle, 0);
        assert_eq!(controls.get(ControlAction::Throttle), [RT]);
        controls.bind(ControlAction::Throttle, 2, K);
        assert_eq!(controls.get(ControlAction::Throttle), [RT, K]);
    }

    #[test]
    fn a_control_moved_within_an_action_leaves_its_old_slot() {
        let mut controls = Controls::default();
        controls.bind(ControlAction::Throttle, 2, W);
        assert_eq!(controls.get(ControlAction::Throttle), [UP, W]);
    }

    #[test]
    fn a_control_taken_by_another_action_of_the_minigame_leaves_the_first() {
        let mut controls = Controls::default();
        let taken_from = controls.bind(ControlAction::Brake, 0, W);
        assert_eq!(taken_from, Some(ControlAction::Throttle));
        assert_eq!(controls.get(ControlAction::Throttle), [UP, RT]);
        assert_eq!(controls.get(ControlAction::Brake)[0], W);
        // Pong keeps W: it's another minigame.
        assert_eq!(controls.get(ControlAction::PaddleUp)[0], W);
    }

    #[test]
    fn clearing_a_slot_moves_the_rest_up() {
        let mut controls = Controls::default();
        assert_eq!(controls.clear(ControlAction::Throttle, 0), Some(W));
        assert_eq!(controls.get(ControlAction::Throttle), [UP, RT]);
        assert_eq!(controls.clear(ControlAction::Throttle, 2), None);
    }

    #[test]
    fn edited_controls_are_made_safe() {
        let controls = Controls {
            throttle: vec![W, Control::Key(RESERVED_KEY), UP, RT, K],
            brake: vec![W, Control::Key(KeyCode::KeyS)],
            ..Controls::default()
        };
        let controls = controls.sanitized();
        assert_eq!(controls.get(ControlAction::Throttle), [W, UP, RT]);
        assert_eq!(
            controls.get(ControlAction::Brake),
            [Control::Key(KeyCode::KeyS)]
        );
        assert_eq!(controls.get(ControlAction::PaddleUp)[0], W);
    }

    #[test]
    fn controls_survive_a_trip_through_the_settings_file() {
        let mut controls = Controls::default();
        controls.bind(ControlAction::SteerLeft, 1, RT);
        let text = toml::to_string_pretty(&controls).unwrap();
        assert_eq!(toml::from_str::<Controls>(&text).unwrap(), controls);
    }

    #[test]
    fn actions_missing_from_the_file_get_their_defaults() {
        let controls: Controls = toml::from_str("throttle = [{ Key = \"KeyK\" }]").unwrap();
        assert_eq!(controls.get(ControlAction::Throttle), [K]);
        assert_eq!(
            controls.get(ControlAction::Brake),
            Controls::default().get(ControlAction::Brake)
        );
    }

    #[test]
    fn controls_have_short_readable_names() {
        let names: Vec<String> = [
            W,
            UP,
            Control::Key(KeyCode::Digit1),
            Control::Key(KeyCode::Numpad8),
            Control::Key(KeyCode::Space),
            Control::Key(KeyCode::ShiftLeft),
            Control::Mouse(MouseButton::Left),
            Control::Mouse(MouseButton::Right),
            Control::Mouse(MouseButton::Middle),
            RT,
            Control::Button(GamepadButton::DPadRight),
            Control::Stick {
                axis: GamepadAxis::LeftStickX,
                positive: false,
            },
            Control::Stick {
                axis: GamepadAxis::RightStickY,
                positive: true,
            },
        ]
        .into_iter()
        .map(Control::name)
        .collect();
        assert_eq!(
            names,
            [
                "W",
                "Up",
                "1",
                "Num 8",
                "Space",
                "LShift",
                "Mouse left",
                "Mouse right",
                "Mouse middle",
                "Pad RT",
                "Pad D-right",
                "LStick left",
                "RStick up"
            ]
        );
        for name in names {
            assert!(name.len() <= 12, "{name} won't fit on its button");
        }
    }
}
