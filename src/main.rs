use bevy::prelude::*;
use smocka::config::network::SERVER_ADDR;
use smocka::network::{HostSession, JoinSession};
use smocka::{SmockaPlugin, camera, minigames};

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(ImagePlugin::default_nearest()),
        camera::CameraPlugin,
        minigames::MinigameVisualsPlugin,
        SmockaPlugin,
    ));

    // `cargo run` hosts a session; `cargo run -- join` connects to one.
    let joining = std::env::args().nth(1).as_deref() == Some("join");
    app.add_systems(Startup, move |mut commands: Commands| {
        if joining {
            commands.queue(JoinSession { addr: SERVER_ADDR });
        } else {
            commands.queue(HostSession { addr: SERVER_ADDR });
        }
    });

    app.run();
}
