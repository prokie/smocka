//! The pre-game setup screen shown after the host picks a minigame in the lobby and before they
//! click "Start game".
//!
//! Every minigame gets this screen: the host can tweak its settings here (with joiners seeing the
//! live values), everyone sees who is in the session, and for Pong and Pong Royale every player
//! picks their [`PaddleShape`].

use bevy::input_focus::AutoFocus;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Button;
use lightyear::prelude::*;

use super::MenuButton;
use super::lobby::{Thumbnail, picker, player_label, show_pick};
use super::widgets::{
    BUTTON_COLOR, HINT_COLOR, ShownRows, label, menu_button, screen, show_rows, wide_menu_button,
};
use crate::minigames::Minigame;
use crate::minigames::pong_paddle::{
    PADDLE_SPRITE_HEIGHT, PADDLE_SPRITE_WIDTH, PaddleShape, PlayerPaddleShape,
};
use crate::minigames::settings::{PublishedSettings, SettingId};
use crate::network::Hosting;
use crate::players::{Player, local_peer_id, player_color};
use crate::session::SessionPhase;

/// Width and height of one paddle shape choice card, in pixels.
const SHAPE_CARD_WIDTH: f32 = 216.0;
const SHAPE_CARD_HEIGHT: f32 = 96.0;
const SHAPE_PREVIEW_SCALE: f32 = 1.3;
const SETUP_THUMBNAIL_WIDTH: f32 = 320.0;

/// Shows the value of one minigame setting on the setup screen.
#[derive(Component, Debug)]
pub(super) struct SetupSettingValue(pub SettingId);

/// The list of players on the setup screen, rebuilt when anyone joins, leaves, or changes pick.
#[derive(Component)]
pub(super) struct SetupPlayerList;

/// A clickable card picking a [`PaddleShape`] on the setup screen.
#[derive(Component, Debug, Clone, Copy)]
pub(super) struct PaddleShapeCard(pub PaddleShape);

/// The paddle sprite preview inside a [`PaddleShapeCard`], tinted with this player's colour.
#[derive(Component, Debug, Clone, Copy)]
pub(super) struct PaddleShapePreview;

/// Status line below the setup screen's action buttons.
#[derive(Component)]
pub(super) struct SetupHint;

/// Run condition: true while the session is on any minigame's [`SessionPhase::Setup`] screen.
pub(super) fn in_setup(phase: Option<Res<State<SessionPhase>>>) -> bool {
    phase.is_some_and(|phase| matches!(*phase.get(), SessionPhase::Setup(_)))
}

/// Whether `game` lets every player pick a [`PaddleShape`] on the setup screen.
pub fn has_paddle_pick(game: Minigame) -> bool {
    matches!(game, Minigame::TwoPlayerPong | Minigame::PongRoyale)
}

/// One player's row on the setup screen: their name (marking host and you) plus their paddle
/// shape for games that use one.
pub fn setup_player_label(
    game: Minigame,
    player: &Player,
    you: Option<PeerId>,
    shape: Option<PaddleShape>,
) -> String {
    let base = player_label(player, you);
    if has_paddle_pick(game) {
        let shape = shape.unwrap_or_default();
        format!("{base} — {}", shape.name())
    } else {
        base
    }
}

/// The hint under the setup screen's controls.
pub fn setup_hint(hosting: bool, startable: bool) -> &'static str {
    match (hosting, startable) {
        (true, true) => "",
        (true, false) => "Waiting for at least one more player.",
        (false, _) => "Waiting for the host to start.",
    }
}

