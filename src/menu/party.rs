//! A party's screens: the next round's minigame before each round, and the winner after the last.

use bevy::prelude::*;

use super::standings::StandingsList;
use super::widgets::{HINT_COLOR, countdown, label, screen};
use crate::config::party::{NEXT_ROUND_DURATION, WINNER_DURATION};
use crate::party::{Party, party_winners, winners_line};
use crate::players::Player;
use crate::session::SessionPhase;
use crate::standings::Points;

/// The next-round screen's lines that show the party.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextRoundLine {
    /// "Round 2 of 5".
    Round,
    /// "Next up: Racing".
    NextUp,
}

/// The headline of the party's last screen: "Ada wins the party!".
#[derive(Component)]
pub struct WinnerLine;

fn standings_list() -> impl Bundle {
    (
        StandingsList,
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        },
    )
}

pub(super) fn spawn_next_round_screen(mut commands: Commands) {
    commands.spawn((
        screen(SessionPhase::NextRound),
        children![
            (NextRoundLine::Round, label("", 48.0, Color::WHITE)),
            (NextRoundLine::NextUp, label("", 32.0, Color::WHITE)),
            standings_list(),
            countdown(NEXT_ROUND_DURATION, "Starting in"),
        ],
    ));
}

/// The [`Party`] arrives from the host, possibly after this screen appears, so it's filled in when
/// it does.
pub(super) fn update_next_round_screen(
    party: Option<Single<&Party>>,
    mut lines: Query<(&NextRoundLine, &mut Text)>,
) {
    let Some(party) = party else {
        return;
    };
    for (line, mut text) in &mut lines {
        let shown = next_round_line(*line, &party);
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

/// What `line` of the next-round screen shows for `party`.
pub fn next_round_line(line: NextRoundLine, party: &Party) -> String {
    match line {
        NextRoundLine::Round if party.round() == party.rounds => "Final round".into(),
        NextRoundLine::Round => format!("Round {} of {}", party.round(), party.rounds),
        NextRoundLine::NextUp => match party.next {
            Some(game) => format!("Next up: {}", game.info().name),
            None => "Picking a game...".into(),
        },
    }
}

pub(super) fn spawn_party_over_screen(mut commands: Commands) {
    commands.spawn((
        screen(SessionPhase::PartyOver),
        children![
            (WinnerLine, label("", 48.0, Color::WHITE)),
            label("Final standings", 20.0, HINT_COLOR),
            standings_list(),
            countdown(WINNER_DURATION, "Back to the lobby in"),
        ],
    ));
}

/// Name the winners, in the winner's colour if there's only one.
pub(super) fn update_party_over_screen(
    players: Query<(&Player, &Points)>,
    line: Single<(&mut Text, &mut TextColor), With<WinnerLine>>,
) {
    let winners = party_winners(
        players
            .iter()
            .map(|(player, points)| (player.clone(), *points)),
    );
    let shown = winners_line(&winners);
    let color = match winners.as_slice() {
        [one] => one.color(),
        _ => Color::WHITE,
    };
    let (mut text, mut text_color) = line.into_inner();
    if text.0 != shown {
        text.0 = shown;
    }
    text_color.set_if_neq(TextColor(color));
}

#[cfg(test)]
mod tests {
    use super::{NextRoundLine, next_round_line};
    use crate::minigames::Minigame;
    use crate::party::Party;

    #[test]
    fn the_next_round_screen_shows_the_round_and_the_pick() {
        let mut party = Party::new(5);
        party.played = 1;
        assert_eq!(
            next_round_line(NextRoundLine::Round, &party),
            "Round 2 of 5"
        );
        assert_eq!(
            next_round_line(NextRoundLine::NextUp, &party),
            "Picking a game..."
        );
        party.next = Some(Minigame::Racing);
        assert_eq!(
            next_round_line(NextRoundLine::NextUp, &party),
            "Next up: Racing"
        );
        party.played = 4;
        assert_eq!(next_round_line(NextRoundLine::Round, &party), "Final round");
    }
}
