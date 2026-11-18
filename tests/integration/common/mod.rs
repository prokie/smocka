//! Shared test harness: headless apps that run real networking over localhost UDP.

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use lightyear::prelude::*;
use smocka::SmockaPlugin;
use smocka::app_state::{AppState, SessionEndReason};
use smocka::lobby::Player;
use smocka::network::{HostSession, JoinSession, LeaveSession};
use smocka::session::SessionPhase;

/// A headless app with the network plugin, plus queries for its connection state.
pub struct TestApp(pub App);

impl TestApp {
    pub fn new() -> Self {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin, SmockaPlugin));
        // `App::run` normally calls these; lightyear inserts resources in `finish`.
        app.finish();
        app.cleanup();
        Self(app)
    }

    /// A new app hosting a session on `addr`.
    pub fn host(addr: SocketAddr) -> Self {
        let mut app = Self::new();
        HostSession { addr }.apply(app.0.world_mut());
        app
    }

    /// A new app joining the session at `addr`.
    pub fn join(addr: SocketAddr) -> Self {
        let mut app = Self::new();
        JoinSession { addr }
            .apply(app.0.world_mut())
            .expect("failed to join session");
        app
    }

    /// Leave the session the way the menu does, with [`LeaveSession`].
    pub fn leave(&mut self) {
        LeaveSession.apply(self.0.world_mut());
    }

    /// This app's [`AppState`].
    pub fn app_state(&self) -> AppState {
        *self.0.world().resource::<State<AppState>>().get()
    }

    /// This app's [`SessionPhase`], or `None` if it isn't in a session.
    pub fn phase(&self) -> Option<SessionPhase> {
        self.0
            .world()
            .get_resource::<State<SessionPhase>>()
            .map(|state| *state.get())
    }

    /// Why this app's last session ended, if it wasn't this player's choice.
    pub fn end_reason(&self) -> Option<String> {
        self.0.world().resource::<SessionEndReason>().0.clone()
    }

    /// Gracefully disconnect this app's client.
    pub fn disconnect(&mut self) {
        let entity = self.client_entity();
        self.0.world_mut().trigger(Disconnect { entity });
    }

    /// This app's (single) client entity.
    pub fn client_entity(&mut self) -> Entity {
        self.0
            .world_mut()
            .query_filtered::<Entity, With<Client>>()
            .single(self.0.world())
            .expect("expected exactly one client entity")
    }

    /// Number of `Client` entities that are connected.
    pub fn connected_clients(&mut self) -> usize {
        self.count::<(With<Client>, With<Connected>)>()
    }

    /// Number of `Client` entities that have been disconnected.
    pub fn disconnected_clients(&mut self) -> usize {
        self.count::<(With<Client>, With<Disconnected>)>()
    }

    /// Number of server-side links to remote clients (excluding the host's own client) that are connected.
    pub fn connected_remote_links(&mut self) -> usize {
        self.count::<(With<LinkOf>, With<Connected>, Without<Client>)>()
    }

    /// Number of lobby players this app can see (spawned locally on the host, replicated on clients).
    pub fn players(&mut self) -> usize {
        self.count::<With<Player>>()
    }

    fn count<F: bevy::ecs::query::QueryFilter>(&mut self) -> usize {
        self.0
            .world_mut()
            .query_filtered::<(), F>()
            .iter(self.0.world())
            .count()
    }
}

/// Ask the OS for a free localhost UDP port, so tests don't collide with each other or a running game.
pub fn free_addr() -> SocketAddr {
    UdpSocket::bind("127.0.0.1:0")
        .and_then(|socket| socket.local_addr())
        .expect("failed to find a free UDP port")
}

/// Update all `apps` until `done` holds, or panic after 5 seconds.
pub fn update_until(apps: &mut [&mut TestApp], done: impl FnMut(&mut [&mut TestApp]) -> bool) {
    update_until_within(apps, Duration::from_secs(5), done);
}

/// Update all `apps` until `done` holds, or panic after `timeout`.
pub fn update_until_within(
    apps: &mut [&mut TestApp],
    timeout: Duration,
    mut done: impl FnMut(&mut [&mut TestApp]) -> bool,
) {
    let deadline = Instant::now() + timeout;
    loop {
        for app in apps.iter_mut() {
            app.0.update();
        }
        if done(apps) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for condition");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Start a host and a joiner on a fresh port and wait until they are connected.
pub fn connected_pair() -> (TestApp, TestApp, SocketAddr) {
    let addr = free_addr();
    let mut host = TestApp::host(addr);
    let mut client = TestApp::join(addr);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].connected_remote_links() == 1 && apps[1].connected_clients() == 1
    });
    (host, client, addr)
}
