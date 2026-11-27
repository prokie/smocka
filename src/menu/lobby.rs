use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, Button};
use lightyear::prelude::*;

use super::MenuButton;
use super::widgets::{
    HINT_COLOR, ShownRows, label, menu_button, screen, settings_button, show_rows, small_button,
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

/// How wide a minigame's settings summary is, in pixels, so the Settings buttons line up.
const SUMMARY_WIDTH: f32 = 220.0;

/// The host's pick of how many rounds the next party has.
#[derive(Component)]
pub struct RoundsText;

/// A clickable swatch that picks colour `.0` from [`PLAYER_COLORS`].
#[derive(Component, Debug, Clone, Copy)]
pub struct ColorSwatch(pub u8);

pub(super) fn spawn_lobby_screen(hosting: Hosting, mut commands: Commands) {
    let root = commands
        .spawn((
            screen(SessionPhase::Lobby),
            children![
                label("Lobby", 48.0, Color::WHITE),
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
    commands.entity(root).add_child(swatches);
    if hosting.is_hosting() {
        commands.entity(root).with_child((
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
    commands.entity(root).add_child(rounds);
    spawn_games(root, hosting.is_hosting(), &mut commands);
    commands
        .entity(root)
        .with_child((LobbyHint, label("", 20.0, HINT_COLOR)));
}

/// One row per minigame, under `root`. The host gets a button to start it, a summary of its
/// settings, and a button to change them. Joiners see the minigames with settings, and the host's
/// picks.
fn spawn_games(root: Entity, hosting: bool, commands: &mut Commands) {
    let games: Vec<Minigame> = Minigame::ALL
        .into_iter()
        .filter(|game| hosting || !game.info().settings.is_empty())
        .collect();
    if games.is_empty() {
        return;
    }
    let heading = if hosting {
        "Or play one game:"
    } else {
        "Minigame settings"
    };
    commands
        .entity(root)
        .with_child(label(heading, 20.0, HINT_COLOR));
    let list = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            ..default()
        })
        .id();
    for game in games {
        let row = commands
            .spawn(Node {
                column_gap: px(12),
                align_items: AlignItems::Center,
                ..default()
            })
            .id();
        if hosting {
            commands.entity(row).with_child((
                menu_button(MenuButton::Start(game), game.info().name),
                InteractionDisabled,
            ));
        } else {
            commands.entity(row).with_child((
                label(game.info().name, 24.0, Color::WHITE),
                Node {
                    width: px(120),
                    ..default()
                },
            ));
        }
        commands.entity(row).with_child((
            SettingsSummary(game),
            label("", 18.0, HINT_COLOR),
            Node {
                width: px(SUMMARY_WIDTH),
                ..default()
            },
        ));
        if hosting && !game.info().settings.is_empty() {
            commands
                .entity(row)
                .with_child(settings_button(MenuButton::MinigameSettings(game)));
        }
        commands.entity(list).add_child(row);
    }
    commands.entity(root).add_child(list);
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

/// Show `value` in `text`, and disable the buttons that `change_of` says change it once it's at
/// the end of `(min, max)` they move it towards.
pub(super) fn show_pick(
    value: u32,
    (min, max): (u32, u32),
    text: &mut Text,
    buttons: &Query<(Entity, &MenuButton, Has<InteractionDisabled>)>,
    commands: &mut Commands,
    change_of: impl Fn(&MenuButton) -> Option<i32>,
) {
    let shown = value.to_string();
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
    use super::{lobby_hint, player_label};
    use crate::players::Player;
    use lightyear::prelude::PeerId;

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
}
