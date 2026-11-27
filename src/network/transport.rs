use super::lag::{SimulatedLag, receive_conditioner};
use crate::app_state::{AppState, SessionEndReason};
use crate::config::network::{PRIVATE_KEY, PROTOCOL_ID};
use crate::profile::{LocalProfile, name_to_user_data};
use bevy::ecs::resource::IsResource;
use bevy::prelude::*;
use lightyear::netcode::{ConnectToken, Key, NetcodeClient, NetcodeServer};
use lightyear::prelude::{
    client::{self},
    server::{self, ServerUdpIo, Start, Started, Stop},
    *,
};
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};

/// Old server and host-client entities are only removed here, not when leaving, so their goodbye
/// packets have time to be sent.
///
/// A joining client is never removed: Lightyear stores client-side resources (such as the
/// prediction manager) on it, so it must live as long as the app. [`JoinSession`] reuses it.
fn start_new_session(world: &mut World) {
    despawn_old_session(world);
    enter_connecting(world);
}

/// Despawn the previous session's server and host client. This also closes the old server's
/// socket, so hosting again on the same port works.
fn despawn_old_session(world: &mut World) {
    let old: Vec<Entity> = world
        .query_filtered::<Entity, (
            Or<(With<Server>, (With<Client>, With<LinkOf>))>,
            Without<IsResource>,
        )>()
        .iter(world)
        .collect();
    for entity in old {
        world.despawn(entity);
    }
}

fn enter_connecting(world: &mut World) {
    if let Some(mut reason) = world.get_resource_mut::<SessionEndReason>() {
        reason.0 = None;
    }
    if let Some(mut next) = world.get_resource_mut::<NextState<AppState>>() {
        next.set(AppState::Connecting);
    }
}

/// Check that a UDP socket can be bound to `addr` right now, then release it.
///
/// Lightyear can't report a failed bind: its `LinkStart` observer returns the error, which Bevy's
/// default error handler turns into a panic. Even without the panic, the netcode server would
/// still be `Started`, leaving the host in a lobby nobody can join.
fn check_can_bind(addr: SocketAddr) -> std::io::Result<()> {
    UdpSocket::bind(addr).map(drop)
}

/// The main-menu message for a failed [`check_can_bind`].
fn describe_host_error(error: &std::io::Error, addr: SocketAddr) -> String {
    let port = addr.port();
    match error.kind() {
        ErrorKind::AddrInUse => format!("Port {port} is in use. Is the game already hosting?"),
        _ => format!("Couldn't host on port {port}."),
    }
}

/// Command: start a server on `addr` and a local client in this process.
///
/// If `addr` can't be bound (another copy of the game is hosting, say), the app stays in the menu
/// and [`SessionEndReason`] says why. Queue it with
/// `commands.queue_handled(HostSession { addr }, warn)`: a plain `commands.queue` panics on error.
#[derive(Debug, Clone, Copy)]
pub struct HostSession {
    pub addr: SocketAddr,
}

impl Command for HostSession {
    type Out = Result;

    fn apply(self, world: &mut World) -> Result {
        let addr = self.addr;
        // Before checking the port, so a previous session's server on the same port lets go of it.
        despawn_old_session(world);
        if let Err(error) = check_can_bind(addr) {
            if let Some(mut reason) = world.get_resource_mut::<SessionEndReason>() {
                reason.0 = Some(describe_host_error(&error, addr));
            }
            return Err(format!("Not hosting: can't bind {addr}: {error}").into());
        }
        enter_connecting(world);

        let server_config = server::NetcodeConfig {
            // We bind to `0.0.0.0`, which Lightyear only matches against loopback addresses in a
            // connect token, so with the check on, joins over LAN (`192.168.x.y`) get ignored.
            server_addr_check: false,
            ..server::NetcodeConfig::default()
                .with_protocol_id(PROTOCOL_ID)
                .with_key(Key::from(PRIVATE_KEY))
        };

        let server = world
            .spawn((
                Name::from("HostServer"),
                NetcodeServer::new(server_config),
                LocalAddr(addr),
                ServerUdpIo,
                // Each joiner's link gets a copy of the conditioner, if any.
                Endpoint::new(receive_conditioner(world)),
            ))
            .id();
        world.trigger(Start { entity: server });

        let client = world
            .spawn((
                Name::from("HostClient"),
                Client,
                LinkOf { endpoint: server },
            ))
            .id();
        world.trigger(Connect { entity: client });

        info!("Hosting on {addr}");
        log_simulated_lag(world);
        Ok(())
    }
}

/// Command: connect to the host at `addr` over UDP.
///
/// On failure the app stays in the menu and [`SessionEndReason`] says why. Queue it with
/// `commands.queue_handled(JoinSession { addr }, warn)`: a plain `commands.queue` panics on error.
#[derive(Debug, Clone, Copy)]
pub struct JoinSession {
    pub addr: SocketAddr,
}

impl Command for JoinSession {
    type Out = Result;

