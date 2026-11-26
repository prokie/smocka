use bevy::prelude::*;
use lightyear::prelude::input::native::InputMarker;

use super::RacingInput;
use super::car::{Car, CarState};
use super::progress::{RaceProgress, lap_label, position_label, running_order};
use super::race::{RaceStatus, status_label};
use super::track::{CHECKPOINTS, outline};
use crate::config::minigames::racing::{START_LINE_X, TRACK_INNER_RADIUS, TRACK_OUTER_RADIUS};
use crate::lobby::player_color;
use crate::session::SessionPhase;

/// Points right. Drawn in white/grey and tinted per player.
const CAR_IMAGE: &str = "minigames/racing/car.png";

/// 640×360, drawn to match the shape in [`track`](super::track).
const TRACK_IMAGE: &str = "minigames/racing/track.png";

/// Draw order: higher values are drawn on top.
const TRACK_Z: f32 = 0.0;
const CAR_Z: f32 = 1.0;

/// Toggles [`TrackGizmos`].
const TRACK_GIZMOS_KEY: KeyCode = KeyCode::F3;
const ROAD_EDGE_GIZMO_COLOR: Color = Color::srgb(1.0, 0.9, 0.2);
const START_LINE_GIZMO_COLOR: Color = Color::srgb(1.0, 0.3, 1.0);
const CHECKPOINT_GIZMO_COLOR: Color = Color::srgb(0.3, 0.9, 1.0);

/// Lap counter in the top-left corner, in screen pixels.
const LAP_TEXT_SIZE: f32 = 28.0;
const LAP_TEXT_MARGIN: f32 = 16.0;
/// Countdown and "GO!" in the middle of the screen, in screen pixels.
const STATUS_TEXT_SIZE: f32 = 72.0;
/// How long "GO!" stays up after the start, in seconds.
const GO_SECONDS: f32 = 1.0;

#[derive(Component)]
pub struct TrackSprite;

/// Shows the lap this player's car is on.
#[derive(Component)]
pub struct LapText;

/// Shows where this player's car is in the running order, e.g. "2nd / 4".
#[derive(Component)]
pub struct PositionText;

/// Shows the countdown, "GO!", and how long until the race ends.
#[derive(Component)]
pub struct StatusText;

/// Whether to draw the track's road edges and start line over the art, to check the two line up.
#[derive(Resource, Debug, Default)]
pub struct TrackGizmos(pub bool);

/// Local-only sprites for racing.
pub struct RacingVisualsPlugin;

impl Plugin for RacingVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TrackGizmos>();
        app.add_systems(
            OnEnter(SessionPhase::Racing),
            (
                spawn_track,
                spawn_lap_text,
                spawn_position_text,
                spawn_status_text,
            ),
        );
        app.add_observer(add_car_sprite);
        app.add_systems(
            Update,
            (
                toggle_track_gizmos,
                draw_track_gizmos.run_if(|show: Res<TrackGizmos>| show.0),
                update_lap_text,
                update_position_text,
                update_status_text,
            )
                .chain()
                .run_if(in_state(SessionPhase::Racing)),
        );
        app.add_systems(
            PostUpdate,
            sync_car_transform.before(TransformSystems::Propagate),
        );
    }
}

fn spawn_track(assets: Res<AssetServer>, mut commands: Commands) {
    commands.spawn((
        TrackSprite,
        DespawnOnExit(SessionPhase::Racing),
        Sprite::from_image(assets.load(TRACK_IMAGE)),
        Transform::from_xyz(0.0, 0.0, TRACK_Z),
    ));
}

fn spawn_lap_text(mut commands: Commands) {
    commands.spawn((
        LapText,
        DespawnOnExit(SessionPhase::Racing),
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

/// Show the lap of this player's own car: the one that takes this machine's input.
fn update_lap_text(
    own_car: Query<&RaceProgress, With<InputMarker<RacingInput>>>,
    mut text: Single<&mut Text, With<LapText>>,
) {
    let shown = own_car
        .single()
        .map(|progress| lap_label(*progress))
        .unwrap_or_default();
    if text.0 != shown {
        text.0 = shown;
    }
}

fn spawn_position_text(mut commands: Commands) {
    commands.spawn((
        PositionText,
        DespawnOnExit(SessionPhase::Racing),
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
/// once the car has finished (the lap counter shows its place) or if this player has no car.
fn update_position_text(
    cars: Query<CarStandings, With<Car>>,
    mut text: Single<&mut Text, With<PositionText>>,
) {
    let own = cars
        .iter()
        .find(|(_, _, own)| *own)
        .filter(|(progress, _, _)| progress.place.is_none())
        .map(|(progress, state, _)| running_order(*progress, state.position));
    let shown = match own {
        Some(own) => {
            let ahead = cars
                .iter()
                .filter(|(progress, state, _)| running_order(**progress, state.position) < own)
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
        DespawnOnExit(SessionPhase::Racing),
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

/// Show the host's [`RaceStatus`]. "GO!" is timed here, from when this app saw the race start.
fn update_status_text(
    time: Res<Time>,
    status: Option<Single<&RaceStatus>>,
    mut go: Local<Option<Timer>>,
    mut text: Single<&mut Text, With<StatusText>>,
) {
    let shown = match status {
        Some(status) => {
            if let RaceStatus::Countdown(_) = **status {
                *go = Some(Timer::from_seconds(GO_SECONDS, TimerMode::Once));
            } else if let Some(go) = go.as_mut() {
                go.tick(time.delta());
            }
            let just_started = go.as_ref().is_some_and(|go| !go.is_finished());
            status_label(**status, just_started)
        }
        None => String::new(),
    };
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

fn draw_track_gizmos(mut gizmos: Gizmos) {
    for radius in [TRACK_INNER_RADIUS, TRACK_OUTER_RADIUS] {
        gizmos.linestrip_2d(outline(radius, 32), ROAD_EDGE_GIZMO_COLOR);
    }
    gizmos.line_2d(
        Vec2::new(START_LINE_X, -TRACK_INNER_RADIUS),
        Vec2::new(START_LINE_X, -TRACK_OUTER_RADIUS),
        START_LINE_GIZMO_COLOR,
    );
    for checkpoint in CHECKPOINTS {
        gizmos.rect_2d(
            Isometry2d::from_translation(checkpoint.center()),
            checkpoint.size(),
            CHECKPOINT_GIZMO_COLOR,
        );
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
            color: player_color(car.number),
            ..Sprite::from_image(assets.load(CAR_IMAGE))
        },
        Transform::from_xyz(0.0, 0.0, CAR_Z),
    ));
}

fn sync_car_transform(mut cars: Query<(&CarState, &mut Transform), With<Car>>) {
    for (state, mut transform) in &mut cars {
        transform.translation.x = state.position.x;
        transform.translation.y = state.position.y;
        transform.rotation = Quat::from_rotation_z(state.heading);
    }
}
