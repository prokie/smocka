mod camera;
mod config;
mod network;

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, camera::CameraPlugin, network::NetworkPlugin))
        .run();
}
