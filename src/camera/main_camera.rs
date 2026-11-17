use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

/// Marker component for the main game camera.
#[derive(Component)]
pub struct MainCamera;

/// Spawn the main 2D camera.
///
/// The camera always shows at least [`VIEW_WIDTH`] × [`VIEW_HEIGHT`] world units, whatever the
/// window size. If the window has a different aspect ratio, it shows extra space at the edges
/// instead of stretching the art.
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
