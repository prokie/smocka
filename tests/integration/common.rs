//! Shared test harness: headless apps that run real networking over localhost UDP.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use lightyear::prelude::server::ServerUdpIo;
use lightyear::prelude::*;
use smocka::SmockaPlugin;
use smocka::app_state::{AppState, SessionEndReason};
use smocka::minigames::Minigame;
use smocka::minigames::pong_royale::RoyaleRules;
use smocka::minigames::settings::{MinigameSettings, SettingId};
use smocka::minigames::two_player_pong::PongRules;
use smocka::network::{HostSession, JoinSession, LagPreset, LeaveSession, SimulatedLag};
use smocka::players::{Player, PlayerLeft};
use smocka::profile::LocalProfile;
use smocka::session::{SessionPhase, StartMinigame};

/// A headless app with the network plugin, plus queries for its connection state.
pub struct TestApp(pub App);

/// The [`Player::number`]s of every [`PlayerLeft`] this app has seen, in order.
#[derive(Resource, Default)]
struct Departures(Vec<u32>);

impl TestApp {
    pub fn new() -> Self {
        let mut app = App::new();
        // Keyboard and gamepad input, which the game reads through `bevy_enhanced_input`. Tests
        // press keys in its `ButtonInput<KeyCode>`.
        app.add_plugins((
            MinimalPlugins,
            bevy::input::InputPlugin,
            bevy::state::app::StatesPlugin,
            SmockaPlugin,
        ));
        // Pong is started all over the tests, a tournament's worth of matches at a time: none of
        // them wait for its countdown, except a test of the countdown, which sets one. Nor do Pong
        // Royale's.
        app.insert_resource(PongRules {
            countdown: Duration::ZERO,
        });
        app.insert_resource(RoyaleRules {
            countdown: Duration::ZERO,
        });
        // `App::run` normally calls these; lightyear inserts resources in `finish`.
        app.init_resource::<Departures>();
        app.add_observer(|trigger: On<PlayerLeft>, mut seen: ResMut<Departures>| {
            seen.0.push(trigger.event().0.number);
        });
        app.finish();
        app.cleanup();
        Self(app)
    }

    /// A new app whose sessions are as slow as `preset`, as with `--lag`. Host or join with
    /// [`start_hosting`](Self::start_hosting) or [`start_joining`](Self::start_joining).
    pub fn with_lag(preset: LagPreset) -> Self {
        let mut app = Self::new();
        app.0
            .world_mut()
            .insert_resource(SimulatedLag::from(preset));
        app
    }

    /// A new app hosting a session on a free port. [`host_addr`](Self::host_addr) says which.
    pub fn host() -> Self {
        let mut app = Self::new();
        app.start_hosting();
        app
    }

    /// Host a session from this app, on a free localhost port the OS picks as the host binds it,
    /// and return the address to join it at.
    ///
    /// Picking a free port first and hosting on it after would leave a moment in between when a
    /// test running at the same time could take the port, and hosting would fail.
    pub fn start_hosting(&mut self) -> SocketAddr {
        let any_port = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
        HostSession { addr: any_port }
            .apply(self.0.world_mut())
            .expect("failed to host session");
        self.host_addr()
    }

    /// Host only: pick `value` for minigame setting `setting`, as the host does in the lobby.
    pub fn set_setting(&mut self, setting: SettingId, value: u32) {
        self.0
            .world_mut()
            .resource_mut::<MinigameSettings>()
            .set(setting, value);
    }

    /// The address this app's host listens on: the port the OS gave it.
    pub fn host_addr(&mut self) -> SocketAddr {
        let addr = self
            .0
            .world_mut()
            .query_filtered::<&LocalAddr, With<ServerUdpIo>>()
            .single(self.0.world())
            .expect("this app should be hosting")
            .0;
        assert_ne!(addr.port(), 0, "the host should have bound its port");
        addr
    }

    /// A new app joining the session at `addr`.
    pub fn join(addr: SocketAddr) -> Self {
        Self::join_as(addr, "")
    }

    /// A new app joining the session at `addr` with the player name `name`, as if typed in the
    /// main menu.
    pub fn join_as(addr: SocketAddr, name: &str) -> Self {
        let mut app = Self::new();
        app.0
            .world_mut()
            .insert_resource(LocalProfile { name: name.into() });
        app.start_joining(addr);
        app
    }

    /// Join the session at `addr` from this app.
    pub fn start_joining(&mut self, addr: SocketAddr) {
        JoinSession { addr }
            .apply(self.0.world_mut())
            .expect("failed to join session");
    }

    /// The round trip to the host Lightyear has measured, once this app has joined.
    pub fn round_trip(&mut self) -> Option<Duration> {
        self.0
            .world_mut()
            .query_filtered::<&Link, (With<Client>, With<Connected>)>()
            .iter(self.0.world())
            .next()
            .map(|link| link.stats.rtt)
    }

