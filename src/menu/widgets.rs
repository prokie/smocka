//! Building blocks shared by every menu screen: colours, full-screen layouts, labels, buttons and
//! text fields, and how buttons look as they're hovered, tabbed to and pressed.

use std::time::Duration;

use bevy::input_focus::tab_navigation::{TabGroup, TabIndex};
use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{Button, ScrollIntoView, TextInput};

use super::MenuButton;
use crate::players::player_color;

pub const BUTTON_COLOR: Color = Color::srgb(0.15, 0.15, 0.18);
pub const BUTTON_HOVERED_COLOR: Color = Color::srgb(0.25, 0.25, 0.3);
pub const BUTTON_PRESSED_COLOR: Color = Color::srgb(0.35, 0.55, 0.85);
pub const BUTTON_DISABLED_COLOR: Color = Color::srgb(0.1, 0.1, 0.1);
pub const ERROR_COLOR: Color = Color::srgb(1.0, 0.45, 0.45);
pub const HINT_COLOR: Color = Color::srgb(0.7, 0.7, 0.75);

/// An [`icon_button`]'s size, and its icon's, in pixels.
const ICON_BUTTON_SIZE: f32 = 32.0;
const ICON_SIZE: f32 = 22.0;

/// Size of the colour square next to each player's name.
const ROW_SWATCH_SIZE: f32 = 18.0;

/// A full-window column, centred, that is despawned when `state` is left.
pub fn screen<S: States>(state: S) -> impl Bundle {
    (
        DespawnOnExit(state),
        // `Tab` only moves between focusable entities inside a tab group.
        TabGroup::new(0),
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(16),
            ..default()
        },
    )
}

/// A line of text `size` pixels high.
pub fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

/// A button reading `text` that does `action` when activated.
pub fn menu_button(action: MenuButton, text: &str) -> impl Bundle {
    button(action, text, 200.0, 24.0)
}

/// A square [`menu_button`] for a single character, such as "+".
pub fn small_button(action: MenuButton, text: &str) -> impl Bundle {
    button(action, text, 48.0, 24.0)
}

/// A small square button showing `icon` (a white image, on the button's colour), that does
/// `action`. Give it a `Node` of its own (from [`icon_button_node`]) to place it.
pub fn icon_button(action: MenuButton, icon: Handle<Image>) -> impl Bundle {
    (
        action,
        Button,
        Hovered::default(),
        TabIndex(0),
        BackgroundColor(BUTTON_COLOR),
        children![(
            ImageNode::new(icon),
            Node {
                width: px(ICON_SIZE),
                height: px(ICON_SIZE),
                ..default()
            },
        )],
    )
}

/// The size and shape of an [`icon_button`].
pub fn icon_button_node() -> Node {
    Node {
        width: px(ICON_BUTTON_SIZE),
        height: px(ICON_BUTTON_SIZE),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border_radius: BorderRadius::all(px(6)),
        ..default()
    }
}

/// A narrower [`menu_button`] with smaller text, for one binding on the Controls screen. It starts
/// empty: the screen fills its text in.
pub fn control_button(action: MenuButton) -> impl Bundle {
    button(action, "", 140.0, 18.0)
}

