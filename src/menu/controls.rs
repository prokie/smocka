//! The Controls screen, shown over the main menu: what each action in each minigame is bound to.
//! Clicking a binding waits for the next key, gamepad button or stick push, and binds it there.

use bevy::input_focus::tab_navigation::TabGroup;
use bevy::input_focus::{AutoFocus, FocusCause, InputFocus};
use bevy::prelude::*;

use super::MenuButton;
use super::widgets::{HINT_COLOR, control_button, label, menu_button};
use crate::app_state::AppState;
use crate::controls::{ACTUATION, Control, ControlAction, Controls, RESERVED_KEY, SLOTS};
use crate::minigames::Minigame;

/// The line under the bindings, saying how to change them.
const HINT: &str = "Click a binding, then press a key, mouse or gamepad button, or push a stick.\n\
                    Backspace clears it. Esc cancels.";
/// How wide the column of action names is, in pixels.
const ACTION_COLUMN_WIDTH: f32 = 172.0;
/// Shown on a binding while it waits for input.
const WAITING_LABEL: &str = "Press...";
/// Shown on an empty binding.
const EMPTY_LABEL: &str = "-";

/// Sticks a binding can capture. A trigger reported as an axis (`LeftZ`, `RightZ`) is left out: on
/// some gamepads it rests at -1, which would bind it straight away.
const STICK_AXES: [GamepadAxis; 4] = [
    GamepadAxis::LeftStickX,
    GamepadAxis::LeftStickY,
    GamepadAxis::RightStickX,
    GamepadAxis::RightStickY,
];

/// The Controls screen.
#[derive(Component)]
pub struct ControlsScreen;

/// Set while one of the screen's bindings waits for the player to press something.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rebinding {
    pub action: ControlAction,
    pub slot: usize,
    /// The binding's button, which gets keyboard focus back when it's done.
    button: Entity,
    /// Whether the frame it started on is over. That frame's press (`Enter` on a focused binding,
    /// say) started it, so it mustn't be bound.
    armed: bool,
}

/// What the player did while a binding waited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Captured {
    /// Pressed the [`RESERVED_KEY`]: leave the binding as it was.
    Cancel,
    /// Pressed `Backspace` or `Delete`: empty the binding.
    Clear,
    /// Pressed or pushed this: bind it.
    Bind(Control),
}

/// Command: show the Controls screen over the main menu, unless it's already there.
#[derive(Debug, Clone, Copy)]
pub struct OpenControls;

impl Command for OpenControls {
    type Out = ();

    fn apply(self, world: &mut World) {
        let open = world
            .query_filtered::<(), With<ControlsScreen>>()
            .iter(world)
            .next()
            .is_some();
        if open {
            return;
        }
        debug!("Opening the controls screen");
        // Hides the main menu behind it.
        let background = world
            .get_resource::<ClearColor>()
            .cloned()
            .unwrap_or_default()
            .0;
        world
            .spawn((
                ControlsScreen,
                DespawnOnExit(AppState::MainMenu),
                // `Tab` stays on this screen.
                TabGroup::modal(),
                GlobalZIndex(1),
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: px(8),
                    ..default()
                },
                BackgroundColor(background),
            ))
            .with_children(|screen| {
                screen.spawn(label("Controls", 48.0, Color::WHITE));
                for game in Minigame::ALL {
                    screen.spawn(label(game.info().name, 24.0, HINT_COLOR));
                    for action in ControlAction::ALL {
                        if action.minigame() != game {
                            continue;
                        }
                        screen
                            .spawn(Node {
                                column_gap: px(8),
                                align_items: AlignItems::Center,
                                ..default()
                            })
                            .with_children(|row| {
                                row.spawn((
                                    label(action.name(), 20.0, Color::WHITE),
                                    Node {
                                        width: px(ACTION_COLUMN_WIDTH),
                                        ..default()
                                    },
                                ));
                                for slot in 0..SLOTS {
                                    let mut button =
                                        row.spawn(control_button(MenuButton::Rebind(action, slot)));
                                    if action == ControlAction::ALL[0] && slot == 0 {
                                        button.insert(AutoFocus);
                                    }
                                }
                            });
                    }
                }
                screen.spawn((
                    label(HINT, 16.0, HINT_COLOR),
                    TextLayout::justify(Justify::Center),
                ));
                screen
                    .spawn(Node {
                        column_gap: px(12),
                        ..default()
                    })
                    .with_children(|buttons| {
                        buttons.spawn(menu_button(MenuButton::ResetControls, "Defaults"));
                        buttons.spawn(menu_button(MenuButton::CloseControls, "Back"));
                    });
            });
    }
}