    /// Leave the session the way the menu does, with [`LeaveSession`].
    pub fn leave(&mut self) {
        LeaveSession.apply(self.0.world_mut());
    }

    /// Start `game` from the lobby, as the host's start buttons do.
    pub fn start_minigame(&mut self, game: Minigame) {
        StartMinigame(game).apply(self.0.world_mut());
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

    /// The addresses of the remote clients connected to this app's server.
    pub fn remote_client_addrs(&mut self) -> Vec<SocketAddr> {
        let world = self.0.world_mut();
        world
            .query_filtered::<&PeerAddr, (With<LinkOf>, With<Connected>, Without<Client>)>()
            .iter(world)
            .map(|addr| addr.0)
            .collect()
    }

    /// Number of lobby players this app can see (spawned locally on the host, replicated on clients).
    /// Every player this app can see, in joining order.
    pub fn player_list(&mut self) -> Vec<Player> {
        let world = self.0.world_mut();
        let mut players: Vec<Player> = world.query::<&Player>().iter(world).cloned().collect();
        players.sort_by_key(|player| player.number);
        players
    }

    /// The player called `name`, as this app sees it.
    pub fn player_named(&mut self, name: &str) -> Option<Player> {
        self.player_list()
            .into_iter()
            .find(|player| player.name == name)
    }

    pub fn players(&mut self) -> usize {
        self.count::<With<Player>>()
    }

    /// The [`Player::number`]s this app can see, sorted.
    pub fn player_numbers(&mut self) -> Vec<u32> {
        let world = self.0.world_mut();
        let mut numbers: Vec<u32> = world
            .query::<&Player>()
            .iter(world)
            .map(|player| player.number)
            .collect();
        numbers.sort_unstable();
        numbers
    }

    /// The number of the player this app is, once it can see it.
    pub fn own_player_number(&mut self) -> Option<u32> {
        let world = self.0.world_mut();
        let own_id = world
            .query_filtered::<&LocalId, (With<Client>, With<Connected>)>()
            .iter(world)
            .next()?
            .0;
        world
            .query::<&Player>()
            .iter(world)
            .find(|player| player.id == own_id)
            .map(|player| player.number)
    }

    /// The [`Player::number`]s of the players this app was told left, in order.
    pub fn departures(&self) -> Vec<u32> {
        self.0.world().resource::<Departures>().0.clone()
    }

    fn count<F: bevy::ecs::query::QueryFilter>(&mut self) -> usize {
        self.0
            .world_mut()
            .query_filtered::<(), F>()
            .iter(self.0.world())
            .count()
    }
}

/// A localhost address where nobody hosts, and the socket holding it: it never answers, and while
/// it's kept, nothing else (a host in a test running at the same time, say) can take the port.
pub fn silent_addr() -> (UdpSocket, SocketAddr) {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("failed to bind a UDP port");
    let addr = socket.local_addr().expect("a bound socket has an address");
    (socket, addr)
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

/// Update all `apps` for `duration` of real time, at the pace [`update_until`] keeps. For checking
/// that something doesn't happen.
pub fn update_for(apps: &mut [&mut TestApp], duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        for app in apps.iter_mut() {
            app.0.update();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Start a host and a joiner on a fresh port and wait until they are connected.
pub fn connected_pair() -> (TestApp, TestApp, SocketAddr) {
    let mut host = TestApp::host();
    let addr = host.host_addr();
    let mut client = TestApp::join(addr);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].connected_remote_links() == 1 && apps[1].connected_clients() == 1
    });
    (host, client, addr)
}

/// [`connected_pair`], with both as slow as `preset` (as with `--lag` on both): the round trip
/// between them is about the preset's, and some packets are lost.
pub fn lagged_pair(preset: LagPreset) -> (TestApp, TestApp) {
    let mut host = TestApp::with_lag(preset);
    let addr = host.start_hosting();
    let mut client = TestApp::with_lag(preset);
    client.start_joining(addr);
    // Lost handshake packets are sent again, which takes longer than over a clean connection.
    update_until_within(
        &mut [&mut host, &mut client],
        Duration::from_secs(15),
        |apps| apps[0].connected_remote_links() == 1 && apps[1].connected_clients() == 1,
    );
    (host, client)
}

/// Start a host and a joiner, have the host start Pong, and wait until both are playing it.
pub fn pair_in_pong() -> (TestApp, TestApp, SocketAddr) {
    let (mut host, mut client, addr) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);
    host.start_minigame(Minigame::TwoPlayerPong);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter()
            .all(|app| app.phase() == Some(SessionPhase::Playing(Minigame::TwoPlayerPong)))
    });
    (host, client, addr)
}
