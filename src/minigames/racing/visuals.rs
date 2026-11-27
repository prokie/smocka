use bevy::prelude::*;
use lightyear::prelude::input::native::InputMarker;

use super::LAPS_SETTING;
use super::PHASE;
use super::RacingInput;
use super::boost_visuals::BoostVisualsPlugin;
use super::camera::RacingCameraPlugin;
use super::car::{Car, CarState};
use super::checkpoint_guide::CheckpointGuidePlugin;
use super::config::{CAR_LENGTH, CAR_WIDTH};
use super::minimap::MinimapPlugin;
use super::progress::{RaceProgress, lap_label, position_label, running_order};
use super::race::{RaceStatus, status_label};
use super::skid_marks::SkidMarksPlugin;
use super::sounds::RacingSoundsPlugin;
use super::track::TrackId;
use crate::minigames::settings::PublishedSettings;
use crate::minigames::smoothing::{Smoothed, remember_previous_tick, smoothed};
use crate::players::player_color;

/// Points right. Drawn in white/grey and tinted per player.
const CAR_IMAGE: &str = "minigames/racing/car.png";

/// Draw order: higher values are drawn on top.
const TRACK_Z: f32 = 0.0;
const CAR_Z: f32 = 1.0;

/// Toggles [`TrackGizmos`].
const TRACK_GIZMOS_KEY: KeyCode = KeyCode::F3;
const ROAD_EDGE_GIZMO_COLOR: Color = Color::srgb(1.0, 0.9, 0.2);
const START_LINE_GIZMO_COLOR: Color = Color::srgb(1.0, 0.3, 1.0);
const CHECKPOINT_GIZMO_COLOR: Color = Color::srgb(0.3, 0.9, 1.0);
const BOOST_PAD_GIZMO_COLOR: Color = Color::srgb(1.0, 0.5, 0.1);

/// Lap counter in the top-left corner, in screen pixels.
const LAP_TEXT_SIZE: f32 = 28.0;
const LAP_TEXT_MARGIN: f32 = 16.0;
/// How long the race has left, in the middle of the screen, in screen pixels.
const STATUS_TEXT_SIZE: f32 = 72.0;

/// The race's track art, sized to [`Layout::size`](super::track::Layout::size) (see
/// [`Layout::image`](super::track::Layout::image)).
#[derive(Component)]
pub struct TrackSprite;

/// Shows the lap this player's car is on.
#[derive(Component)]
pub struct LapText;

/// Shows where this player's car is in the running order, e.g. "2nd / 4".
#[derive(Component)]
pub struct PositionText;

/// Shows how long until the race ends, once someone has finished.
#[derive(Component)]
pub struct StatusText;

/// Whether to draw the track's road edges, start line, checkpoints and boost pads over the art, to
/// check the two line up.
#[derive(Resource, Debug, Default)]
pub struct TrackGizmos(pub bool);

/// Local-only sprites, text and sounds for racing.
pub struct RacingVisualsPlugin;

impl Plugin for RacingVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            RacingCameraPlugin,
            MinimapPlugin,
            SkidMarksPlugin,
            RacingSoundsPlugin,
            CheckpointGuidePlugin,
            BoostVisualsPlugin,
        ));
        app.init_resource::<TrackGizmos>();
        app.add_systems(
            OnEnter(PHASE),
            (spawn_lap_text, spawn_position_text, spawn_status_text),
        );
        app.add_observer(add_car_sprite);
        app.add_systems(
            Update,
            (
                spawn_track,
                toggle_track_gizmos,
                draw_track_gizmos.run_if(|show: Res<TrackGizmos>| show.0),
                update_lap_text,
                update_position_text,
                update_status_text,
            )
                .chain()
                .run_if(in_state(PHASE)),
        );
        app.add_systems(
            FixedFirst,
            remember_previous_tick::<CarState>.run_if(in_state(PHASE)),
        );
        app.add_systems(
            PostUpdate,
            sync_car_transform
                .before(TransformSystems::Propagate)
                .run_if(in_state(PHASE)),
        );
    }
}

/// Draw the race's track once its [`TrackId`] is known: at once on the host, and on a joiner when
/// the race's entity arrives.
fn spawn_track(
    track: Option<Single<&TrackId>>,
    drawn: Query<(), With<TrackSprite>>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Some(track) = track.map(|track| **track) else {
        return;
    };
    if !drawn.is_empty() {
        return;
    }
    let layout = track.layout();
    debug!("Drawing the {} track", track.name());
    commands.spawn((
        TrackSprite,
        DespawnOnExit(PHASE),
        Sprite {
            custom_size: Some(layout.size),
            ..Sprite::from_image(assets.load(layout.image))
        },
        Transform::from_xyz(0.0, 0.0, TRACK_Z),
    ));
}

fn spawn_lap_text(mut commands: Commands) {
    commands.spawn((
        LapText,
        DespawnOnExit(PHASE),
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(LAP_TEXT_SIZE),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: px(LAP_TEXT_MARGIN),
            left: px(LAP_TEXT_MARGIN),
            ..default()
        },
    ));
}