    fn apply(self, world: &mut World) -> Result {
        let addr = self.addr;
        let client_id = new_client_id();

        // The player's name travels in the token's user data, so the host knows it on connect.
        let name = world
            .get_resource::<LocalProfile>()
            .map(|profile| profile.name.clone())
            .unwrap_or_default();
        let config = client::NetcodeConfig::default();
        let token = ConnectToken::build(addr, PROTOCOL_ID, client_id, Key::from(PRIVATE_KEY))
            .timeout_seconds(config.client_timeout_secs)
            .expire_seconds(config.token_expire_secs)
            .user_data(name_to_user_data(&name))
            .generate();
        let netcode = match token
            .map_err(BevyError::from)
            .and_then(|token| Ok(NetcodeClient::new(Authentication::Token(token), config)?))
        {
            Ok(netcode) => netcode,
            Err(error) => {
                if let Some(mut reason) = world.get_resource_mut::<SessionEndReason>() {
                    reason.0 = Some(format!("Couldn't set up a connection to {addr}."));
                }
                return Err(error);
            }
        };

        start_new_session(world);

        // Reuse the client from an earlier join, if there is one (see `start_new_session`).
        let earlier = world
            .query_filtered::<Entity, (With<Client>, With<NetcodeClient>)>()
            .iter(world)
            .next();
        let client = match earlier {
            Some(client) => {
                // Close the old socket, so `Connect` binds a new one on a new port. Lightyear's
                // UDP server remembers the address of every client it has seen, so coming back
                // from the same one makes it drop the first packet and log an error.
                world.trigger(Unlink {
                    entity: client,
                    reason: UnlinkReason::UserRequested(None),
                });
                world.entity_mut(client).insert((PeerAddr(addr), netcode));
                let conditioner = receive_conditioner(world);
                if let Some(mut link) = world.get_mut::<Link>(client) {
                    link.recv.conditioner = conditioner;
                }
                client
            }
            None => world
                .spawn((
                    Name::from("Client"),
                    Client,
                    Link::default().with_conditioner(receive_conditioner(world)),
                    LocalAddr(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))),
                    PeerAddr(addr),
                    ReplicationReceiver,
                    PredictionManager::default(),
                    netcode,
                    UdpIo::default(),
                ))
                .id(),
        };
        world.trigger(Connect { entity: client });

        info!("Joining {addr} as client {client_id}");
        log_simulated_lag(world);
        Ok(())
    }
}

/// Say in the log when `--lag` is making this session's connection worse than it is.
fn log_simulated_lag(world: &World) {
    if let Some(lag) = world.get_resource::<SimulatedLag>() {
        info!("Simulating lag (--lag): {}", lag.describe());
    }
}

/// Random and never 0. The host refuses an id that is already connected, so don't derive it from
/// the clock: clients started at about the same time would collide.
fn new_client_id() -> u64 {
    rand::random_range(1..=u64::MAX)
}

/// Command: leave the session (or stop connecting) and go back to the main menu, which then runs
/// [`close_session`].
#[derive(Debug, Clone, Copy)]
pub struct LeaveSession;

impl Command for LeaveSession {
    type Out = ();

    fn apply(self, world: &mut World) {
        world
            .resource_mut::<NextState<AppState>>()
            .set(AppState::MainMenu);
        info!("Leaving the session");
    }
}

/// Disconnect this app's client and stop its server. Runs on entering the main menu and when the
/// window is closed.
///
/// It must run before `PostUpdate`, so goodbye packets go out before Lightyear despawns the
/// server's client links; otherwise other players only notice when their connection times out.
pub fn close_session(
    clients: Query<Entity, (With<Client>, Without<Disconnected>)>,
    servers: Query<Entity, (With<Server>, With<Started>)>,
    mut commands: Commands,
) {
    for entity in &clients {
        commands.trigger(Disconnect { entity });
    }
    for entity in &servers {
        commands.trigger(Stop { entity });
    }
}

#[cfg(test)]
mod tests {
    use super::{check_can_bind, describe_host_error, new_client_id};
    use std::io::{Error, ErrorKind};
    use std::net::UdpSocket;

    #[test]
    fn a_taken_port_cant_be_bound() {
        let taken = UdpSocket::bind("127.0.0.1:0").unwrap();
        let error = check_can_bind(taken.local_addr().unwrap()).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AddrInUse);
    }

    #[test]
    fn checking_a_free_port_releases_it() {
        let addr = UdpSocket::bind("127.0.0.1:0")
            .and_then(|socket| socket.local_addr())
            .unwrap();
        check_can_bind(addr).unwrap();
        check_can_bind(addr).unwrap();
    }

    #[test]
    fn a_port_in_use_suggests_the_game_is_already_hosting() {
        let addr = "0.0.0.0:5000".parse().unwrap();
        assert_eq!(
            describe_host_error(&Error::from(ErrorKind::AddrInUse), addr),
            "Port 5000 is in use. Is the game already hosting?"
        );
        assert_eq!(
            describe_host_error(&Error::from(ErrorKind::PermissionDenied), addr),
            "Couldn't host on port 5000."
        );
    }

    #[test]
    fn client_ids_differ_between_joins() {
        assert_ne!(new_client_id(), new_client_id());
    }

    #[test]
    fn client_ids_are_never_zero() {
        assert!((0..1000).all(|_| new_client_id() != 0));
    }
}