fn button(action: MenuButton, text: &str, width: f32, text_size: f32) -> impl Bundle {
    (
        action,
        Button,
        Hovered::default(),
        // Reachable with `Tab`; `Enter` presses the focused button.
        TabIndex(0),
        Node {
            width: px(width),
            height: px(48),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(BUTTON_COLOR),
        children![label(text, text_size, Color::WHITE)],
    )
}

/// A one-line text field holding `text`, at most `max_characters` long, about `visible_width`
/// characters wide.
pub fn text_field(text: String, max_characters: usize, visible_width: f32) -> impl Bundle {
    (
        TextInput,
        // Clicking only gives keyboard focus to entities with a `TabIndex` (Bevy's
        // `PointerFocusPlugin`). Without it, a field can only be focused by `AutoFocus`.
        TabIndex(0),
        EditableText {
            max_characters: Some(max_characters),
            visible_width: Some(visible_width),
            ..EditableText::new(text)
        },
        TextFont {
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextLayout::no_wrap(),
        TextCursorStyle::default(),
        Node {
            padding: UiRect::axes(px(10), px(10)),
            border: px(2).all(),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BorderColor::all(HINT_COLOR),
        BackgroundColor(BUTTON_COLOR),
    )
}

/// A player's row: a square of their colour, then their name.
pub fn player_row(text: &str, color: u8) -> impl Bundle {
    (
        Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        },
        children![
            (
                Node {
                    width: px(ROW_SWATCH_SIZE),
                    height: px(ROW_SWATCH_SIZE),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(player_color(color)),
            ),
            label(text.to_string(), 24.0, Color::WHITE),
        ],
    )
}

/// What a list of [`player_row`]s shows, and which list entity it was built in. Keep one in a
/// `Local` per list, and pass it to [`show_rows`].
#[derive(Default)]
pub struct ShownRows(Option<(Entity, Vec<(String, u8)>)>);

/// Rebuild `list` with one [`player_row`] per `(text, colour)` in `rows`, unless it already shows
/// them. A screen spawned again gets a new list, which is always filled.
pub fn show_rows(
    list: Entity,
    rows: Vec<(String, u8)>,
    shown: &mut ShownRows,
    commands: &mut Commands,
) {
    if shown
        .0
        .as_ref()
        .is_some_and(|(shown_list, shown_rows)| *shown_list == list && *shown_rows == rows)
    {
        return;
    }
    commands.entity(list).despawn_children();
    for (text, color) in &rows {
        commands.entity(list).with_child(player_row(text, *color));
    }
    shown.0 = Some((list, rows));
}

/// A line counting down the seconds until the screen moves on: "{prefix} {seconds}…". The host
/// decides when that happens; this is only this app's estimate, started when the screen appeared.
#[derive(Component)]
pub struct Countdown {
    timer: Timer,
    prefix: &'static str,
}

impl Countdown {
    /// Count down from the start again, for when the host restarts its timer.
    pub fn restart(&mut self) {
        self.timer.reset();
    }

    /// How long is left.
    #[cfg(test)]
    pub fn remaining(&self) -> Duration {
        self.timer.remaining()
    }

    pub(super) fn tick(&mut self, delta: Duration) {
        self.timer.tick(delta);
    }
}

/// A [`Countdown`] from `duration`.
pub fn countdown(duration: Duration, prefix: &'static str) -> impl Bundle {
    (
        Countdown {
            timer: Timer::new(duration, TimerMode::Once),
            prefix,
        },
        label("", 20.0, HINT_COLOR),
    )
}

pub(super) fn update_countdowns(
    time: Res<Time>,
    mut countdowns: Query<(&mut Countdown, &mut Text)>,
) {
    for (mut countdown, mut text) in &mut countdowns {
        countdown.tick(time.delta());
        let shown = format!(
            "{} {}...",
            countdown.prefix,
            countdown.timer.remaining_secs().ceil().max(1.0)
        );
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

type ButtonLook = (
    Entity,
    &'static Hovered,
    Has<Pressed>,
    Has<InteractionDisabled>,
    &'static mut BackgroundColor,
);

/// Buttons light up when hovered, or when reached with `Tab` (focus from the keyboard is
/// "visible"; focus from a click isn't, so a clicked button doesn't stay lit).
/// When `Tab` or a gamepad moves keyboard focus to something scrolled out of sight in a
/// [`ScrollArea`](bevy::ui_widgets::ScrollArea), such as a minigame tile in the lobby, scroll it
/// into view. Bevy doesn't do this by itself. Clicks are left alone: what's clicked is in view.
pub(super) fn scroll_focus_into_view(
    focus: Res<InputFocus>,
    focus_visible: Res<InputFocusVisible>,
    nodes: Query<(), With<Node>>,
    mut commands: Commands,
) {
    if !focus.is_changed() || !focus_visible.0 {
        return;
    }
    if let Some(entity) = focus.get().filter(|entity| nodes.contains(*entity)) {
        commands.trigger(ScrollIntoView { entity });
    }
}

pub(super) fn style_buttons(
    mut buttons: Query<ButtonLook, With<MenuButton>>,
    focus: Res<InputFocus>,
    focus_visible: Res<InputFocusVisible>,
) {
    for (entity, hovered, pressed, disabled, mut background) in &mut buttons {
        let tabbed_to = focus_visible.0 && focus.get() == Some(entity);
        let color = match (disabled, pressed, hovered.get() || tabbed_to) {
            (true, _, _) => BUTTON_DISABLED_COLOR,
            (false, true, _) => BUTTON_PRESSED_COLOR,
            (false, false, true) => BUTTON_HOVERED_COLOR,
            (false, false, false) => BUTTON_COLOR,
        };
        background.set_if_neq(BackgroundColor(color));
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::{ShownRows, show_rows};

    fn fill(world: &mut World, list: Entity, rows: &[&str], shown: &mut ShownRows) {
        let rows = rows.iter().map(|text| (text.to_string(), 0)).collect();
        show_rows(list, rows, shown, &mut world.commands());
        world.flush();
    }

    fn row_count(world: &World, list: Entity) -> usize {
        world
            .get::<Children>(list)
            .map_or(0, |children| children.len())
    }

    #[test]
    fn a_new_list_gets_the_same_rows_again() {
        let mut world = World::new();
        let mut shown = ShownRows::default();
        let first = world.spawn_empty().id();
        fill(&mut world, first, &["Ada", "Bo"], &mut shown);
        assert_eq!(row_count(&world, first), 2);

        // The screen goes away and comes back with the same players.
        world.despawn(first);
        let second = world.spawn_empty().id();
        fill(&mut world, second, &["Ada", "Bo"], &mut shown);
        assert_eq!(row_count(&world, second), 2);
    }
}