/// Command: close the Controls screen, and give keyboard focus back to the main menu's Controls
/// button.
#[derive(Debug, Clone, Copy)]
pub struct CloseControls;

impl Command for CloseControls {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.remove_resource::<Rebinding>();
        let screens: Vec<Entity> = world
            .query_filtered::<Entity, With<ControlsScreen>>()
            .iter(world)
            .collect();
        if screens.is_empty() {
            return;
        }
        for screen in screens {
            world.despawn(screen);
        }
        debug!("Closed the controls screen");
        let button = world
            .query::<(Entity, &MenuButton)>()
            .iter(world)
            .find(|(_, button)| **button == MenuButton::Controls)
            .map(|(entity, _)| entity);
        if let (Some(button), Some(mut focus)) = (button, world.get_resource_mut::<InputFocus>()) {
            focus.set(button, FocusCause::Navigated);
        }
    }
}

/// Command: put every control back to its default.
#[derive(Debug, Clone, Copy)]
pub struct ResetControls;

impl Command for ResetControls {
    type Out = ();

    fn apply(self, world: &mut World) {
        world.remove_resource::<Rebinding>();
        if *world.resource::<Controls>() == Controls::default() {
            debug!("Not resetting the controls: they're already the defaults");
            return;
        }
        *world.resource_mut::<Controls>() = Controls::default();
        info!("Controls reset to the defaults");
    }
}

/// Command: make `action`'s `slot` wait for the next key, gamepad button or stick push. Takes
/// keyboard focus off its `button`, or `Enter` and `Space` would press it again instead.
#[derive(Debug, Clone, Copy)]
pub struct StartRebinding {
    pub action: ControlAction,
    pub slot: usize,
    pub button: Entity,
}

impl Command for StartRebinding {
    type Out = ();

    fn apply(self, world: &mut World) {
        debug!("{} waits for a control", describe(self.action, self.slot));
        world.insert_resource(Rebinding {
            action: self.action,
            slot: self.slot,
            button: self.button,
            armed: false,
        });
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            focus.clear();
        }
    }
}

/// "Racing: Accelerate, binding 2", for the log.
fn describe(action: ControlAction, slot: usize) -> String {
    format!(
        "{}: {}, binding {}",
        action.minigame().info().name,
        action.name(),
        slot + 1
    )
}

/// What the player pressed this frame, if anything a binding takes: a key (`Esc` cancels,
/// `Backspace` or `Delete` clears), a released mouse button, a gamepad button, or a stick pushed
/// at least [`ACTUATION`].
///
/// Mouse buttons are captured on release (while [`on_menu_button`](super::on_menu_button) ignores
/// clicks during [`Rebinding`]), so binding `Mouse left` while the cursor is over a button never
/// triggers that button on release.
pub fn captured_input<'a>(
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    gamepads: impl IntoIterator<Item = &'a Gamepad>,
) -> Option<Captured> {
    if keys.just_pressed(RESERVED_KEY) {
        return Some(Captured::Cancel);
    }
    if keys.any_just_pressed([KeyCode::Backspace, KeyCode::Delete]) {
        return Some(Captured::Clear);
    }
    if let Some(&key) = keys.get_just_pressed().next() {
        return Some(Captured::Bind(Control::Key(key)));
    }
    if let Some(&button) = mouse.get_just_released().next() {
        return Some(Captured::Bind(Control::Mouse(button)));
    }
    for gamepad in gamepads {
        if let Some(&button) = gamepad.get_just_pressed().next() {
            return Some(Captured::Bind(Control::Button(button)));
        }
        for axis in STICK_AXES {
            let value = gamepad.get(axis).unwrap_or_default();
            if value.abs() >= ACTUATION {
                return Some(Captured::Bind(Control::Stick {
                    axis,
                    positive: value > 0.0,
                }));
            }
        }
    }
    None
}

