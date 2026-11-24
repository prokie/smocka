//! Local-only rendering for racing: the ground and a sprite for each car.
//!
//! None of this is replicated. Each peer attaches sprites to the replicated cars it sees. The art
//! lives in `assets/minigames/racing/`. Overwrite those PNGs to change how the game looks: no code
//! changes are needed as long as the file names stay the same.

use bevy::prelude::*;

use super::car::{Car, CarState};
use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};
use crate::lobby::player_color;
use crate::session::SessionPhase;

/// Car image (16×10), pointing right. Drawn in white/grey and tinted per player.
const CAR_IMAGE: &str = "minigames/racing/car.png";

/// Colour of the ground the cars drive on, until there is track art.
const GROUND_COLOR: Color = Color::srgb(0.2, 0.42, 0.22);

/// Draw order: higher values are drawn on top.
const GROUND_Z: f32 = 0.0;
const CAR_Z: f32 = 1.0;

/// Marker for the local ground sprite.
#[derive(Component)]
pub struct Ground;

/// Adds sprites to racing entities. Needs a renderer and an [`AssetServer`], so it is added by the
/// binary (through [`MinigameVisualsPlugin`](crate::minigames::MinigameVisualsPlugin)), not by
/// headless tests.
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

/// When the race starts, cover the screen with plain ground. It is local to each peer and removed
/// when the race ends.
fn spawn_ground(mut commands: Commands) {
    commands.spawn((
        Ground,
        DespawnOnExit(SessionPhase::Racing),
        Sprite::from_color(GROUND_COLOR, Vec2::new(VIEW_WIDTH, VIEW_HEIGHT)),
        Transform::from_xyz(0.0, 0.0, GROUND_Z),
    ));
}

/// When a car appears, give it a sprite in its driver's colour.
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

/// Move and turn each car sprite to match its (predicted or interpolated) [`CarState`].
fn sync_car_transform(mut cars: Query<(&CarState, &mut Transform), With<Car>>) {
    for (state, mut transform) in &mut cars {
        transform.translation.x = state.position.x;
        transform.translation.y = state.position.y;
        transform.rotation = Quat::from_rotation_z(state.heading);
    }
}
