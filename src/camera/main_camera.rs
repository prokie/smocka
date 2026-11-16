use bevy::prelude::*;

/// Marker component for the main game camera.
#[derive(Component)]
pub struct MainCamera;

/// Spawn the main 2D camera.
pub fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera2d, MainCamera));
}
