//! The screen between a tournament's matches: the round, who plays next, and how the last match
//! went.

use bevy::prelude::*;
use lightyear::prelude::*;

use super::widgets::{HINT_COLOR, countdown, label, screen};
use crate::config::session::NEXT_MATCH_DURATION;
use crate::players::{Player, local_peer_id};
use crate::session::SessionPhase;
use crate::tournament::Tournament;

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
