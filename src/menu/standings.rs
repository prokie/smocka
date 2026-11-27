//! The standings screen after each result, and the hold-`Tab` (or gamepad Back) standings overlay
//! during a minigame, tournament wait, or results screen.

use bevy::ecs::system::SystemParam;
use bevy::picking::Pickable;
use bevy::prelude::*;
use lightyear::prelude::*;

use super::widgets::{HINT_COLOR, ShownRows, countdown, label, screen, show_rows};
use crate::app_state::AppState;
use crate::config::session::STANDINGS_DURATION;
use crate::minigames::outcome::ordinal;
use crate::party::Party;
use crate::players::{Player, local_peer_id};
use crate::session::SessionPhase;
use crate::standings::{Points, Standing, standings};

/// Translucent card behind the hold-`Tab` standings overlay, so it reads over any minigame.
const OVERLAY_BACKGROUND: Color = Color::srgba(0.08, 0.08, 0.11, 0.9);

/// One row per player: place, name, points.
#[derive(Component)]
pub struct StandingsList;

/// The hold-`Tab` standings overlay shown over non-interactive session screens.
#[derive(Component)]
pub struct StandingsOverlay;

/// The party-round line under "Standings" inside [`StandingsOverlay`].
#[derive(Component)]
pub struct StandingsOverlayCaption;

/// The player rows inside [`StandingsOverlay`].
#[derive(Component)]
pub struct StandingsOverlayList;

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
    format_standing(standing, you, true)
}

/// Like [`standing_label`], without the last-game `(+N)` gain, for the mid-game hold-`Tab`
/// overlay where a gain from an earlier round would look stale.
pub fn overlay_standing_label(standing: &Standing, you: Option<PeerId>) -> String {
    format_standing(standing, you, false)
}

fn format_standing(standing: &Standing, you: Option<PeerId>, show_gained: bool) -> String {
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
    if show_gained && points.gained > 0 {
        line.push_str(&format!(" (+{})", points.gained));
    }
    line
}

/// Whether holding `Tab` opens [`StandingsOverlay`] in `phase`. Off in the lobby (where `Tab`
/// moves between buttons) and on screens that already show the standings.
pub fn overlay_allowed(phase: SessionPhase) -> bool {
    matches!(
        phase,
        SessionPhase::Playing(_) | SessionPhase::NextMatch | SessionPhase::Results
    )
}

/// The line under "Standings" on [`StandingsOverlay`]: which round of a party is being played, or
/// empty outside a party.
pub fn overlay_caption(party: Option<&Party>) -> String {
    match party {
        None => String::new(),
        Some(party) if party.round() >= party.rounds => "Final round".into(),
        Some(party) => format!("Round {} of {}", party.round(), party.rounds),
    }
}

/// Spawns [`StandingsOverlay`] hidden for the whole session; [`update_standings_overlay`] shows
/// it while `Tab` (or gamepad Back) is held.
pub(super) fn spawn_standings_overlay(mut commands: Commands) {
    commands.spawn((
        StandingsOverlay,
        DespawnOnExit(AppState::InSession),
        GlobalZIndex(2),
        Pickable::IGNORE,
        Visibility::Hidden,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        children![(
            Pickable::IGNORE,
            BackgroundColor(OVERLAY_BACKGROUND),
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10),
                padding: UiRect::axes(px(32), px(24)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            children![
                label("Standings", 36.0, Color::WHITE),
                (
                    StandingsOverlayCaption,
                    label("", 18.0, HINT_COLOR),
                    Node {
                        display: Display::None,
                        ..default()
                    },
                ),
                (
                    StandingsOverlayList,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    },
                ),
            ],
        )],
    ));
}

/// UI queries used by [`update_standings_overlay`].
#[derive(SystemParam)]
pub(super) struct OverlayUi<'w, 's> {
    overlay: Single<'w, 's, &'static mut Visibility, With<StandingsOverlay>>,
    caption: Single<'w, 's, (&'static mut Text, &'static mut Node), With<StandingsOverlayCaption>>,
    list: Single<'w, 's, Entity, With<StandingsOverlayList>>,
}