/// While a binding waits: bind whatever the player presses there (see [`captured_input`]), then
/// give its button keyboard focus back. Saving is left to the menu (see
/// [`save_controls`](super::save_controls)).
pub(super) fn capture_control(
    mut rebinding: ResMut<Rebinding>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    mut controls: ResMut<Controls>,
    mut focus: ResMut<InputFocus>,
    mut commands: Commands,
) {
    if !rebinding.armed {
        rebinding.armed = true;
        return;
    }
    let Some(captured) = captured_input(&keys, &mouse, gamepads) else {
        return;
    };
    let Rebinding {
        action,
        slot,
        button,
        ..
    } = *rebinding;
    let what = describe(action, slot);
    let old = controls.get(action).get(slot).copied();
    match captured {
        Captured::Cancel => debug!("{what} stays {}", slot_label(&controls, action, slot)),
        Captured::Clear => match controls.clear(action, slot) {
            Some(old) => info!("{what}: {} -> nothing", old.name()),
            None => debug!("{what} stays empty"),
        },
        Captured::Bind(control) if old == Some(control) => {
            debug!("{what} stays {}", control.name());
        }
        Captured::Bind(control) => {
            let taken_from = controls.bind(action, slot, control);
            info!(
                "{what}: {} -> {}{}",
                old.map_or_else(|| "nothing".into(), Control::name),
                control.name(),
                taken_from.map_or_else(String::new, |other| format!(
                    " (taken from {})",
                    other.name()
                ))
            );
        }
    }
    commands.remove_resource::<Rebinding>();
    focus.set(button, FocusCause::Navigated);
}

/// What `action`'s `slot` shows when it isn't waiting.
pub fn slot_label(controls: &Controls, action: ControlAction, slot: usize) -> String {
    controls
        .get(action)
        .get(slot)
        .map_or_else(|| EMPTY_LABEL.into(), |control| control.name())
}

/// Show each binding's control, or that it waits for one.
pub(super) fn update_control_labels(
    controls: Res<Controls>,
    rebinding: Option<Res<Rebinding>>,
    buttons: Query<(&MenuButton, &Children)>,
    mut texts: Query<&mut Text>,
) {
    for (button, children) in &buttons {
        let MenuButton::Rebind(action, slot) = *button else {
            continue;
        };
        let waiting = rebinding
            .as_ref()
            .is_some_and(|rebinding| rebinding.action == action && rebinding.slot == slot);
        let shown = if waiting {
            WAITING_LABEL.into()
        } else {
            slot_label(&controls, action, slot)
        };
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child)
                && text.0 != shown
            {
                text.0.clone_from(&shown);
            }
        }
    }
}

