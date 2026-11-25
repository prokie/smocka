use bevy::prelude::*;

use super::car::{Car, CarState};
use super::track::outline;
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

#[derive(Component)]
pub struct TrackSprite;

/// Whether to draw the track's road edges and start line over the art, to check the two line up.
#[derive(Resource, Debug, Default)]
pub struct TrackGizmos(pub bool);

/// Local-only sprites for racing.
pub struct RacingVisualsPlugin;

impl Plugin for RacingVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TrackGizmos>();
        app.add_systems(OnEnter(SessionPhase::Racing), spawn_track);
        app.add_observer(add_car_sprite);
        app.add_systems(
            Update,
            (
                toggle_track_gizmos,
                draw_track_gizmos.run_if(|show: Res<TrackGizmos>| show.0),
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
