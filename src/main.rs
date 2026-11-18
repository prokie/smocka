use bevy::prelude::*;
use smocka::config::network::{HOST_ADDR, SERVER_ADDR};
use smocka::menu::{MenuPlugin, parse_address};
use smocka::network::{HostSession, JoinSession};
use smocka::{SmockaPlugin, camera, minigames};

/// What to do at startup, from the command line.
enum Launch {
    /// `cargo run`: show the main menu.
    Menu,
    /// `cargo run -- host`: host straight away.
    Host,
    /// `cargo run -- join [address]`: join straight away (default: a host on this machine).
    Join(std::net::SocketAddr),
}

fn main() {
    let mut args = std::env::args().skip(1);
    let launch = match args.next().as_deref() {
        Some("host") => Launch::Host,
        Some("join") => match args.next() {
            Some(typed) => match parse_address(&typed) {
                Some(addr) => Launch::Join(addr),
                None => {
                    eprintln!("Not an address: {typed}");
                    std::process::exit(2);
                }
            },
            None => Launch::Join(SERVER_ADDR),
        },
        _ => Launch::Menu,
    };

    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(ImagePlugin::default_nearest()),
        camera::CameraPlugin,
        minigames::MinigameVisualsPlugin,
        MenuPlugin,
        SmockaPlugin,
    ));

    app.add_systems(Startup, move |mut commands: Commands| match launch {
        Launch::Menu => {}
        Launch::Host => commands.queue(HostSession { addr: HOST_ADDR }),
        Launch::Join(addr) => commands.queue(JoinSession { addr }),
    });

    app.run();
}
