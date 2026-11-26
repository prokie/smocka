use bevy::prelude::*;
use lightyear::prelude::*;

use super::{HINT_COLOR, label, screen};
use crate::config::session::RESULTS_DURATION;
use crate::lobby::{Player, local_peer_id};
use crate::session::{MinigameResult, SessionPhase};

/// The headline: who won.
#[derive(Component)]
pub struct WinnerText;

/// The line under the headline: the score, or however the minigame summed up its end.
#[derive(Component)]
pub struct SummaryText;

/// Counts down to the lobby. The host decides when that happens; this is only this app's estimate,
/// started when it saw the results.
#[derive(Component)]
pub struct LobbyCountdown(Timer);

pub(super) fn spawn_results_screen(mut commands: Commands) {
    commands.spawn((
        screen(SessionPhase::Results),
        children![
            (WinnerText, label("", 48.0, Color::WHITE)),
            (SummaryText, label("", 32.0, Color::WHITE)),
            (
                LobbyCountdown(Timer::new(RESULTS_DURATION, TimerMode::Once)),
                label("", 20.0, HINT_COLOR),
            ),
        ],
    ));
}

/// Excludes the winner and summary lines, so the countdown's text can be borrowed alongside them.
type NotHeadlines = (Without<WinnerText>, Without<SummaryText>);

pub(super) fn update_results_screen(
    time: Res<Time>,
    result: Option<Single<&MinigameResult>>,
    players: Query<&Player>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut winner_text: Single<&mut Text, (With<WinnerText>, Without<SummaryText>)>,
    mut summary_text: Single<&mut Text, (With<SummaryText>, Without<WinnerText>)>,
    mut countdown: Single<(&mut LobbyCountdown, &mut Text), NotHeadlines>,
) {
    if let Some(result) = result {
        let winner = players.iter().find(|player| player.id == result.winner);
        let you = local_peer_id(&clients);
        let shown = headline(winner, you);
        if winner_text.0 != shown {
            winner_text.0 = shown;
        }
        if summary_text.0 != result.summary {
            summary_text.0.clone_from(&result.summary);
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

pub fn headline(winner: Option<&Player>, you: Option<PeerId>) -> String {
    match winner {
        Some(winner) if Some(winner.id) == you => "You win!".into(),
        Some(winner) => format!("{} wins!", winner.name()),
        // The winner left before the results arrived.
        None => "Game over".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::headline;
    use crate::lobby::Player;
    use lightyear::prelude::PeerId;

    fn host() -> Player {
        Player {
            id: PeerId::Local(0),
            number: 1,
            name: "Ada".into(),
            color: 0,
        }
    }

    #[test]
    fn congratulates_you() {
        assert_eq!(headline(Some(&host()), Some(PeerId::Local(0))), "You win!");
    }

    #[test]
    fn names_someone_else() {
        assert_eq!(
            headline(Some(&host()), Some(PeerId::Netcode(3))),
            "Ada wins!"
        );
    }

    #[test]
    fn copes_with_a_winner_who_left() {
        assert_eq!(headline(None, None), "Game over");
    }
}
