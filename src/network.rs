mod client;
mod server;
mod transport;

pub use transport::{HostSession, JoinSession, LeaveSession, close_session};

use crate::app_state::AppState;
use crate::config::network::{SEND_INTERVAL, TICK_RATE};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use lightyear::prelude::{client::ClientPlugins, server::ServerPlugins, server::Started, *};
use std::time::Duration;

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

        app.insert_resource(ReplicationMetadata::new(SEND_INTERVAL));

        app.add_systems(OnEnter(AppState::MainMenu), transport::close_session);
    }
}

/// Whether this app is hosting the session (it has a started server).
#[derive(SystemParam)]
pub struct Hosting<'w, 's> {
    servers: Query<'w, 's, (), (With<Server>, With<Started>)>,
}

impl Hosting<'_, '_> {
    pub fn is_hosting(&self) -> bool {
        !self.servers.is_empty()
    }
}

/// Run condition: `true` on the hosting peer. Outside a system, use
/// `world.run_system_cached(is_host)`.
pub fn is_host(hosting: Hosting) -> bool {
    hosting.is_hosting()
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
