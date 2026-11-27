//! Settings shared by the whole game: networking, the session's screens, parties and the virtual
//! screen. Each minigame keeps its own in its folder, such as
//! [`minigames::racing::config`](crate::minigames::racing::config).

pub mod network {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    use std::time::Duration;

    pub const TICK_RATE: f64 = 128.0;
    pub const SEND_INTERVAL: Duration = Duration::from_nanos((2.0 * 1e9 / TICK_RATE) as u64);

    /// UDP port the host listens on, and the port joiners use if they don't give one.
    pub const SERVER_PORT: u16 = 5000;
    /// Where the host listens: every network interface, so players on the same network can join.
    pub const HOST_ADDR: SocketAddr =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, SERVER_PORT));
    /// The address the menu suggests joining: a host on this machine.
    pub const SERVER_ADDR: SocketAddr =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, SERVER_PORT));

    /// This build's version, from `Cargo.toml`. Shown in the main menu.
    pub const VERSION: &str = env!("CARGO_PKG_VERSION");

    /// Must match between host and joining clients, or the host ignores the join (which then
    /// times out). Derived from [`VERSION`], so players on different versions can't play
    /// together: another version may register replicated components differently and break in
    /// confusing ways. Bump the version in `Cargo.toml` for every release.
    pub const PROTOCOL_ID: u64 = fnv1a(VERSION.as_bytes());

    /// The 64-bit FNV-1a hash of `bytes`: simple, stable across builds and platforms, and usable
    /// in a `const`.
    const fn fnv1a(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
            i += 1;
        }
        hash
    }

    pub const PRIVATE_KEY: [u8; 32] = [0; 32];

    #[cfg(test)]
    mod tests {
        use super::fnv1a;

        #[test]
        fn different_versions_get_different_protocol_ids() {
            assert_ne!(fnv1a(b"0.1.0"), fnv1a(b"0.1.1"));
            assert_ne!(fnv1a(b"0.1.0"), fnv1a(b"0.2.0"));
            // The published FNV-1a test vector, so the hash can't silently change.
            assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        }
    }
}

pub mod session {
    use std::time::Duration;

    /// The fewest players the host can start a minigame with.
    pub const MIN_PLAYERS: usize = 2;
    /// How long the results of a minigame stay up before the standings.
    pub const RESULTS_DURATION: Duration = Duration::from_secs(5);
    /// How long the standings stay up before everyone goes back to the lobby.
    pub const STANDINGS_DURATION: Duration = Duration::from_secs(5);
    /// How long the screen before each tournament match stays up, unless
    /// [`TournamentRules`](crate::tournament::TournamentRules) says otherwise.
    pub const NEXT_MATCH_DURATION: Duration = Duration::from_secs(4);
}

/// Parties: a set number of rounds, each a minigame the game picks. See [`crate::party`].
pub mod party {
    use std::time::Duration;

    /// How many rounds a party has, unless the host picks another number in the lobby.
    pub const ROUNDS: u32 = 5;
    /// The fewest rounds the host can pick.
    pub const MIN_ROUNDS: u32 = 1;
    /// The most rounds the host can pick.
    pub const MAX_ROUNDS: u32 = 20;
    /// How long the coming round's minigame is announced before it starts.
    pub const NEXT_ROUND_DURATION: Duration = Duration::from_secs(4);
    /// How long the party's winner is shown before everyone goes back to the lobby.
    pub const WINNER_DURATION: Duration = Duration::from_secs(8);
}

pub mod display {
    /// Width of the virtual screen in world units. The camera scales it to fit the window, and
    /// all gameplay sizes and speeds are in these units.
    pub const VIEW_WIDTH: f32 = 640.0;
    /// Height of the virtual screen in world units. See [`VIEW_WIDTH`].
    pub const VIEW_HEIGHT: f32 = 360.0;

    /// Texture pixels per world unit: art is drawn for a 1080p screen (3 × 640×360), and every
    /// sprite is given its size in world units, so the art's resolution never affects gameplay.
    /// A 4K screen scales the art up 2× and a 720p one down 1.5×, both fine with linear filtering
    /// and no mipmaps.
    pub const ART_SCALE: f32 = 3.0;
}