/// Spawn the pre-game setup screen for whichever minigame is in [`SessionPhase::Setup`].
pub(super) fn spawn_setup_screen(
    phase: Res<State<SessionPhase>>,
    hosting: Hosting,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let SessionPhase::Setup(game) = *phase.get() else {
        return;
    };
    let info = game.info();
    let is_hosting = hosting.is_hosting();

    let root = commands
        .spawn((
            screen(SessionPhase::Setup(game)),
            children![label(info.name, 48.0, Color::WHITE)],
        ))
        .id();

    let columns = commands
        .spawn(Node {
            column_gap: px(48),
            align_items: AlignItems::FlexStart,
            ..default()
        })
        .id();
    commands.entity(root).add_child(columns);

    // Left column: minigame settings (editable on the host, live read-only on joiners) + players.
    let left = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: px(12),
            ..default()
        })
        .id();
    if !info.settings.is_empty() {
        commands
            .entity(left)
            .with_child(label("Settings", 20.0, HINT_COLOR));
        for id in SettingId::all_of(game) {
            let setting = id.setting();
            let change = is_hosting.then_some(move |by| MenuButton::ChangeSetting(id, by));
            let row = picker(
                setting.name,
                SetupSettingValue(id),
                setting.step_labels(),
                change,
                &mut commands,
            );
            commands.entity(left).add_child(row);
        }
    }
    commands.entity(left).with_children(|column| {
        column.spawn((
            label("Players", 20.0, HINT_COLOR),
            Node {
                margin: UiRect::top(px(8)),
                ..default()
            },
        ));
        column.spawn((
            SetupPlayerList,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },
        ));
    });

    // Right column: per-player paddle shape picker for Pong / Pong Royale, or a live preview.
    let right = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: px(12),
            ..default()
        })
        .id();
    if has_paddle_pick(game) {
        commands
            .entity(right)
            .with_child(label("Your paddle", 20.0, HINT_COLOR));
        let grid = commands
            .spawn(Node {
                display: Display::Grid,
                grid_template_columns: RepeatedGridTrack::px(2, SHAPE_CARD_WIDTH),
                column_gap: px(12),
                row_gap: px(12),
                ..default()
            })
            .id();
        for shape in PaddleShape::ALL {
            let card = spawn_shape_card(shape, assets.load(shape.image()), &mut commands);
            commands.entity(grid).add_child(card);
        }
        commands.entity(right).add_child(grid);
    } else {
        commands.entity(right).with_child((
            Thumbnail {
                game,
                shown: info.thumbnail,
            },
            ImageNode::new(assets.load(info.thumbnail)),
            Node {
                width: px(SETUP_THUMBNAIL_WIDTH),
                height: px(SETUP_THUMBNAIL_WIDTH * 9.0 / 16.0),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
        ));
    }

    commands.entity(columns).add_children(&[left, right]);

    // Bottom bar: Start + Back for the host, status line for everyone.
    if is_hosting {
        commands.entity(root).with_child((
            Node {
                column_gap: px(16),
                margin: UiRect::top(px(12)),
                ..default()
            },
            children![
                menu_button(MenuButton::ReturnToLobby, "Back"),
                (
                    wide_menu_button(MenuButton::Start(game), "Start game"),
                    AutoFocus,
                ),
            ],
        ));
    }
    commands
        .entity(root)
        .with_child((SetupHint, label("", 20.0, HINT_COLOR)));
}

