use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

#[derive(Component)]
struct MainCamera;

/// Always shows at least [`VIEW_WIDTH`] × [`VIEW_HEIGHT`]; other aspect ratios get extra space at
/// the edges instead of stretched art.
pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        MainCamera,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: VIEW_WIDTH,
                min_height: VIEW_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
}
