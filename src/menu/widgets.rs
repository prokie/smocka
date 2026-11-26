//! Building blocks shared by every menu screen: colours, full-screen layouts, labels, buttons and
//! text fields, and how buttons look as they're hovered, tabbed to and pressed.

use bevy::input_focus::tab_navigation::{TabGroup, TabIndex};
use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{Button, TextInput};

use super::MenuButton;
use crate::players::player_color;

pub const BUTTON_COLOR: Color = Color::srgb(0.15, 0.15, 0.18);
pub const BUTTON_HOVERED_COLOR: Color = Color::srgb(0.25, 0.25, 0.3);
pub const BUTTON_PRESSED_COLOR: Color = Color::srgb(0.35, 0.55, 0.85);
pub const BUTTON_DISABLED_COLOR: Color = Color::srgb(0.1, 0.1, 0.1);
pub const ERROR_COLOR: Color = Color::srgb(1.0, 0.45, 0.45);
pub const HINT_COLOR: Color = Color::srgb(0.7, 0.7, 0.75);

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
    button(action, text, 200.0)
}

/// A square [`menu_button`] for a single character, such as "+".
pub fn small_button(action: MenuButton, text: &str) -> impl Bundle {
    button(action, text, 48.0)
}

fn button(action: MenuButton, text: &str, width: f32) -> impl Bundle {
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
        children![label(text, 24.0, Color::WHITE)],
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

type ButtonLook = (
    Entity,
    &'static Hovered,
    Has<Pressed>,
    Has<InteractionDisabled>,
    &'static mut BackgroundColor,
);

/// Buttons light up when hovered, or when reached with `Tab` (focus from the keyboard is
/// "visible"; focus from a click isn't, so a clicked button doesn't stay lit).
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
