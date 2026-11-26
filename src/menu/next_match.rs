//! The screen between a tournament's matches: the round, who plays next, how the last match
//! went, and the whole bracket.

use bevy::prelude::*;
use lightyear::prelude::*;

use super::widgets::{BUTTON_COLOR, HINT_COLOR, countdown, label, screen};
use crate::config::session::NEXT_MATCH_DURATION;
use crate::players::{Player, local_peer_id, player_color};
use crate::session::SessionPhase;
use crate::tournament::{BracketMatch, Tournament};

/// A player who lost their match is drawn this transparent in the bracket.
const KNOCKED_OUT_ALPHA: f32 = 0.35;
/// Text size of the names in the bracket.
const BRACKET_TEXT_SIZE: f32 = 18.0;

/// Every round of the tournament side by side, rebuilt when the tournament changes.
#[derive(Component)]
pub struct BracketView;

/// The screen's lines that show the tournament.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextMatchLine {
    /// How the last match went, e.g. "Cy beat Dan (1 - 0)".
    LastMatch,
    /// The round being played, e.g. "Semi-final".
    Round,
    /// Who plays next, e.g. "Ada vs Bo (you)".
    Match,
}

pub(super) fn spawn_next_match_screen(mut commands: Commands) {
    commands.spawn((
        screen(SessionPhase::NextMatch),
        children![
            (NextMatchLine::LastMatch, label("", 20.0, HINT_COLOR)),
            (NextMatchLine::Round, label("", 48.0, Color::WHITE)),
            (NextMatchLine::Match, label("", 32.0, Color::WHITE)),
            (
                BracketView,
                Node {
                    column_gap: px(32),
                    align_items: AlignItems::Center,
                    margin: UiRect::vertical(px(8)),
                    ..default()
                },
            ),
            countdown(NEXT_MATCH_DURATION, "Starting in"),
        ],
    ));
}

/// The [`Tournament`] arrives from the host, possibly after this screen appears, so it's filled
/// in when it does.
pub(super) fn update_next_match_screen(
    tournament: Option<Single<&Tournament>>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut lines: Query<(&NextMatchLine, &mut Text)>,
) {
    let Some(tournament) = tournament else {
        return;
    };
    let Some((round, players)) = tournament.bracket.next_match() else {
        return;
    };
    let you = local_peer_id(&clients);
    let name = |id: PeerId| {
        let name = tournament.bracket.player(id).map_or("?", Player::name);
        if Some(id) == you {
            format!("{name} (you)")
        } else {
            name.to_string()
        }
    };
    for (line, mut text) in &mut lines {
        let shown = match line {
            NextMatchLine::LastMatch => tournament.last_match.clone(),
            NextMatchLine::Round => tournament.bracket.round_name(round),
            NextMatchLine::Match => format!("{} vs {}", name(players[0]), name(players[1])),
        };
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

/// Rebuild the [`BracketView`] when the tournament changes, or the screen is new.
pub(super) fn update_bracket_view(
    tournament: Option<Single<&Tournament>>,
    view: Single<Entity, With<BracketView>>,
    mut shown: Local<Option<(Entity, Tournament)>>,
    mut commands: Commands,
) {
    let Some(tournament) = tournament else {
        return;
    };
    if shown
        .as_ref()
        .is_some_and(|(view_shown, tournament_shown)| {
            *view_shown == *view && tournament_shown == *tournament
        })
    {
        return;
    }
    let bracket = &tournament.bracket;
    let next = bracket.next_match();
    commands.entity(*view).despawn_children();
    for (round, matches) in bracket.overview().into_iter().enumerate() {
        let column = commands
            .spawn(Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                row_gap: px(10),
                ..default()
            })
            .with_child(label(
                bracket.round_name(round),
                BRACKET_TEXT_SIZE,
                HINT_COLOR,
            ))
            .id();
        for pairing in matches {
            let up_next = next.is_some_and(|(next_round, players)| {
                next_round == round && pairing.players == players.map(Some)
            });
            let card = commands
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::axes(px(10), px(6)),
                        border: px(2).all(),
                        border_radius: BorderRadius::all(px(6)),
                        min_width: px(120),
                        ..default()
                    },
                    BackgroundColor(BUTTON_COLOR),
                    BorderColor::all(if up_next { Color::WHITE } else { Color::NONE }),
                ))
                .id();
            for seat in 0..2 {
                let (text, color) = seat_label(&tournament, &pairing, seat);
                commands
                    .entity(card)
                    .with_child(label(text, BRACKET_TEXT_SIZE, color));
            }
            commands.entity(column).add_child(card);
        }
        commands.entity(*view).add_child(column);
    }
    *shown = Some((*view, tournament.clone()));
}

/// What seat `seat` (0 or 1) of `pairing` shows: the player in their colour, dimmed once they've
/// lost; "bye"; or "?" until it's known who plays.
fn seat_label(tournament: &Tournament, pairing: &BracketMatch, seat: usize) -> (String, Color) {
    match pairing.players[seat] {
        Some(id) => {
            let player = tournament.bracket.player(id);
            let name = player.map_or("?", Player::name).to_string();
            let color = player.map_or(Color::WHITE, |player| player_color(player.color));
            let lost = pairing.winner.is_some_and(|winner| winner != id);
            let alpha = if lost { KNOCKED_OUT_ALPHA } else { 1.0 };
            (name, color.with_alpha(alpha))
        }
        None if pairing.bye => ("bye".into(), HINT_COLOR),
        None => ("?".into(), HINT_COLOR),
    }
}
