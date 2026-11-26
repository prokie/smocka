//! "Watching: Ada vs Bo (semi-final)" along the bottom of the screen, for players who aren't in
//! the minigame being played, such as those waiting for their next tournament match.

use bevy::prelude::*;
use lightyear::prelude::*;

use super::notices::NOTICE_BACKGROUND;
use super::widgets::{HINT_COLOR, label};
use crate::app_state::AppState;
use crate::players::{Player, local_peer_id};
use crate::session::{Participants, SessionPhase};
use crate::tournament::Tournament;

/// The line saying what this player is watching. Hidden while they play, or outside minigames.
#[derive(Component)]
pub struct WatchingText;

pub(super) fn spawn_watching_text(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(AppState::InSession),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(8),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            WatchingText,
            label("", 20.0, HINT_COLOR),
            Node {
                padding: UiRect::axes(px(10), px(4)),
                border_radius: BorderRadius::all(px(6)),
                ..default()
            },
            BackgroundColor(NOTICE_BACKGROUND),
            Visibility::Hidden,
        )],
    ));
}

pub(super) fn update_watching_text(
    phase: Option<Res<State<SessionPhase>>>,
    participants: Option<Single<&Participants>>,
    tournament: Option<Single<&Tournament>>,
    players: Query<&Player>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    text: Single<(&mut Text, &mut Visibility), With<WatchingText>>,
) {
    let (mut text, mut visibility) = text.into_inner();
    let you = local_peer_id(&clients);
    let shown = match (participants, you) {
        (Some(participants), Some(you))
            if phase.is_some_and(|phase| phase.get().is_minigame())
                && !participants.contains(you) =>
        {
            let names: Vec<&str> = participants
                .0
                .iter()
                .map(|id| {
                    players
                        .iter()
                        .find(|player| player.id == *id)
                        .map_or("?", Player::name)
                })
                .collect();
            let round = tournament.and_then(|tournament| {
                let (round, _) = tournament.bracket.next_match()?;
                Some(tournament.bracket.round_name(round))
            });
            watching_label(&names, round.as_deref())
        }
        _ => String::new(),
    };
    visibility.set_if_neq(if shown.is_empty() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    if text.0 != shown {
        if !shown.is_empty() {
            debug!("{shown}");
        }
        text.0 = shown;
    }
}

/// "Watching: Ada vs Bo (semi-final)" for a match between `players` in tournament round `round`,
/// or just "Watching" for a game with more players.
pub fn watching_label(players: &[&str], round: Option<&str>) -> String {
    let mut line = match players {
        [first, second] => format!("Watching: {first} vs {second}"),
        _ => "Watching".into(),
    };
    if let Some(round) = round {
        line.push_str(&format!(" ({})", round.to_lowercase()));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::watching_label;

    #[test]
    fn names_the_match_and_the_round() {
        assert_eq!(
            watching_label(&["Ada", "Bo"], Some("Semi-final")),
            "Watching: Ada vs Bo (semi-final)"
        );
        assert_eq!(watching_label(&["Ada", "Bo"], None), "Watching: Ada vs Bo");
        assert_eq!(watching_label(&["Ada", "Bo", "Cy"], None), "Watching");
    }
}
