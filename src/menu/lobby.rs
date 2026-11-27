use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, Button, ScrollArea};
use lightyear::prelude::*;

use super::MenuButton;
use super::widgets::{
    BUTTON_COLOR, HINT_COLOR, ShownRows, icon_button, icon_button_node, label, menu_button, screen,
    show_rows, small_button,
};
use crate::config::party::{MAX_ROUNDS, MIN_ROUNDS};
use crate::minigames::Minigame;
use crate::minigames::settings::PublishedSettings;
use crate::network::Hosting;
use crate::party::PartyRounds;
use crate::players::{
    PLAYER_COLORS, PickColor, Player, color_is_free, local_peer_id, player_color,
};
use crate::session::{SessionPhase, can_start};

/// Size of the colour swatches you click to pick a colour, in screen pixels.
const SWATCH_SIZE: f32 = 36.0;
/// A colour someone else has is drawn this transparent.
const TAKEN_ALPHA: f32 = 0.2;

/// The list of players in the lobby: one row per player, rebuilt when anything in it changes.
#[derive(Component)]
pub struct PlayerList;

/// The line under the player list saying what happens next.
#[derive(Component)]
pub struct LobbyHint;

/// Shows a minigame's settings as the host picked them, e.g. "Laps: 3".
#[derive(Component)]
pub struct SettingsSummary(pub Minigame);

/// A minigame's thumbnail on its tile in the lobby, and which picture it shows (see
/// [`update_thumbnails`]).
#[derive(Component, Debug)]
pub struct Thumbnail {
    pub game: Minigame,
    pub shown: &'static str,
}

/// Space between the lobby's two columns, in pixels.
const COLUMN_GAP: f32 = 64.0;
/// How many minigames each row of the lobby's grid holds.
const GRID_COLUMNS: usize = 3;
/// Space between tiles, in pixels.
const GRID_GAP: f32 = 12.0;
/// Space around a tile's insides, in pixels.
const TILE_PADDING: f32 = 6.0;
/// A tile's thumbnail is this wide, in pixels, and 16:9.
const THUMBNAIL_WIDTH: f32 = 176.0;
/// A tile's size, in pixels: its thumbnail, name and settings summary.
const TILE_WIDTH: f32 = THUMBNAIL_WIDTH + 2.0 * TILE_PADDING;
const TILE_HEIGHT: f32 = THUMBNAIL_WIDTH * 9.0 / 16.0 + 2.0 * TILE_PADDING + 52.0;
/// The settings icon on a tile.
const GEAR_ICON: &str = "menu/gear.png";

/// The host's pick of how many rounds the next party has.
#[derive(Component)]
pub struct RoundsText;

/// A clickable swatch that picks colour `.0` from [`PLAYER_COLORS`].
#[derive(Component, Debug, Clone, Copy)]
pub struct ColorSwatch(pub u8);

/// The lobby: the title, then two columns, then the hint. On the left, who's here and the colour
/// you pick. On the right, what to play: a party, or one minigame from a grid. Two columns keep
/// it short enough for eight players in a 720-pixel-high window.
pub(super) fn spawn_lobby_screen(
    hosting: Hosting,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let root = commands
        .spawn((
            screen(SessionPhase::Lobby),
            children![label("Lobby", 48.0, Color::WHITE)],
        ))
        .id();
    let columns = commands
        .spawn(Node {
            column_gap: px(COLUMN_GAP),
            align_items: AlignItems::FlexStart,
            ..default()
        })
        .id();
    commands.entity(root).add_child(columns);

    let players = commands
        .spawn((
            column(),
            children![
                label("Players", 20.0, HINT_COLOR),
                (
                    PlayerList,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    },
                ),
                label("Your colour", 20.0, HINT_COLOR),
            ],
        ))
        .id();
    let swatches = commands
        .spawn(Node {
            column_gap: px(10),
            ..default()
        })
        .id();
    for index in 0..PLAYER_COLORS.len() as u8 {
        commands.entity(swatches).with_child(swatch(index));
    }
    commands.entity(players).add_child(swatches);

    let play = commands.spawn(column()).id();
    let party = commands
        .spawn(Node {
            column_gap: px(24),
            align_items: AlignItems::Center,
            ..default()
        })
        .id();
    if hosting.is_hosting() {
        commands.entity(party).with_child((
            menu_button(MenuButton::StartParty, "Start party"),
            InteractionDisabled,
        ));
    }
    let rounds = picker(
        "Party rounds",
        RoundsText,
        hosting.is_hosting().then_some(MenuButton::ChangeRounds),
        &mut commands,
    );
    commands.entity(party).add_child(rounds);
    commands.entity(play).add_child(party);
    spawn_games(
        play,
        hosting.is_hosting(),
        &|path| assets.load(path),
        &mut commands,
    );

    commands.entity(columns).add_children(&[players, play]);
    commands
        .entity(root)
        .with_child((LobbyHint, label("", 20.0, HINT_COLOR)));
}