fn spawn_shape_card(shape: PaddleShape, image: Handle<Image>, commands: &mut Commands) -> Entity {
    commands
        .spawn((
            PaddleShapeCard(shape),
            MenuButton::PickPaddleShape(shape),
            Button,
            Hovered::default(),
            TabIndex(0),
            Node {
                width: px(SHAPE_CARD_WIDTH),
                height: px(SHAPE_CARD_HEIGHT),
                column_gap: px(14),
                padding: UiRect::horizontal(px(14)),
                align_items: AlignItems::Center,
                border: px(2).all(),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(BUTTON_COLOR),
            BorderColor::all(Color::NONE),
            children![
                (
                    PaddleShapePreview,
                    ImageNode::new(image),
                    Node {
                        width: px(PADDLE_SPRITE_WIDTH * SHAPE_PREVIEW_SCALE),
                        height: px(PADDLE_SPRITE_HEIGHT * SHAPE_PREVIEW_SCALE),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ),
                (
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        ..default()
                    },
                    children![
                        label(shape.name(), 20.0, Color::WHITE),
                        label(shape.summary(), 13.0, HINT_COLOR),
                    ],
                ),
            ],
        ))
        .id()
}

/// Keep every setting row on the setup screen up to date from the replicated [`PublishedSettings`].
pub(super) fn update_setup_settings(
    settings: Option<Single<&PublishedSettings>>,
    mut values: Query<(&SetupSettingValue, &mut Text)>,
    buttons: Query<(Entity, &MenuButton, Has<InteractionDisabled>)>,
    mut commands: Commands,
) {
    let Some(settings) = settings else {
        return;
    };
    for (SetupSettingValue(id), mut text) in &mut values {
        let setting = id.setting();
        let value = settings.get(*id);
        show_pick(
            value,
            setting.label(value),
            (setting.min, setting.max),
            &mut text,
            &buttons,
            &mut commands,
            |action| match action {
                MenuButton::ChangeSetting(button_id, by) if button_id == id => Some(*by),
                _ => None,
            },
        );
    }
}

/// Rebuild the player roster on the setup screen whenever players or paddle picks change.
pub(super) fn update_setup_players(
    phase: Res<State<SessionPhase>>,
    list: Single<Entity, With<SetupPlayerList>>,
    players: Query<(&Player, Option<&PlayerPaddleShape>)>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut shown: Local<ShownRows>,
    mut commands: Commands,
) {
    let SessionPhase::Setup(game) = *phase.get() else {
        return;
    };
    let you = local_peer_id(&clients);
    let mut players: Vec<_> = players.iter().collect();
    players.sort_by_key(|(player, _)| player.number);
    let rows = players
        .into_iter()
        .map(|(player, shape)| {
            (
                setup_player_label(game, player, you, shape.map(|shape| shape.0)),
                player.color,
            )
        })
        .collect();
    show_rows(*list, rows, &mut shown, &mut commands);
}

/// Highlight this player's picked paddle shape card and tint every preview in their colour.
pub(super) fn update_paddle_shape_cards(
    players: Query<(&Player, Option<&PlayerPaddleShape>)>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut cards: Query<(&PaddleShapeCard, &mut BorderColor)>,
    mut previews: Query<&mut ImageNode, With<PaddleShapePreview>>,
) {
    let you = local_peer_id(&clients);
    let local = you.and_then(|you| players.iter().find(|(player, _)| player.id == you));
    let picked = local
        .and_then(|(_, shape)| shape.map(|shape| shape.0))
        .unwrap_or_default();
    let tint = local.map_or(Color::WHITE, |(player, _)| player_color(player.color));

    for (PaddleShapeCard(shape), mut border) in &mut cards {
        let outline = if *shape == picked {
            Color::WHITE
        } else {
            Color::NONE
        };
        border.set_if_neq(BorderColor::all(outline));
    }
    for mut preview in &mut previews {
        if preview.color != tint {
            preview.color = tint;
        }
    }
}

/// Keep the setup screen's Start button and hint in sync with the player count.
pub(super) fn update_setup_hint(
    phase: Res<State<SessionPhase>>,
    players: Query<&Player>,
    hosting: Hosting,
    mut hint: Single<&mut Text, With<SetupHint>>,
    buttons: Query<(Entity, &MenuButton, Has<InteractionDisabled>)>,
    mut commands: Commands,
) {
    let SessionPhase::Setup(game) = *phase.get() else {
        return;
    };
    let startable = players.iter().len() >= game.info().min_players;
    let shown = setup_hint(hosting.is_hosting(), startable);
    if hint.0 != shown {
        hint.0 = shown.to_string();
    }
    for (button, action, disabled) in &buttons {
        if *action != MenuButton::Start(game) {
            continue;
        }
        match (startable, disabled) {
            (true, true) => {
                commands.entity(button).remove::<InteractionDisabled>();
            }
            (false, false) => {
                commands.entity(button).insert(InteractionDisabled);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::PeerId;

    use super::{has_paddle_pick, setup_hint, setup_player_label};
    use crate::minigames::Minigame;
    use crate::minigames::pong_paddle::PaddleShape;
    use crate::players::Player;

    #[test]
    fn pong_games_offer_paddle_picks_and_others_do_not() {
        assert!(has_paddle_pick(Minigame::TwoPlayerPong));
        assert!(has_paddle_pick(Minigame::PongRoyale));
        assert!(!has_paddle_pick(Minigame::Racing));
        assert!(!has_paddle_pick(Minigame::Shootout));
    }

    #[test]
    fn setup_player_label_shows_paddle_shape_only_for_pong_games() {
        let player = Player {
            id: PeerId::Local(0),
            number: 1,
            name: "Ada".into(),
            color: 0,
        };
        assert_eq!(
            setup_player_label(
                Minigame::TwoPlayerPong,
                &player,
                Some(PeerId::Local(0)),
                Some(PaddleShape::Wedge)
            ),
            "Ada (host, you) — Wedge"
        );
        assert_eq!(
            setup_player_label(
                Minigame::Racing,
                &player,
                Some(PeerId::Local(0)),
                Some(PaddleShape::Wedge)
            ),
            "Ada (host, you)"
        );
    }

    #[test]
    fn setup_hint_tracks_host_and_min_players() {
        assert_eq!(setup_hint(true, true), "");
        assert_eq!(
            setup_hint(true, false),
            "Waiting for at least one more player."
        );
        assert_eq!(setup_hint(false, true), "Waiting for the host to start.");
    }
}