/// Show [`StandingsOverlay`] while `Tab` (or gamepad Back) is held during a minigame, tournament
/// wait, or results screen, and keep its rows up to date while visible.
pub(super) fn update_standings_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    phase: Option<Res<State<SessionPhase>>>,
    party: Option<Single<&Party>>,
    players: Query<(&Player, &Points)>,
    clients: Query<&LocalId, (With<Client>, With<Connected>)>,
    (mut ui, mut shown, mut commands): (OverlayUi, Local<ShownRows>, Commands),
) {
    let held = keys.pressed(KeyCode::Tab)
        || gamepads
            .iter()
            .any(|gamepad| gamepad.pressed(GamepadButton::Select));
    let visible = held && phase.is_some_and(|phase| overlay_allowed(*phase.get()));
    ui.overlay.set_if_neq(if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !visible {
        return;
    }

    let caption = overlay_caption(party.as_deref().copied());
    let (ref mut caption_text, ref mut caption_node) = *ui.caption;
    if caption_text.0 != caption {
        caption_text.0.clone_from(&caption);
    }
    let display = if caption.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if caption_node.display != display {
        caption_node.display = display;
    }

    let you = local_peer_id(&clients);
    let rows = standings(
        players
            .iter()
            .map(|(player, points)| (player.clone(), *points)),
    )
    .iter()
    .map(|standing| (overlay_standing_label(standing, you), standing.player.color))
    .collect();
    show_rows(*ui.list, rows, &mut shown, &mut commands);
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use lightyear::prelude::PeerId;

    use super::{
        StandingsOverlay, StandingsOverlayList, overlay_allowed, overlay_caption,
        overlay_standing_label, spawn_standings_overlay, standing_label, standings_captions,
        update_standings_overlay,
    };
    use crate::minigames::Minigame;
    use crate::party::Party;
    use crate::players::Player;
    use crate::session::SessionPhase;
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
        // The hold-Tab overlay leaves off the previous round's (+N) gain.
        assert_eq!(
            overlay_standing_label(&standing(4, 1), Some(PeerId::Netcode(1))),
            "1st Ada (you): 4 points"
        );
    }

    #[test]
    fn overlay_only_opens_where_tab_is_free_and_standings_arent_already_shown() {
        assert!(overlay_allowed(SessionPhase::Playing(Minigame::Shootout)));
        assert!(overlay_allowed(SessionPhase::NextMatch));
        assert!(overlay_allowed(SessionPhase::Results));
        assert!(!overlay_allowed(SessionPhase::Lobby));
        assert!(!overlay_allowed(SessionPhase::Standings));
        assert!(!overlay_allowed(SessionPhase::NextRound));
        assert!(!overlay_allowed(SessionPhase::PartyOver));
    }

    #[test]
    fn overlay_caption_names_the_party_round() {
        assert_eq!(overlay_caption(None), "");
        let mut party = Party::new(4);
        assert_eq!(overlay_caption(Some(&party)), "Round 1 of 4");
        party.played = 3;
        assert_eq!(overlay_caption(Some(&party)), "Final round");
    }

    #[test]
    fn holding_tab_shows_the_overlay_in_a_game_and_hides_it_on_release_or_in_the_lobby() {
        let mut world = World::new();
        world.init_resource::<ButtonInput<KeyCode>>();
        world.insert_resource(State::new(SessionPhase::Playing(Minigame::Shootout)));
        world.spawn((
            Player {
                id: PeerId::Netcode(1),
                number: 1,
                name: "Ada".into(),
                color: 0,
            },
            Points {
                total: 3,
                gained: 1,
            },
        ));
        world.run_system_once(spawn_standings_overlay).unwrap();

        let visibility = |world: &mut World| {
            *world
                .query_filtered::<&Visibility, With<StandingsOverlay>>()
                .single(world)
                .unwrap()
        };

        // Hidden until Tab is held.
        world.run_system_once(update_standings_overlay).unwrap();
        assert_eq!(visibility(&mut world), Visibility::Hidden);

        // Holding Tab during a minigame shows the overlay and fills its rows.
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        world.run_system_once(update_standings_overlay).unwrap();
        assert_eq!(visibility(&mut world), Visibility::Inherited);
        let list = world
            .query_filtered::<Entity, With<StandingsOverlayList>>()
            .single(&world)
            .unwrap();
        assert_eq!(world.get::<Children>(list).map_or(0, Children::len), 1);

        // In the lobby, Tab stays keyboard navigation and doesn't open the overlay.
        world.insert_resource(State::new(SessionPhase::Lobby));
        world.run_system_once(update_standings_overlay).unwrap();
        assert_eq!(visibility(&mut world), Visibility::Hidden);

        // Releasing Tab in a minigame hides it again.
        world.insert_resource(State::new(SessionPhase::Playing(Minigame::Shootout)));
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::Tab);
        world.run_system_once(update_standings_overlay).unwrap();
        assert_eq!(visibility(&mut world), Visibility::Hidden);
    }
}
