use crate::config::network::{PRIVATE_KEY, PROTOCOL_ID};
use bevy::prelude::*;
use lightyear::netcode::{Key, NetcodeClient, NetcodeServer};
use lightyear::prelude::{
    client::{self},
    server::{self, ServerUdpIo, Start},
    *,
};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

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

        let client = world
            .spawn((
                Name::from("Client"),
                Client,
                Link::default(),
                LocalAddr(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))),
                PeerAddr(addr),
                ReplicationReceiver,
                PredictionManager::default(),
                NetcodeClient::new(auth, client::NetcodeConfig::default())?,
                UdpIo::default(),
            ))
            .id();
        world.trigger(Connect { entity: client });

        info!("Joining {addr} as client {client_id}");
        Ok(())
    }
}
