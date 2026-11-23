mod client;
mod server;
mod transport;

pub use transport::{HostSession, JoinSession, LeaveSession, close_session};

use crate::app_state::AppState;
use crate::config::network::{SEND_INTERVAL, TICK_RATE};
use bevy::prelude::*;
use lightyear::prelude::{client::ClientPlugins, server::ServerPlugins, server::Started, *};
use std::time::Duration;

/// Lightyear's client and server plugins, the tick rate and replication interval, and closing the
/// connection when the app goes back to the menu.
pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        let tick_duration = Duration::from_secs_f64(1.0 / TICK_RATE);

        app.add_plugins((
            ClientPlugins { tick_duration },
            ServerPlugins { tick_duration },
            server::ServerPlugin,
            client::ClientPlugin,
        ));

        // Configure the replication interval for the network timeline.
        app.insert_resource(ReplicationMetadata::new(SEND_INTERVAL));

        app.add_systems(OnEnter(AppState::MainMenu), transport::close_session);
    }
}

/// Run condition: `true` on the peer that is hosting the session (it has a started server).
///
/// Use it for systems that decide the game state, such as spawning or moving the ball, so that
/// only the host runs them and clients just receive the result.
pub fn is_host(servers: Query<(), (With<Server>, With<Started>)>) -> bool {
    !servers.is_empty()
}

#[cfg(test)]
mod tests {
    use super::NetworkPlugin;
    use bevy::prelude::*;

    #[test]
    fn network_plugin_initializes() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::state::app::StatesPlugin,
            NetworkPlugin,
        ));
        // `App::run` normally calls these; lightyear inserts resources in `finish`.
        app.finish();
        app.cleanup();
        app.update();
    }
}
