//! Building blocks shared by every menu screen: colours, full-screen layouts, labels, buttons and
//! text fields, and how buttons look as they're hovered, tabbed to and pressed.

use std::time::Duration;

use bevy::input_focus::tab_navigation::{TabGroup, TabIndex};
use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{
    Button, ControlOrientation, ScrollArea, ScrollIntoView, Scrollbar, ScrollbarDragState,
    ScrollbarThumb, TextInput,
};
use lightyear::prelude::*;

use super::MenuButton;
use crate::players::{Player, local_peer_id, player_color};
use crate::session::{SkipVotes, VoteSkip};

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

/// How wide a [`scroll_column`]'s scrollbar is, and the room kept clear for it either side of the
/// content (so the content stays centred), in pixels.
const SCROLLBAR_WIDTH: f32 = 6.0;
const SCROLLBAR_ROOM: f32 = 20.0;
/// The shortest a scrollbar's thumb gets, in pixels, so it's always easy to grab.
const SCROLLBAR_MIN_THUMB: f32 = 32.0;
const SCROLLBAR_COLOR: Color = Color::srgba(0.7, 0.7, 0.75, 0.45);
const SCROLLBAR_ACTIVE_COLOR: Color = Color::srgb(0.85, 0.85, 0.9);

/// A [`scroll_column`]'s scrollbar, shown only while the column has more than fits.
#[derive(Component, Debug)]
pub struct ColumnScrollbar;

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

/// Shows how many players have voted to skip a waiting screen, and which key/button votes.
#[derive(Component)]
pub struct SkipHint;

/// A [`SkipHint`] line placed under a waiting screen's [`countdown`].
pub fn skip_hint() -> impl Bundle {
    (SkipHint, label("", 16.0, HINT_COLOR))
}

/// What a [`SkipHint`] shows: whether this player already voted, and how many of `total` players
/// have agreed so far.
pub fn skip_hint_label(voted: bool, agreed: usize, total: usize) -> String {
    if voted {
        format!("Ready to skip ({agreed}/{total})")
    } else if agreed > 0 {
        format!("Space / A to skip ({agreed}/{total})")
    } else {
        "Space / A to skip".into()
    }
}

/// Pressing `Space`, `Enter`, or gamepad `A` (`South`) on a waiting screen votes to skip it.
pub(super) fn vote_skip_on_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    votes: Option<Single<&SkipVotes>>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut commands: Commands,
) {
    let pressed = keys.any_just_pressed([KeyCode::Space, KeyCode::Enter])
        || gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::South));
    if !pressed {
        return;
    }
    let already_voted =
        votes.is_some_and(|votes| local_peer_id(&clients).is_some_and(|you| votes.contains(you)));
    if !already_voted {
        commands.queue(VoteSkip);
    }
}

