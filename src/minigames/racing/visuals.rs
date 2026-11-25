use bevy::prelude::*;

use super::car::{Car, CarState};
use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};
use crate::lobby::player_color;
use crate::session::SessionPhase;

/// Points right. Drawn in white/grey and tinted per player.
const CAR_IMAGE: &str = "minigames/racing/car.png";

/// Until there is track art.
const GROUND_COLOR: Color = Color::srgb(0.2, 0.42, 0.22);

/// Draw order: higher values are drawn on top.
const GROUND_Z: f32 = 0.0;
const CAR_Z: f32 = 1.0;

#[derive(Component)]
pub struct Ground;

/// Local-only sprites for racing.
pub struct RacingVisualsPlugin;

impl Plugin for RacingVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(SessionPhase::Racing), spawn_ground);
        app.add_observer(add_car_sprite);
        app.add_systems(
            PostUpdate,
            sync_car_transform.before(TransformSystems::Propagate),
        );
    }
}

fn spawn_ground(mut commands: Commands) {
    commands.spawn((
        Ground,
        DespawnOnExit(SessionPhase::Racing),
        Sprite::from_color(GROUND_COLOR, Vec2::new(VIEW_WIDTH, VIEW_HEIGHT)),
        Transform::from_xyz(0.0, 0.0, GROUND_Z),
    ));
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
