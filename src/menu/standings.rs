//! The standings screen after each result: everyone's points so far, leader first.

use bevy::prelude::*;
use lightyear::prelude::*;

use super::widgets::{HINT_COLOR, ShownRows, countdown, label, screen, show_rows};
use crate::config::session::STANDINGS_DURATION;
use crate::party::Party;
use crate::players::{Player, local_peer_id};
use crate::session::{SessionPhase, ordinal};
use crate::standings::{Points, Standing, standings};

/// One row per player: place, name, points.
#[derive(Component)]
pub struct StandingsList;

/// In a [`Party`], it says which round was just played, and what comes next.
pub(super) fn spawn_standings_screen(party: Option<Single<&Party>>, mut commands: Commands) {
    let party = party.map(|party| (*party).clone());
    let (progress, next) = standings_captions(party.as_ref());
    commands.spawn((
        screen(SessionPhase::Standings),
        children![
            label("Standings", 48.0, Color::WHITE),
            label(progress, 20.0, HINT_COLOR),
            (
                StandingsList,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    ..default()
                },
            ),
            countdown(STANDINGS_DURATION, next),
        ],
    ));
}

/// The line under the headline (how far the party is), and what the countdown counts down to.
pub fn standings_captions(party: Option<&Party>) -> (String, &'static str) {
    match party {
        None => (String::new(), "Back to the lobby in"),
        Some(party) if party.is_over() => ("After the final round".into(), "The winner in"),
        Some(party) => (
            format!("After round {} of {}", party.played, party.rounds),
            "Next round in",
        ),
    }
}

pub(super) fn update_standings_screen(
    players: Query<(&Player, &Points)>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    list: Single<Entity, With<StandingsList>>,
    mut shown: Local<ShownRows>,
    mut commands: Commands,
) {
    let you = local_peer_id(&clients);
    let rows = standings(
        players
            .iter()
            .map(|(player, points)| (player.clone(), *points)),
    )
    .iter()
    .map(|standing| (standing_label(standing, you), standing.player.color))
    .collect();
    show_rows(*list, rows, &mut shown, &mut commands);
}

/// A standings row, marking `you`: "1st Ada (you): 4 points (+1)", "2nd Bo: 1 point".
pub fn standing_label(standing: &Standing, you: Option<PeerId>) -> String {
    let Standing {
        place,
        player,
        points,
    } = standing;
    let mut line = format!("{} {}", ordinal(*place), player.name());
    if Some(player.id) == you {
        line.push_str(" (you)");
    }
    let unit = if points.total == 1 { "point" } else { "points" };
    line.push_str(&format!(": {} {unit}", points.total));
    if points.gained > 0 {
        line.push_str(&format!(" (+{})", points.gained));
    }
    line
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::PeerId;

    use super::{standing_label, standings_captions};
    use crate::party::Party;
    use crate::players::Player;
    use crate::standings::{Points, Standing};

    fn standing(total: u32, gained: u32) -> Standing {
        Standing {
            place: 1,
            player: Player {
                id: PeerId::Netcode(1),
                number: 1,
                name: "Ada".into(),
                color: 0,
            },
            points: Points { total, gained },
        }
    }

    #[test]
    fn in_a_party_says_how_far_it_is() {
        assert_eq!(
            standings_captions(None),
            (String::new(), "Back to the lobby in")
        );
        let mut party = Party::new(3);
        party.played = 1;
        assert_eq!(
            standings_captions(Some(&party)),
            ("After round 1 of 3".into(), "Next round in")
        );
        party.played = 3;
        assert_eq!(
            standings_captions(Some(&party)),
            ("After the final round".into(), "The winner in")
        );
    }

    #[test]
    fn shows_points_and_what_the_last_game_gave() {
        assert_eq!(
            standing_label(&standing(4, 1), None),
            "1st Ada: 4 points (+1)"
        );
        assert_eq!(standing_label(&standing(1, 0), None), "1st Ada: 1 point");
        assert_eq!(
            standing_label(&standing(0, 0), Some(PeerId::Netcode(1))),
            "1st Ada (you): 0 points"
        );
    }
}
