// Release builds on Windows run without a console window next to the game. Debug builds keep it,
// for the logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bevy::audio::Volume;
use bevy::ecs::error::warn;
use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryMonitor, PrimaryWindow, WindowPosition};
use smocka::config::network::{HOST_ADDR, SERVER_ADDR};
use smocka::menu::MenuPlugin;
use smocka::network::{HostSession, JoinSession, parse_address};
use smocka::profile::{LastAddress, LocalProfile, clean_name};
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

/// Where to put the window, to test two players side by side: the left or right half
/// (`--left` / `--right`) of the primary monitor, or of another one with `--secondary`. Removed
/// once the window is placed.
///
/// Only works where apps may place their own windows: X11, Windows and macOS, but not Wayland
/// (`tools/dev.sh` runs the game on X11 for this).
#[derive(Resource, Debug, Clone, Copy)]
struct ScreenHalf {
    side: Side,
    /// Use the first monitor (from the left) that isn't the primary one, if there is one.
    secondary: bool,
}

#[derive(Debug, Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// Physical pixels to leave free below the window, for its title bar and a desktop panel.
const SCREEN_HALF_MARGIN: u32 = 100;

fn main() {
    let (flags, positional): (Vec<String>, Vec<String>) = std::env::args()
        .skip(1)
        .partition(|arg| arg.starts_with("--"));
    let has = |name: &str| flags.iter().any(|flag| flag == name);
    let side = if has("--left") {
        Some(Side::Left)
    } else if has("--right") {
        Some(Side::Right)
    } else {
        None
    };
    let half = side.map(|side| ScreenHalf {
        side,
        secondary: has("--secondary"),
    });
    // `--mute` plays without sound, so two copies on one machine (`tools/dev.sh`) aren't heard twice.
    let mute = has("--mute");
    // `--name=Ada` plays as "Ada" this time, without changing the saved name.
    let name = flags
        .iter()
        .find_map(|flag| flag.strip_prefix("--name="))
        .map(clean_name);
    let mut args = positional.into_iter();
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
        // Smooth filtering for the 1080p art (see `config::display::ART_SCALE`).
        DefaultPlugins.set(ImagePlugin::default_linear()),
        camera::CameraPlugin,
        minigames::MinigameVisualsPlugin,
        MenuPlugin,
        SmockaPlugin,
    ));

    if mute {
        info!("Sound is off (--mute)");
        app.insert_resource(GlobalVolume::new(Volume::SILENT));
    }
    if let Some(name) = name {
        app.insert_resource(LocalProfile { name });
    }
    // If the join fails, the main menu offers the same address again, without saving it.
    if let Launch::Join(addr) = launch {
        app.insert_resource(LastAddress(addr.to_string()));
    }
    if let Some(half) = half {
        app.insert_resource(half);
        app.add_systems(Update, place_window.run_if(resource_exists::<ScreenHalf>));
    }

    app.add_systems(Startup, move |mut commands: Commands| match launch {
        Launch::Menu => {}
        // A failure stays in the main menu, which shows why.
        Launch::Host => commands.queue_handled(HostSession { addr: HOST_ADDR }, warn),
        Launch::Join(addr) => commands.queue_handled(JoinSession { addr }, warn),
    });

    app.run();
}

/// Size and move the window to its [`ScreenHalf`]. Monitors are only known a frame or so after
/// startup, so this waits for them. With `--secondary` but only one monitor, it uses that one.
fn place_window(
    half: Res<ScreenHalf>,
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let mut monitors: Vec<_> = monitors.iter().collect();
    monitors.sort_by_key(|(monitor, _)| (monitor.physical_position.x, monitor.physical_position.y));
    let primary = monitors.iter().find(|(_, primary)| *primary);
    let other = monitors.iter().find(|(_, primary)| !*primary);
    let chosen = if half.secondary {
        other.or(primary)
    } else {
        primary
    };
    let Some((monitor, _)) = chosen else {
        return;
    };
    if half.secondary && other.is_none() {
        info!("Only one monitor: placing the window on it instead of a secondary one");
    }
    let width = monitor.physical_width / 2;
    let height = monitor.physical_height.saturating_sub(SCREEN_HALF_MARGIN);
    let x = match half.side {
        Side::Left => 0,
        Side::Right => width as i32,
    };
    let position = monitor.physical_position + IVec2::new(x, 0);
    window.resolution.set_physical_resolution(width, height);
    window.position = WindowPosition::At(position);
    debug!(
        "Placed the window on the {:?} half of {}: {width}x{height} at {position}",
        half.side,
        monitor.name.as_deref().unwrap_or("a monitor")
    );
    commands.remove_resource::<ScreenHalf>();
}