/// Keep every [`SkipHint`] in sync with the host's [`SkipVotes`].
pub(super) fn update_skip_hints(
    votes: Option<Single<&SkipVotes>>,
    players: Query<&Player>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut hints: Query<(&mut Text, &mut TextColor), With<SkipHint>>,
) {
    let total = players.iter().len();
    let you = local_peer_id(&clients);
    let (voted, agreed) = match votes {
        Some(votes) => (
            you.is_some_and(|you| votes.contains(you)),
            votes
                .voters
                .iter()
                .filter(|id| players.iter().any(|player| player.id == **id))
                .count(),
        ),
        None => (false, 0),
    };
    let shown = skip_hint_label(voted, agreed, total);
    let color = if voted { Color::WHITE } else { HINT_COLOR };
    for (mut text, mut text_color) in &mut hints {
        if text.0 != shown {
            text.0.clone_from(&shown);
        }
        text_color.set_if_neq(TextColor(color));
    }
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
/// A centred column holding what `content` spawns, that scrolls up and down once it's taller than
/// the room it gets. Put it in a column between things that must stay on screen (a title, a Back
/// button) and give those `flex_shrink: 0`: it shrinks to the room they leave, and they don't
/// scroll. What `content` spawns never shrinks: it scrolls instead.
///
/// It scrolls with the mouse wheel, by dragging the thin scrollbar on its right (shown only while
/// something is cut off, see [`show_scrollbars_when_needed`]), and to whatever `Tab` or a gamepad
/// focuses in it (see [`scroll_focus_into_view`]).
pub fn scroll_column(
    row_gap: f32,
    content: impl FnOnce(&mut ChildSpawner) + Send + Sync + 'static,
) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            // Lets it shrink below its content's height when the window is short.
            min_height: px(0),
            ..default()
        },
        Children::spawn(SpawnWith(move |frame: &mut ChildSpawner| {
            let area = frame
                .spawn((
                    ScrollArea,
                    Node {
                        flex_direction: FlexDirection::Column,
                        min_height: px(0),
                        padding: UiRect::horizontal(px(SCROLLBAR_WIDTH + SCROLLBAR_ROOM)),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                ))
                .with_children(|area| {
                    // Keeps its whole height, so what doesn't fit overflows to be scrolled to.
                    area.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(row_gap),
                        flex_shrink: 0.0,
                        ..default()
                    })
                    .with_children(content);
                })
                .id();
            frame.spawn((
                ColumnScrollbar,
                Scrollbar::new(area, ControlOrientation::Vertical, SCROLLBAR_MIN_THUMB),
                Node {
                    position_type: PositionType::Absolute,
                    top: px(0),
                    bottom: px(0),
                    right: px(0),
                    width: px(SCROLLBAR_WIDTH),
                    ..default()
                },
                Visibility::Hidden,
                children![(
                    Hovered::default(),
                    BackgroundColor(SCROLLBAR_COLOR),
                    ScrollbarThumb {
                        border_radius: BorderRadius::all(px(SCROLLBAR_WIDTH / 2.0)),
                        border: UiRect::ZERO,
                    },
                )],
            ));
        })),
    )
}

/// Whether a scroll box laid out as `area` has content cut off, so it can scroll.
pub fn can_scroll(area: &ComputedNode) -> bool {
    // Physical pixels: half a pixel of rounding isn't worth a scrollbar.
    area.content_size().y - area.size().y > 0.5
}

