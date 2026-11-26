//! The results screen: who won, how the minigame ended, and everyone's place.

use bevy::prelude::*;
use lightyear::prelude::*;

use super::widgets::{HINT_COLOR, label, player_row, screen};
use crate::config::session::RESULTS_DURATION;
use crate::players::local_peer_id;
use crate::session::{MinigameResult, Placement, SessionPhase, ordinal};

/// The headline: who won.
#[derive(Component)]
pub struct WinnerText;

/// The line under the headline: the score, or however the minigame summed up its end.
#[derive(Component)]
pub struct SummaryText;

/// Everyone's place: one row per [`Placement`], best first.
#[derive(Component)]
pub struct PlacementList;

/// Counts down to the lobby. The host decides when that happens; this is only this app's estimate,
/// started when it saw the results.
#[derive(Component)]
pub struct LobbyCountdown(Timer);

pub(super) fn spawn_results_screen(mut commands: Commands) {
    commands.spawn((
        screen(SessionPhase::Results),
        children![
            (WinnerText, label("", 48.0, Color::WHITE)),
            (SummaryText, label("", 24.0, HINT_COLOR)),
            (
                PlacementList,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
            ),
            (
                LobbyCountdown(Timer::new(RESULTS_DURATION, TimerMode::Once)),
                label("", 20.0, HINT_COLOR),
            ),
        ],
    ));
}

/// Excludes the winner and summary lines, so the countdown's text can be borrowed alongside them.
type NotHeadlines = (Without<WinnerText>, Without<SummaryText>);

/// The result arrives from the host, possibly after this screen appears, so it's filled in when
/// it does.
#[allow(clippy::too_many_arguments)]
pub(super) fn update_results_screen(
    time: Res<Time>,
    result: Option<Single<&MinigameResult>>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut winner_text: Single<&mut Text, (With<WinnerText>, Without<SummaryText>)>,
    mut summary_text: Single<&mut Text, (With<SummaryText>, Without<WinnerText>)>,
    mut countdown: Single<(&mut LobbyCountdown, &mut Text), NotHeadlines>,
    list: Single<Entity, With<PlacementList>>,
    mut shown_rows: Local<Vec<(String, u8)>>,
    mut commands: Commands,
) {
    if let Some(result) = result {
        let you = local_peer_id(&clients);
        let winners: Vec<&Placement> = result.winners().collect();
        let shown = headline(&winners, you);
        if winner_text.0 != shown {
            winner_text.0 = shown;
        }
        if summary_text.0 != result.summary {
            summary_text.0.clone_from(&result.summary);
        }
        let rows: Vec<(String, u8)> = result
            .placements
            .iter()
            .map(|placement| (placement_label(placement, you), placement.color))
            .collect();
        if *shown_rows != rows {
            commands.entity(*list).despawn_children();
            for (text, color) in &rows {
                commands.entity(*list).with_child(player_row(text, *color));
            }
            *shown_rows = rows;
        }
    }

    let (timer, text) = &mut *countdown;
    timer.0.tick(time.delta());
    let shown = format!(
        "Back to the lobby in {}…",
        timer.0.remaining_secs().ceil().max(1.0)
    );
    if text.0 != shown {
        text.0 = shown;
    }
}

/// "You win!", "Ada wins!", or for a tie "Ada and Bo win!".
pub fn headline(winners: &[&Placement], you: Option<PeerId>) -> String {
    if winners.iter().any(|winner| Some(winner.player) == you) {
        return "You win!".into();
    }
    match winners {
        [] => "Game over".into(),
        [winner] => format!("{} wins!", winner.name),
        [first @ .., last] => {
            let first: Vec<&str> = first.iter().map(|winner| winner.name.as_str()).collect();
            format!("{} and {} win!", first.join(", "), last.name)
        }
    }
}

/// A placement's row, marking `you`: "1st Ada", "2nd Bo (you)", "3rd Cy (you, DNF)".
pub fn placement_label(placement: &Placement, you: Option<PeerId>) -> String {
    let mut notes = Vec::new();
    if Some(placement.player) == you {
        notes.push("you");
    }
    if !placement.note.is_empty() {
        notes.push(&placement.note);
    }
    let mut line = format!("{} {}", ordinal(placement.place), placement.name);
    if !notes.is_empty() {
        line.push_str(&format!(" ({})", notes.join(", ")));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::{headline, placement_label};
    use crate::session::Placement;
    use lightyear::prelude::PeerId;

    fn placement(id: u64, name: &str, place: u32) -> Placement {
        Placement {
            player: PeerId::Netcode(id),
            name: name.into(),
            color: 0,
            place,
            note: String::new(),
        }
    }

    #[test]
    fn congratulates_you() {
        let ada = placement(1, "Ada", 1);
        assert_eq!(headline(&[&ada], Some(PeerId::Netcode(1))), "You win!");
    }

    #[test]
    fn names_someone_else() {
        let ada = placement(1, "Ada", 1);
        assert_eq!(headline(&[&ada], Some(PeerId::Netcode(3))), "Ada wins!");
    }

    #[test]
    fn names_everyone_who_tied() {
        let (ada, bo, cy) = (
            placement(1, "Ada", 1),
            placement(2, "Bo", 1),
            placement(3, "Cy", 1),
        );
        assert_eq!(headline(&[&ada, &bo], None), "Ada and Bo win!");
        assert_eq!(headline(&[&ada, &bo, &cy], None), "Ada, Bo and Cy win!");
        assert_eq!(headline(&[&ada, &bo], Some(PeerId::Netcode(2))), "You win!");
    }

    #[test]
    fn copes_without_a_winner() {
        assert_eq!(headline(&[], None), "Game over");
    }

    #[test]
    fn marks_you_and_notes() {
        let mut cy = placement(3, "Cy", 3);
        assert_eq!(placement_label(&cy, None), "3rd Cy");
        assert_eq!(
            placement_label(&cy, Some(PeerId::Netcode(3))),
            "3rd Cy (you)"
        );
        cy.note = "DNF".into();
        assert_eq!(placement_label(&cy, None), "3rd Cy (DNF)");
        assert_eq!(
            placement_label(&cy, Some(PeerId::Netcode(3))),
            "3rd Cy (you, DNF)"
        );
    }
}
