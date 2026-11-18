//! Commands that start, join and leave a session.

use crate::app_state::{AppState, SessionEndReason};
use crate::config::network::{PRIVATE_KEY, PROTOCOL_ID};
use bevy::ecs::resource::IsResource;
use bevy::prelude::*;
use lightyear::netcode::{Key, NetcodeClient, NetcodeServer};
use lightyear::prelude::{
    client::{self},
    server::{self, ServerUdpIo, Start, Started, Stop},
    *,
};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// Get ready for a new session: remove the server and host client left over from the last time
/// this app hosted, forget why the last session ended, and go to [`AppState::Connecting`].
///
/// Old entities are only removed here, not when leaving, so that their goodbye packets have time
/// to be sent.
///
/// A joining client is never removed: Lightyear stores client-side resources (such as the
/// prediction manager) on it, so it must live as long as the app. [`JoinSession`] reuses it.
fn start_new_session(world: &mut World) {
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
    if let Some(mut reason) = world.get_resource_mut::<SessionEndReason>() {
        reason.0 = None;
    }
    if let Some(mut next) = world.get_resource_mut::<NextState<AppState>>() {
        next.set(AppState::Connecting);
    }
}

/// Command: start a server on `addr` and a local client in this process (the player hosting the session).
///
/// Queue it with `commands.queue(HostSession { addr })`, or apply it directly with `.apply(world)`.
#[derive(Debug, Clone, Copy)]
pub struct HostSession {
    pub addr: SocketAddr,
}

impl Command for HostSession {
    type Out = ();

    fn apply(self, world: &mut World) {
        start_new_session(world);
        let addr = self.addr;
        let server_config = server::NetcodeConfig::default()
            .with_protocol_id(PROTOCOL_ID)
            .with_key(Key::from(PRIVATE_KEY));

        let server = world
            .spawn((
                Name::from("HostServer"),
                NetcodeServer::new(server_config),
                LocalAddr(addr),
                ServerUdpIo,
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
    }
}

/// Command: connect to the host at `addr` over UDP.
///
/// Queue it with `commands.queue(JoinSession { addr })`, or apply it directly with `.apply(world)`.
#[derive(Debug, Clone, Copy)]
pub struct JoinSession {
    pub addr: SocketAddr,
}

impl Command for JoinSession {
    type Out = Result;

    fn apply(self, world: &mut World) -> Result {
        start_new_session(world);
        let addr = self.addr;
        let client_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::from(d.subsec_nanos()))
            .unwrap_or(1)
            .max(1);

        let auth = Authentication::Manual {
            server_addr: addr,
            client_id,
            private_key: Key::from(PRIVATE_KEY),
            protocol_id: PROTOCOL_ID,
        };

        let netcode = NetcodeClient::new(auth, client::NetcodeConfig::default())?;

        // Reuse the client from an earlier join, if there is one (see `start_new_session`).
        let earlier = world
            .query_filtered::<Entity, (With<Client>, With<NetcodeClient>)>()
            .iter(world)
            .next();
        let client = match earlier {
            Some(client) => {
                world.entity_mut(client).insert((PeerAddr(addr), netcode));
                client
            }
            None => world
                .spawn((
                    Name::from("Client"),
                    Client,
                    Link::default(),
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
        Ok(())
    }
}

/// Command: leave the current session (or stop connecting to one) and go back to the main menu.
///
/// This only switches to [`AppState::MainMenu`]. Entering the menu then disconnects this app's
/// client and stops its server, so it works the same whether the player or a lost connection
/// ended the session.
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

/// On entering the main menu: disconnect this app's client and stop its server, if they are
/// still running. A hosting player stopping the server disconnects everyone else too.
///
/// This runs during the state transition, before `PostUpdate`, so the goodbye packets are sent
/// this frame, before Lightyear despawns the server's client links in the next one. Otherwise
/// the other players would only notice when their connection times out.
pub(super) fn close_session(
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