/// `Esc` closes the screen, unless a binding is waiting: then it only cancels that.
pub(super) fn close_controls_on_escape(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(RESERVED_KEY) {
        commands.queue(CloseControls);
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input_focus::InputFocus;
    use bevy::prelude::*;

    use super::{
        Captured, CloseControls, ControlsScreen, OpenControls, Rebinding, ResetControls,
        StartRebinding, capture_control, captured_input, slot_label, update_control_labels,
    };
    use crate::controls::{Control, ControlAction, Controls, SLOTS};
    use crate::menu::MenuButton;

    fn captured(
        keys: &[KeyCode],
        mouse_released: &[MouseButton],
        gamepad: Option<Gamepad>,
    ) -> Option<Captured> {
        let mut input = ButtonInput::default();
        for &key in keys {
            input.press(key);
        }
        let mut mouse = ButtonInput::default();
        for &button in mouse_released {
            mouse.press(button);
            mouse.release(button);
        }
        captured_input(&input, &mouse, gamepad.as_ref())
    }

    #[test]
    fn a_key_press_is_captured() {
        assert_eq!(
            captured(&[KeyCode::KeyK], &[], None),
            Some(Captured::Bind(Control::Key(KeyCode::KeyK)))
        );
        assert_eq!(captured(&[], &[], None), None);
    }

    #[test]
    fn a_mouse_button_is_captured_on_release() {
        assert_eq!(
            captured(&[], &[MouseButton::Left], None),
            Some(Captured::Bind(Control::Mouse(MouseButton::Left)))
        );
        // Pressing without releasing yet waits for release.
        let keys = ButtonInput::default();
        let mut mouse = ButtonInput::default();
        mouse.press(MouseButton::Left);
        assert_eq!(captured_input(&keys, &mouse, None), None);
    }

    #[test]
    fn escape_cancels_and_backspace_or_delete_clears() {
        assert_eq!(
            captured(&[KeyCode::Escape], &[], None),
            Some(Captured::Cancel)
        );
        assert_eq!(
            captured(&[KeyCode::Backspace], &[], None),
            Some(Captured::Clear)
        );
        assert_eq!(
            captured(&[KeyCode::Delete], &[], None),
            Some(Captured::Clear)
        );
    }

    #[test]
    fn a_gamepad_button_is_captured() {
        let mut gamepad = Gamepad::default();
        gamepad.digital_mut().press(GamepadButton::South);
        assert_eq!(
            captured(&[], &[], Some(gamepad)),
            Some(Captured::Bind(Control::Button(GamepadButton::South)))
        );
    }

    #[test]
    fn a_stick_pushed_far_enough_is_captured_with_its_direction() {
        let pushed = |axis, value| {
            let mut gamepad = Gamepad::default();
            gamepad.analog_mut().set(axis, value);
            captured(&[], &[], Some(gamepad))
        };
        assert_eq!(
            pushed(GamepadAxis::LeftStickX, -0.9),
            Some(Captured::Bind(Control::Stick {
                axis: GamepadAxis::LeftStickX,
                positive: false
            }))
        );
        assert_eq!(pushed(GamepadAxis::RightStickY, 0.2), None);
        // A trigger resting as an axis is ignored.
        assert_eq!(pushed(GamepadAxis::LeftZ, -1.0), None);
    }

    /// A world waiting for a control for racing's Accelerate, first binding.
    fn waiting_for_throttle() -> (World, Entity) {
        let mut world = World::new();
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<ButtonInput<MouseButton>>();
        world.init_resource::<Controls>();
        world.init_resource::<InputFocus>();
        let button = world.spawn_empty().id();
        world.commands().queue(StartRebinding {
            action: ControlAction::Throttle,
            slot: 0,
            button,
        });
        world.flush();
        (world, button)
    }

    fn press(world: &mut World, key: KeyCode) {
        let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.press(key);
    }

    #[test]
    fn a_waiting_binding_takes_the_next_press_but_not_the_one_that_started_it() {
        let (mut world, button) = waiting_for_throttle();
        // Enter pressed the binding's button this frame.
        press(&mut world, KeyCode::Enter);
        world.run_system_once(capture_control).unwrap();
        assert!(world.contains_resource::<Rebinding>());
        assert_eq!(
            world.resource::<Controls>().get(ControlAction::Throttle)[0],
            Control::Key(KeyCode::KeyW)
        );

        press(&mut world, KeyCode::KeyK);
        world.run_system_once(capture_control).unwrap();
        assert!(!world.contains_resource::<Rebinding>());
        let controls = world.resource::<Controls>();
        assert_eq!(slot_label(controls, ControlAction::Throttle, 0), "K");
        assert_eq!(world.resource::<InputFocus>().get(), Some(button));
    }

    #[test]
    fn escape_leaves_the_binding_as_it_was() {
        let (mut world, _) = waiting_for_throttle();
        world.run_system_once(capture_control).unwrap();
        press(&mut world, KeyCode::Escape);
        world.run_system_once(capture_control).unwrap();
        assert!(!world.contains_resource::<Rebinding>());
        assert_eq!(*world.resource::<Controls>(), Controls::default());
    }

    #[test]
    fn backspace_empties_the_binding() {
        let (mut world, _) = waiting_for_throttle();
        world.run_system_once(capture_control).unwrap();
        press(&mut world, KeyCode::Backspace);
        world.run_system_once(capture_control).unwrap();
        let controls = world.resource::<Controls>();
        assert_eq!(slot_label(controls, ControlAction::Throttle, 0), "Up");
        assert_eq!(slot_label(controls, ControlAction::Throttle, 2), "-");
    }

    #[test]
    fn defaults_undo_every_change() {
        let mut world = World::new();
        let mut controls = Controls::default();
        controls.bind(ControlAction::Brake, 1, Control::Key(KeyCode::Space));
        world.insert_resource(controls);
        world.commands().queue(ResetControls);
        world.flush();
        assert_eq!(*world.resource::<Controls>(), Controls::default());
    }

    /// The text on `action`'s `slot` binding.
    fn shown(world: &mut World, action: ControlAction, slot: usize) -> String {
        let mut buttons = world.query::<(&MenuButton, &Children)>();
        let child = buttons
            .iter(world)
            .find(|(button, _)| **button == MenuButton::Rebind(action, slot))
            .map(|(_, children)| children[0])
            .unwrap();
        world.get::<Text>(child).unwrap().0.clone()
    }

    #[test]
    fn the_screen_shows_every_binding_of_every_action() {
        let mut world = World::new();
        world.init_resource::<Controls>();
        world.commands().queue(OpenControls);
        // Opening it again doesn't stack another.
        world.commands().queue(OpenControls);
        world.flush();
        let screens = world
            .query_filtered::<(), With<ControlsScreen>>()
            .iter(&world)
            .count();
        assert_eq!(screens, 1);
        let bindings = world
            .query::<&MenuButton>()
            .iter(&world)
            .filter(|button| matches!(button, MenuButton::Rebind(..)))
            .count();
        assert_eq!(bindings, ControlAction::ALL.len() * SLOTS);

        world.run_system_once(update_control_labels).unwrap();
        assert_eq!(shown(&mut world, ControlAction::Throttle, 2), "Pad RT");
        assert_eq!(shown(&mut world, ControlAction::SteerLeft, 0), "A");

        world.insert_resource(Rebinding {
            action: ControlAction::Throttle,
            slot: 2,
            button: Entity::PLACEHOLDER,
            armed: true,
        });
        world.run_system_once(update_control_labels).unwrap();
        assert_eq!(shown(&mut world, ControlAction::Throttle, 2), "Press...");
    }

    #[test]
    fn closing_the_screen_gives_focus_back_to_the_controls_button() {
        let mut world = World::new();
        world.init_resource::<Controls>();
        world.init_resource::<InputFocus>();
        let controls_button = world.spawn(MenuButton::Controls).id();
        world.commands().queue(OpenControls);
        world.flush();

        world.commands().queue(CloseControls);
        world.flush();
        let screens = world
            .query_filtered::<(), With<ControlsScreen>>()
            .iter(&world)
            .count();
        assert_eq!(screens, 0);
        assert_eq!(world.resource::<InputFocus>().get(), Some(controls_button));
    }
}
