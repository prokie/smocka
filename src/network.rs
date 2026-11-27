mod client;
mod lag;
mod server;
mod transport;

pub use lag::{LagPreset, SimulatedLag};
pub use transport::{HostSession, JoinSession, LeaveSession, close_session};

use crate::app_state::AppState;
use crate::config::network::{SEND_INTERVAL, SERVER_PORT, TICK_RATE};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use lightyear::prelude::{client::ClientPlugins, server::ServerPlugins, server::Started, *};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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

/// Accepts an IPv4 address or `localhost`, with or without a port (default [`SERVER_PORT`]).
/// Other host names are not looked up, so this never blocks on DNS.
pub fn parse_address(typed: &str) -> Option<SocketAddr> {
    let typed = typed.trim();
    let (host, port) = match typed.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (typed, SERVER_PORT),
    };
    let ip = if host.eq_ignore_ascii_case("localhost") {
        Ipv4Addr::LOCALHOST
    } else {
        host.parse().ok()?
    };
    Some(SocketAddr::new(IpAddr::V4(ip), port))
}

#[cfg(test)]
mod tests {
    use super::{NetworkPlugin, parse_address};
    use crate::config::network::SERVER_PORT;
    use bevy::prelude::*;
    use std::net::SocketAddr;

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

    fn addr(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    #[test]
    fn reads_an_address_with_a_port() {
        assert_eq!(
            parse_address("192.168.1.10:6000"),
            Some(addr("192.168.1.10:6000"))
        );
    }

    #[test]
    fn uses_the_default_port_when_none_is_given() {
        assert_eq!(
            parse_address(" 10.0.0.2 "),
            Some(addr(&format!("10.0.0.2:{SERVER_PORT}")))
        );
    }

    #[test]
    fn understands_localhost() {
        assert_eq!(
            parse_address("localhost:5001"),
            Some(addr("127.0.0.1:5001"))
        );
    }

    #[test]
    fn rejects_anything_else() {
        for typed in ["", "my-friend", "1.2.3", "1.2.3.4:port", "1.2.3.4:99999"] {
            assert_eq!(parse_address(typed), None, "{typed:?}");
        }
    }
}