/// Show each [`scroll_column`]'s scrollbar only while its column has more than fits: with
/// nothing to scroll, the thumb would fill the whole bar.
pub(super) fn show_scrollbars_when_needed(
    mut scrollbars: Query<(&Scrollbar, &mut Visibility), With<ColumnScrollbar>>,
    areas: Query<&ComputedNode>,
) {
    for (scrollbar, mut visibility) in &mut scrollbars {
        let needed = areas.get(scrollbar.target).is_ok_and(can_scroll);
        visibility.set_if_neq(if needed {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// Scrollbar thumbs light up while hovered or dragged.
pub(super) fn style_scrollbar_thumbs(
    mut thumbs: Query<(&Hovered, &ScrollbarDragState, &mut BackgroundColor), With<ScrollbarThumb>>,
) {
    for (hovered, drag, mut background) in &mut thumbs {
        let color = if hovered.get() || drag.dragging {
            SCROLLBAR_ACTIVE_COLOR
        } else {
            SCROLLBAR_COLOR
        };
        background.set_if_neq(BackgroundColor(color));
    }
}

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
    use bevy::app::{HierarchyPropagatePlugin, PropagateSet};
    use bevy::camera::{ComputedCameraValues, RenderTargetInfo, Viewport};
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use bevy::text::RemSize;
    use bevy::transform::systems::StaticTransformOptimizations;
    use bevy::ui::ui_surface::UiSurface;
    use bevy::ui::update::propagate_ui_target_cameras;
    use bevy::ui::{
        ComputedUiRenderTargetInfo, ComputedUiTargetCamera, UiGlobalTransform,
        sync_font_size_to_em_size, ui_layout_system,
    };
    use bevy::ui_widgets::ScrollArea;

    use super::{
        ColumnScrollbar, ShownRows, can_scroll, scroll_column, show_rows,
        show_scrollbars_when_needed, skip_hint_label,
    };

    /// An app that lays out UI on a `width` × `height` pixel window, set up as Bevy's own layout
    /// tests do. It has no fonts, so these tests use boxes of a set size rather than text.
    fn layout_app(width: u32, height: u32) -> App {
        let mut app = App::new();
        app.add_plugins((
            TaskPoolPlugin::default(),
            HierarchyPropagatePlugin::<ComputedUiTargetCamera>::new(PostUpdate),
            HierarchyPropagatePlugin::<ComputedUiRenderTargetInfo>::new(PostUpdate),
        ));
        app.init_resource::<UiScale>()
            .init_resource::<UiSurface>()
            .init_resource::<bevy::text::TextPipeline>()
            .init_resource::<bevy::text::FontCx>()
            .init_resource::<RemSize>()
            .init_resource::<bevy::text::ScaleCx>()
            .init_resource::<StaticTransformOptimizations>();
        app.add_systems(
            PostUpdate,
            (
                ApplyDeferred,
                propagate_ui_target_cameras,
                sync_font_size_to_em_size,
                ui_layout_system,
            )
                .chain(),
        );
        app.configure_sets(
            PostUpdate,
            PropagateSet::<ComputedUiTargetCamera>::default()
                .after(propagate_ui_target_cameras)
                .before(ui_layout_system),
        );
        app.configure_sets(
            PostUpdate,
            PropagateSet::<ComputedUiRenderTargetInfo>::default()
                .after(propagate_ui_target_cameras)
                .before(ui_layout_system),
        );
        let size = UVec2::new(width, height);
        app.world_mut().spawn((
            Camera2d,
            Camera {
                computed: ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size: size,
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                viewport: Some(Viewport {
                    physical_size: size,
                    ..default()
                }),
                ..default()
            },
        ));
        app
    }

    /// The button under a [`scroll_column`] that must stay in view.
    #[derive(Component)]
    struct Below;

    /// A screen like the Controls screen, on a window `height` pixels tall: a title, a
    /// [`scroll_column`] of twelve 30-pixel rows (448 pixels in all), and a button under it.
    fn screen_on_window(height: u32) -> App {
        let mut app = layout_app(1000, height);
        app.world_mut().spawn((
            Node {
                // Over the rest of the menu, as the Controls screen is.
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(8),
                padding: UiRect::vertical(px(24)),
                ..default()
            },
            children![
                Node {
                    width: px(200),
                    height: px(50),
                    flex_shrink: 0.0,
                    ..default()
                },
                scroll_column(8.0, |list| {
                    for _ in 0..12 {
                        list.spawn(Node {
                            width: px(300),
                            height: px(30),
                            ..default()
                        });
                    }
                }),
                (
                    Below,
                    Node {
                        width: px(100),
                        height: px(40),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ),
            ],
        ));
        app.update();
        app.world_mut()
            .run_system_once(show_scrollbars_when_needed)
            .unwrap();
        app
    }

    fn scroll_area(app: &mut App) -> ComputedNode {
        *app.world_mut()
            .query_filtered::<&ComputedNode, With<ScrollArea>>()
            .single(app.world())
            .unwrap()
    }

    fn scrollbar_shown(app: &mut App) -> bool {
        *app.world_mut()
            .query_filtered::<&Visibility, With<ColumnScrollbar>>()
            .single(app.world())
            .unwrap()
            != Visibility::Hidden
    }

    #[test]
    fn a_scroll_column_shrinks_to_fit_a_short_window_and_scrolls() {
        let mut app = screen_on_window(300);
        let area = scroll_area(&mut app);
        // 300 - 48 padding - 50 title - 40 button - 2 gaps of 8.
        assert_eq!(area.size().y, 146.0);
        // Every row keeps its height, to be scrolled to.
        assert_eq!(area.content_size().y, 448.0);
        assert!(can_scroll(&area), "{area:?}");
        assert!(scrollbar_shown(&mut app));

        // The button under it is still on the window.
        let (button, at) = app
            .world_mut()
            .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<Below>>()
            .single(app.world())
            .unwrap();
        let bottom = at.translation.y + button.size().y / 2.0;
        assert!(bottom <= 300.0, "the button's bottom is at {bottom}");
    }

    #[test]
    fn a_scroll_column_that_fits_shows_everything_and_no_scrollbar() {
        let mut app = screen_on_window(800);
        let area = scroll_area(&mut app);
        assert_eq!(area.size().y, 448.0);
        assert!(!can_scroll(&area), "{area:?}");
        assert!(!scrollbar_shown(&mut app));
    }

    #[test]
    fn skip_hint_shows_prompt_then_vote_progress() {
        assert_eq!(skip_hint_label(false, 0, 3), "Space / A to skip");
        assert_eq!(skip_hint_label(false, 1, 3), "Space / A to skip (1/3)");
        assert_eq!(skip_hint_label(true, 2, 3), "Ready to skip (2/3)");
    }

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