/// One of the lobby's two columns: its parts stacked, lined up on the left.
fn column() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::FlexStart,
        row_gap: px(12),
        ..default()
    }
}

/// The minigames, as a grid of tiles under `parent`, in a box that scrolls once there are more than
/// [`GRID_COLUMNS`] of them. Each tile has the minigame's thumbnail, its name and a summary of its
/// settings. On the host the whole tile is a button that starts it, with a gear in the corner
/// that opens its settings. Joiners see the same tiles, to see what can be played and the host's
/// picks. `image` loads an image by its path under `assets/`.
fn spawn_games(
    parent: Entity,
    hosting: bool,
    image: &impl Fn(&'static str) -> Handle<Image>,
    commands: &mut Commands,
) {
    let heading = if hosting {
        "Or play one game:"
    } else {
        "Minigames"
    };
    commands
        .entity(parent)
        .with_child(label(heading, 20.0, HINT_COLOR));
    // Only as many columns as there are minigames, so a short first row stays centred.
    let columns = Minigame::ALL.len().clamp(1, GRID_COLUMNS) as u16;
    let grid = commands
        .spawn(Node {
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::px(columns, TILE_WIDTH),
            column_gap: px(GRID_GAP),
            row_gap: px(GRID_GAP),
            ..default()
        })
        .id();
    for game in Minigame::ALL {
        let tile = spawn_tile(game, hosting, image, commands);
        commands.entity(grid).add_child(tile);
    }
    let scroll = commands
        .spawn((
            ScrollArea,
            Node {
                // Most of a second row, so a cut-off row shows there's more to scroll to. As tall
                // as the players column with eight players.
                max_height: px(TILE_HEIGHT * 1.75 + GRID_GAP),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .add_child(grid)
        .id();
    commands.entity(parent).add_child(scroll);
}

/// `game`'s tile in the lobby's grid (see [`spawn_games`]).
fn spawn_tile(
    game: Minigame,
    hosting: bool,
    image: &impl Fn(&'static str) -> Handle<Image>,
    commands: &mut Commands,
) -> Entity {
    let info = game.info();
    let look = (
        Node {
            width: px(TILE_WIDTH),
            height: px(TILE_HEIGHT),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: px(TILE_PADDING).all(),
            row_gap: px(4),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BackgroundColor(BUTTON_COLOR),
    );
    let face = commands
        .spawn(look)
        .with_children(|face| {
            face.spawn((
                Thumbnail {
                    game,
                    shown: info.thumbnail,
                },
                ImageNode::new(image(info.thumbnail)),
                Node {
                    width: px(THUMBNAIL_WIDTH),
                    height: px(THUMBNAIL_WIDTH * 9.0 / 16.0),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
            ));
            face.spawn(label(info.name, 20.0, Color::WHITE));
            face.spawn((SettingsSummary(game), label("", 14.0, HINT_COLOR)));
        })
        .id();
    if !hosting {
        return face;
    }
    // The tile is the start button. The gear is laid over its corner, not inside it, so
    // clicking the gear is never also a click on the tile.
    commands.entity(face).insert((
        MenuButton::Start(game),
        Button,
        Hovered::default(),
        TabIndex(0),
        InteractionDisabled,
    ));
    let tile = commands
        .spawn(Node {
            width: px(TILE_WIDTH),
            height: px(TILE_HEIGHT),
            ..default()
        })
        .add_child(face)
        .id();
    if !info.settings.is_empty() {
        commands.entity(tile).with_child((
            icon_button(MenuButton::MinigameSettings(game), image(GEAR_ICON)),
            Node {
                position_type: PositionType::Absolute,
                top: px(TILE_PADDING + 4.0),
                right: px(TILE_PADDING + 4.0),
                ..icon_button_node()
            },
        ));
    }
    tile
}

/// Show on each tile the picture that fits the host's picks, such as racing's picked track. From
/// the replicated [`PublishedSettings`], so joiners' tiles change with the host's.
pub(super) fn update_thumbnails(
    settings: Option<Single<&PublishedSettings>>,
    assets: Res<AssetServer>,
    mut thumbnails: Query<(&mut Thumbnail, &mut ImageNode)>,
) {
    let Some(settings) = settings else {
        return;
    };
    for (mut thumbnail, mut image) in &mut thumbnails {
        let wanted = thumbnail.game.thumbnail(&settings);
        if thumbnail.shown != wanted {
            debug!("The {} tile now shows {wanted}", thumbnail.game.info().name);
            thumbnail.shown = wanted;
            image.image = assets.load(wanted);
        }
    }
}

/// Dim a thumbnail while its tile can't be started (too few players), as the tile itself is.
pub(super) fn dim_disabled_thumbnails(
    mut thumbnails: Query<(&ChildOf, &mut ImageNode), With<Thumbnail>>,
    tiles: Query<Has<InteractionDisabled>, With<MenuButton>>,
) {
    for (parent, mut image) in &mut thumbnails {
        let disabled = tiles.get(parent.parent()).unwrap_or(false);
        let color = if disabled {
            Color::srgb(0.45, 0.45, 0.45)
        } else {
            Color::WHITE
        };
        if image.color != color {
            image.color = color;
        }
    }
}

/// A row showing one of the host's picks: `name`, then the value in a text marked with `marker`.
/// Everyone sees it; only the host gets "-" and "+", which are `change(-1)` and `change(1)`.
pub(super) fn picker(
    name: &str,
    marker: impl Component,
    change: Option<impl Fn(i32) -> MenuButton>,
    commands: &mut Commands,
) -> Entity {
    let row = commands
        .spawn((
            Node {
                column_gap: px(12),
                align_items: AlignItems::Center,
                ..default()
            },
            children![label(name, 20.0, HINT_COLOR)],
        ))
        .id();
    let text = commands
        .spawn((
            marker,
            label("", 24.0, Color::WHITE),
            Node {
                min_width: px(30),
                justify_content: JustifyContent::Center,
                ..default()
            },
            TextLayout::justify(Justify::Center),
        ))
        .id();
    match change {
        Some(change) => {
            commands
                .entity(row)
                .with_child(small_button(change(-1), "-"))
                .add_child(text)
                .with_child(small_button(change(1), "+"));
        }
        None => {
            commands.entity(row).add_child(text);
        }
    }
    row
}

fn swatch(index: u8) -> impl Bundle {
    (
        ColorSwatch(index),
        Button,
        Hovered::default(),
        TabIndex(0),
        Node {
            width: px(SWATCH_SIZE),
            height: px(SWATCH_SIZE),
            border: px(3).all(),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(player_color(index)),
        BorderColor::all(Color::NONE),
    )
}

/// Clicking a swatch asks for its colour.
pub(super) fn on_swatch(
    trigger: On<Activate>,
    swatches: Query<&ColorSwatch>,
    mut commands: Commands,
) {
    if let Ok(ColorSwatch(index)) = swatches.get(trigger.entity) {
        commands.queue(PickColor(*index));
    }
}

/// The players, sorted in joining order, and which of them is this app's.
fn players_and_you<'a>(
    players: &'a Query<&Player>,
    clients: &Query<&LocalId, (With<Client>, With<Connected>)>,
) -> (Vec<&'a Player>, Option<PeerId>) {
    let mut players: Vec<&Player> = players.iter().collect();
    players.sort_by_key(|player| player.number);
    (players, local_peer_id(clients))
}

/// Rebuild the player rows when a name, a colour or the players themselves change.
pub(super) fn update_player_list(
    players: Query<&Player>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    list: Single<Entity, With<PlayerList>>,
    mut shown: Local<ShownRows>,
    mut commands: Commands,
) {
    let (players, you) = players_and_you(&players, &clients);
    let rows = players
        .iter()
        .map(|player| (player_label(player, you), player.color))
        .collect();
    show_rows(*list, rows, &mut shown, &mut commands);
}

/// Outline your own colour, dim and disable colours other players have.
pub(super) fn update_swatches(
    players: Query<&Player>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut swatches: Query<SwatchLook>,
    focus: Res<InputFocus>,
    focus_visible: Res<InputFocusVisible>,
    mut commands: Commands,
) {
    let (players, you) = players_and_you(&players, &clients);
    let own = players.iter().find(|player| Some(player.id) == you);
    for (ColorSwatch(index), disabled, hovered, mut background, mut border, entity) in &mut swatches
    {
        let yours = own.is_some_and(|own| own.color == *index);
        let free = own.is_some_and(|own| color_is_free(players.iter().copied(), own.id, *index));
        let alpha = if free { 1.0 } else { TAKEN_ALPHA };
        background.set_if_neq(BackgroundColor(player_color(*index).with_alpha(alpha)));
        let tabbed_to = focus_visible.0 && focus.get() == Some(entity);
        let outline = match (yours, (hovered.get() || tabbed_to) && free) {
            (true, _) => Color::WHITE,
            (false, true) => HINT_COLOR,
            (false, false) => Color::NONE,
        };
        border.set_if_neq(BorderColor::all(outline));
        match (free && !yours, disabled) {
            (true, true) => {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
            (false, false) => {
                commands.entity(entity).insert(InteractionDisabled);
            }
            _ => {}
        }
    }
}

/// Keep the hint and the start buttons up to date with how many players are here.
pub(super) fn update_lobby_hint(
    players: Query<&Player>,
    hosting: Hosting,
    mut hint: Single<&mut Text, With<LobbyHint>>,
    buttons: Query<(Entity, &MenuButton, Has<InteractionDisabled>)>,
    mut commands: Commands,
) {
    let startable = can_start(players.iter().len());
    let shown = lobby_hint(hosting.is_hosting(), startable);
    if hint.0 != shown {
        hint.0 = shown.to_string();
    }
    for (button, action, disabled) in &buttons {
        let startable = match action {
            MenuButton::Start(game) => players.iter().len() >= game.info().min_players,
            MenuButton::StartParty => startable,
            _ => continue,
        };
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

/// Show each minigame's settings as the host picked them (to everyone, from the replicated
/// [`PublishedSettings`]). Empty until they arrive.
pub(super) fn update_settings_summaries(
    settings: Option<Single<&PublishedSettings>>,
    mut summaries: Query<(&SettingsSummary, &mut Text)>,
) {
    for (SettingsSummary(game), mut text) in &mut summaries {
        let shown = settings
            .as_ref()
            .map(|settings| settings.summary(*game))
            .unwrap_or_default();
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

/// Show the host's pick of rounds (to everyone, from the replicated [`PartyRounds`]), and on the
/// host disable "-" and "+" at the limits.
pub(super) fn update_round_picker(
    rounds: Option<Single<&PartyRounds>>,
    mut text: Single<&mut Text, With<RoundsText>>,
    buttons: Query<(Entity, &MenuButton, Has<InteractionDisabled>)>,
    mut commands: Commands,
) {
    let Some(rounds) = rounds.map(|rounds| rounds.0) else {
        return;
    };
    show_pick(
        rounds,
        rounds.to_string(),
        (MIN_ROUNDS, MAX_ROUNDS),
        &mut text,
        &buttons,
        &mut commands,
        |action| match action {
            MenuButton::ChangeRounds(change) => Some(*change),
            _ => None,
        },
    );
}

/// Show `value` in `text` as `shown`, and disable the buttons that `change_of` says change it once
/// it's at the end of `(min, max)` they move it towards.
pub(super) fn show_pick(
    value: u32,
    shown: String,
    (min, max): (u32, u32),
    text: &mut Text,
    buttons: &Query<(Entity, &MenuButton, Has<InteractionDisabled>)>,
    commands: &mut Commands,
    change_of: impl Fn(&MenuButton) -> Option<i32>,
) {
    if text.0 != shown {
        text.0 = shown;
    }
    for (button, action, disabled) in buttons {
        let Some(change) = change_of(action) else {
            continue;
        };
        let at_limit = (change < 0 && value <= min) || (change > 0 && value >= max);
        match (at_limit, disabled) {
            (true, false) => {
                commands.entity(button).insert(InteractionDisabled);
            }
            (false, true) => {
                commands.entity(button).remove::<InteractionDisabled>();
            }
            _ => {}
        }
    }
}

/// A swatch's state and the parts of it that show that state.
type SwatchLook = (
    &'static ColorSwatch,
    Has<InteractionDisabled>,
    &'static Hovered,
    &'static mut BackgroundColor,
    &'static mut BorderColor,
    Entity,
);

/// A player's name, marking the host and `you`.
pub fn player_label(player: &Player, you: Option<PeerId>) -> String {
    let mut line = player.name().to_string();
    match (player.is_host(), Some(player.id) == you) {
        (true, true) => line.push_str(" (host, you)"),
        (true, false) => line.push_str(" (host)"),
        (false, true) => line.push_str(" (you)"),
        (false, false) => {}
    }
    line
}

pub fn lobby_hint(hosting: bool, startable: bool) -> &'static str {
    match (hosting, startable) {
        (true, true) => "Start a party when everyone is here.",
        (true, false) => "Waiting for at least one more player.",
        (false, _) => "Waiting for the host to start.",
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;
    use bevy::ui::InteractionDisabled;
    use bevy::ui_widgets::ScrollArea;

    use super::{
        SettingsSummary, Thumbnail, lobby_hint, player_label, spawn_games, update_thumbnails,
    };
    use crate::menu::MenuButton;
    use crate::minigames::Minigame;
    use crate::minigames::racing::TRACK_SETTING;
    use crate::minigames::racing::track::TrackId;
    use crate::minigames::settings::{PublishedSettings, SettingValues};
    use crate::players::Player;
    use lightyear::prelude::PeerId;

    /// The lobby's minigame grid, as the host (`hosting`) or a joiner sees it.
    fn games(hosting: bool) -> World {
        let mut world = World::new();
        let root = world.spawn(Node::default()).id();
        spawn_games(root, hosting, &|_| Handle::default(), &mut world.commands());
        world.flush();
        world
    }

    fn count<F: bevy::ecs::query::QueryFilter>(world: &mut World) -> usize {
        world.query_filtered::<(), F>().iter(world).count()
    }

    fn buttons(world: &mut World) -> Vec<MenuButton> {
        world.query::<&MenuButton>().iter(world).copied().collect()
    }

    #[test]
    fn the_host_gets_a_tile_per_minigame_that_starts_it_and_a_gear_for_its_settings() {
        let mut world = games(true);
        let buttons = buttons(&mut world);
        for game in Minigame::ALL {
            assert!(buttons.contains(&MenuButton::Start(game)), "{buttons:?}");
            assert_eq!(
                buttons.contains(&MenuButton::MinigameSettings(game)),
                !game.info().settings.is_empty(),
                "{buttons:?}"
            );
        }
        // Until enough players are here (see `update_lobby_hint`).
        assert_eq!(
            count::<(With<MenuButton>, With<InteractionDisabled>)>(&mut world),
            Minigame::ALL.len()
        );
        assert_eq!(count::<With<Thumbnail>>(&mut world), Minigame::ALL.len());
        assert_eq!(count::<With<ScrollArea>>(&mut world), 1);
    }

    #[test]
    fn joiners_see_every_minigame_and_its_settings_but_cant_start_or_change_them() {
        let mut world = games(false);
        assert_eq!(buttons(&mut world), []);
        assert_eq!(count::<With<Thumbnail>>(&mut world), Minigame::ALL.len());
        assert_eq!(
            count::<With<SettingsSummary>>(&mut world),
            Minigame::ALL.len()
        );
    }

    fn player(id: PeerId, number: u32, name: &str) -> Player {
        Player {
            id,
            number,
            name: name.into(),
            color: 0,
        }
    }

    #[test]
    fn marks_the_host_and_you() {
        let host = player(PeerId::Local(0), 1, "Ada");
        let joiner = player(PeerId::Netcode(7), 2, "Bo");
        assert_eq!(player_label(&host, Some(PeerId::Netcode(7))), "Ada (host)");
        assert_eq!(player_label(&joiner, Some(PeerId::Netcode(7))), "Bo (you)");
        assert_eq!(
            player_label(&host, Some(PeerId::Local(0))),
            "Ada (host, you)"
        );
        assert_eq!(player_label(&joiner, Some(PeerId::Local(0))), "Bo");
    }

    #[test]
    fn joiners_wait_for_the_host() {
        assert_eq!(lobby_hint(false, true), "Waiting for the host to start.");
        assert_eq!(
            lobby_hint(true, false),
            "Waiting for at least one more player."
        );
    }

    /// The path of the picture `game`'s tile shows, as [`Thumbnail::shown`] and as the loaded
    /// image.
    fn tile_picture(app: &mut App, game: Minigame) -> (&'static str, String) {
        let (thumbnail, image) = app
            .world_mut()
            .query::<(&Thumbnail, &ImageNode)>()
            .iter(app.world())
            .find(|(thumbnail, _)| thumbnail.game == game)
            .unwrap();
        let loaded = image
            .image
            .path()
            .map(ToString::to_string)
            .unwrap_or_default();
        (thumbnail.shown, loaded)
    }

    #[test]
    fn racings_tile_follows_the_hosts_track_on_every_peer() {
        // What a joiner has: the replicated picks, not the host's own settings.
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .add_systems(Update, update_thumbnails);
        for game in Minigame::ALL {
            app.world_mut().spawn((
                Thumbnail {
                    game,
                    shown: game.info().thumbnail,
                },
                ImageNode::default(),
            ));
        }
        let mut picks = SettingValues::default();
        picks.set(TRACK_SETTING, TrackId::Oval as u32);
        let published = app.world_mut().spawn(PublishedSettings(picks.clone())).id();

        app.update();
        let oval = TrackId::Oval.layout().thumbnail;
        assert_eq!(
            tile_picture(&mut app, Minigame::Racing),
            (oval, oval.into())
        );
        // The other tiles keep their pictures.
        let pong = Minigame::TwoPlayerPong.info().thumbnail;
        assert_eq!(tile_picture(&mut app, Minigame::TwoPlayerPong).0, pong);

        picks.set(TRACK_SETTING, TrackId::FigureEight as u32);
        app.world_mut()
            .entity_mut(published)
            .insert(PublishedSettings(picks));
        app.update();
        let figure_eight = TrackId::FigureEight.layout().thumbnail;
        assert_eq!(
            tile_picture(&mut app, Minigame::Racing),
            (figure_eight, figure_eight.into())
        );
    }
}
