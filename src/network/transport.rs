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
/// The netcode client is built before anything else changes, so if that fails the app stays where
/// it was (in the main menu), [`SessionEndReason`] says why, and the error is returned.
///
/// Queue it with `commands.queue_handled(JoinSession { addr }, warn)` (the player already sees the
/// reason, so the error only needs logging), or apply it directly with `.apply(world)`. A plain
/// `commands.queue` hands a failure to Bevy's default error handler, which panics.
#[derive(Debug, Clone, Copy)]
pub struct JoinSession {
    pub addr: SocketAddr,
}

impl Command for JoinSession {
    type Out = Result;

    fn apply(self, world: &mut World) -> Result {
        let addr = self.addr;
        let client_id = new_client_id();

        let auth = Authentication::Manual {
            server_addr: addr,
            client_id,
            private_key: Key::from(PRIVATE_KEY),
            protocol_id: PROTOCOL_ID,
        };

        let netcode = match NetcodeClient::new(auth, client::NetcodeConfig::default()) {
            Ok(netcode) => netcode,
            Err(error) => {
                if let Some(mut reason) = world.get_resource_mut::<SessionEndReason>() {
                    reason.0 = Some(format!("Couldn't set up a connection to {addr}."));
                }
                return Err(error.into());
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

/// A netcode client id for a joining client: random, and never 0.
///
/// The client picks its own id, and the host refuses a connection whose id is already connected
/// (the joiner just times out with "Couldn't connect to the host"). A random 64-bit id makes two
/// players picking the same one practically impossible. Don't derive it from the clock: clients
/// started at about the same time can get the same value.
fn new_client_id() -> u64 {
    rand::random_range(1..=u64::MAX)
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

/// Disconnect this app's client and stop its server, if they are still running. A hosting player
/// stopping the server disconnects everyone else too.
///
/// Runs on entering the main menu. The menu also runs it when the window is closed, since the app
/// exits before it would get back to the menu.
///
/// This runs during the state transition, before `PostUpdate`, so the goodbye packets are sent
/// this frame, before Lightyear despawns the server's client links in the next one. Otherwise
/// the other players would only notice when their connection times out.
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
    use super::new_client_id;

    #[test]
    fn client_ids_differ_between_joins() {
        assert_ne!(new_client_id(), new_client_id());
    }

    #[test]
    fn client_ids_are_never_zero() {
        assert!((0..1000).all(|_| new_client_id() != 0));
    }
}