/// Show the lap of this player's own car: the one that takes this machine's input. Empty until
/// the host's [`PublishedSettings`] (with the race's laps) arrive.
fn update_lap_text(
    own_car: Query<&RaceProgress, With<InputMarker<RacingInput>>>,
    settings: Option<Single<&PublishedSettings>>,
    mut text: Single<&mut Text, With<LapText>>,
) {
    let shown = match (own_car.single(), settings) {
        (Ok(progress), Some(settings)) => lap_label(*progress, settings.get(LAPS_SETTING)),
        _ => String::new(),
    };
    if text.0 != shown {
        // On every peer, so a joiner's log shows what it saw of its own car's laps.
        if !shown.is_empty() {
            debug!("Your lap counter: {shown}");
        }
        text.0 = shown;
    }
}

fn spawn_position_text(mut commands: Commands) {
    commands.spawn((
        PositionText,
        DespawnOnExit(PHASE),
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(LAP_TEXT_SIZE),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: px(LAP_TEXT_MARGIN),
            right: px(LAP_TEXT_MARGIN),
            ..default()
        },
    ));
}

/// Each car's progress and state, and whether it's this player's own.
type CarStandings = (
    &'static RaceProgress,
    &'static CarState,
    Has<InputMarker<RacingInput>>,
);

/// Show where this player's car is in the running order. Worked out here from the replicated
/// progress and the positions this app sees, so it can be a moment off for close cars. Empty
/// once the car has finished (the lap counter shows its place), if this player has no car, or
/// until the race's track is known.
fn update_position_text(
    track: Option<Single<&TrackId>>,
    cars: Query<CarStandings, With<Car>>,
    mut text: Single<&mut Text, With<PositionText>>,
) {
    let track = track.map(|id| id.track());
    let own = track.and_then(|track| {
        cars.iter()
            .find(|(_, _, own)| *own)
            .filter(|(progress, _, _)| progress.place.is_none())
            .map(|(progress, state, _)| (track, running_order(track, *progress, state.position)))
    });
    let shown = match own {
        Some((track, own)) => {
            let ahead = cars
                .iter()
                .filter(|(progress, state, _)| {
                    running_order(track, **progress, state.position) < own
                })
                .count();
            position_label(ahead + 1, cars.iter().len())
        }
        None => String::new(),
    };
    if text.0 != shown {
        text.0 = shown;
    }
}

fn spawn_status_text(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(PHASE),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        children![(
            StatusText,
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(STATUS_TEXT_SIZE),
                ..default()
            },
            TextColor(Color::WHITE),
        )],
    ));
}

/// Show the host's [`RaceStatus`]: how long the race has left once someone has finished.
fn update_status_text(
    status: Option<Single<&RaceStatus>>,
    mut text: Single<&mut Text, With<StatusText>>,
) {
    let shown = status
        .map(|status| status_label(**status))
        .unwrap_or_default();
    if text.0 != shown {
        text.0 = shown;
    }
}

fn toggle_track_gizmos(keys: Res<ButtonInput<KeyCode>>, mut show: ResMut<TrackGizmos>) {
    if keys.just_pressed(TRACK_GIZMOS_KEY) {
        show.0 = !show.0;
        debug!("Track gizmos {}", if show.0 { "on" } else { "off" });
    }
}

fn draw_track_gizmos(track: Option<Single<&TrackId>>, mut gizmos: Gizmos) {
    let Some(track) = track.map(|id| id.track()) else {
        return;
    };
    let layout = track.layout();
    for offset in [-layout.road_half_width, layout.road_half_width] {
        gizmos.linestrip_2d(track.outline(offset, 4.0), ROAD_EDGE_GIZMO_COLOR);
    }
    let across = Vec2::from_angle(layout.start_heading).perp() * layout.road_half_width;
    gizmos.line_2d(
        layout.start_line - across,
        layout.start_line + across,
        START_LINE_GIZMO_COLOR,
    );
    for checkpoint in track.checkpoints() {
        gizmos.rect_2d(
            checkpoint.isometry(),
            checkpoint.size(),
            CHECKPOINT_GIZMO_COLOR,
        );
    }
    for pad in track.boost_pads() {
        gizmos.rect_2d(pad.isometry(), pad.size(), BOOST_PAD_GIZMO_COLOR);
    }
}

fn add_car_sprite(
    trigger: On<Add<Car>>,
    cars: Query<&Car>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Ok(car) = cars.get(trigger.entity) else {
        return;
    };
    commands.entity(trigger.entity).insert((
        Sprite {
            color: player_color(car.color),
            custom_size: Some(Vec2::new(CAR_LENGTH, CAR_WIDTH)),
            ..Sprite::from_image(assets.load(CAR_IMAGE))
        },
        Transform::from_xyz(0.0, 0.0, CAR_Z),
    ));
}

/// Draw each car between its last two ticks (see [`smoothing`](crate::minigames::smoothing)).
pub(super) fn sync_car_transform(
    time: Res<Time<Fixed>>,
    mut cars: Query<Smoothed<CarState>, With<Car>>,
) {
    for (state, previous, mut transform) in &mut cars {
        let shown = smoothed(state, previous, time.overstep_fraction());
        transform.translation.x = shown.position.x;
        transform.translation.y = shown.position.y;
        transform.rotation = Quat::from_rotation_z(shown.heading);
    }
}
