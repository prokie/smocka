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
    /// How long the results of a minigame stay up before everyone goes back to the lobby.
    pub const RESULTS_DURATION: Duration = Duration::from_secs(5);
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

pub mod minigames {
    pub mod two_player_pong {
        use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

        /// Thickness of the wall drawn along the top and bottom of `arena.png`, in world units.
        pub const WALL_THICKNESS: f32 = 4.0;

        /// Horizontal distance from the arena centre to each paddle, in world units.
        pub const PADDLE_X_OFFSET: f32 = 296.0;
        /// Paddle width in world units. Draw `paddle.png` with the same proportions.
        pub const PADDLE_WIDTH: f32 = 8.0;
        /// Paddle height in world units. Draw `paddle.png` with the same proportions.
        pub const PADDLE_HEIGHT: f32 = 48.0;
        /// Paddle speed in world units per second.
        pub const PADDLE_SPEED: f32 = 300.0;
        /// Highest `y` a paddle centre may reach (the lowest is the negative of this).
        /// Keeps the paddle inside the walls at the top and bottom of the arena.
        pub const PADDLE_MAX_Y: f32 = VIEW_HEIGHT / 2.0 - WALL_THICKNESS - PADDLE_HEIGHT / 2.0;

        /// Ball width and height in world units. Draw `ball.png` square.
        pub const BALL_SIZE: f32 = 8.0;
        /// Ball speed along each axis when it is served, in world units per second.
        /// Keep `x` positive: serves flip it to aim at the side being served to.
        pub const BALL_SERVE_VELOCITY: bevy::math::Vec2 = bevy::math::Vec2::new(150.0, 100.0);
        /// Each paddle hit multiplies the ball's speed by this, up to [`BALL_MAX_SPEED`].
        /// Each serve starts again at [`BALL_SERVE_VELOCITY`].
        pub const BALL_HIT_SPEEDUP: f32 = 1.1;
        /// The ball never gets faster than this, in world units per second.
        ///
        /// Keep it below `(PADDLE_WIDTH + BALL_SIZE) * TICK_RATE` (2048 for the current values).
        /// Above that, the ball can jump past a paddle in one tick without ever overlapping it.
        pub const BALL_MAX_SPEED: f32 = 600.0;
        /// The steepest angle the ball can leave a paddle at, in radians away from straight across.
        ///
        /// Where the ball hits a paddle decides its angle: the centre sends it straight across,
        /// and the very ends send it off at this angle, up or down. Keep it well below 90°, or the
        /// ball can crawl almost vertically across the arena.
        pub const BALL_MAX_BOUNCE_ANGLE: f32 = 37.0_f32.to_radians();
        /// Highest `y` the ball centre may reach before it bounces (the lowest is the negative).
        pub const BALL_MAX_Y: f32 = VIEW_HEIGHT / 2.0 - WALL_THICKNESS - BALL_SIZE / 2.0;
        /// How far left or right of the centre the ball centre must get to score a goal.
        /// At this distance the ball is fully off screen.
        pub const GOAL_X: f32 = VIEW_WIDTH / 2.0 + BALL_SIZE / 2.0;
        /// Goals a player needs to win, unless [`PongRules`](crate::minigames::two_player_pong::score::PongRules)
        /// says otherwise.
        pub const GOALS_TO_WIN: u32 = 5;
        /// Font size of the score at the top of the arena, in world units.
        pub const SCORE_FONT_SIZE: f32 = 32.0;
    }

    /// Car handling and the starting grid for the racing minigame. Speeds are in world units per
    /// second, accelerations in world units per second², and angles in radians.
    pub mod racing {
        use std::time::Duration;

        use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

        /// Car length (nose to tail) in world units. Draw `car.png` with the same proportions.
        pub const CAR_LENGTH: f32 = 16.0;
        /// Car width in world units. Draw `car.png` with the same proportions.
        pub const CAR_WIDTH: f32 = 10.0;

        /// Top speed going forwards.
        pub const MAX_SPEED: f32 = 200.0;
        /// Top speed in reverse.
        pub const MAX_REVERSE_SPEED: f32 = 70.0;
        /// How fast the throttle speeds the car up.
        pub const ACCELERATION: f32 = 250.0;
        /// How fast holding brake (or throttle while rolling backwards) slows the car down.
        pub const BRAKE_DECELERATION: f32 = 500.0;
        /// How fast brake speeds the car up backwards once it has stopped.
        pub const REVERSE_ACCELERATION: f32 = 150.0;
        /// How fast the car slows down with neither throttle nor brake held.
        pub const COAST_DECELERATION: f32 = 150.0;

        /// How fast the car turns with steering held, once it is going at least
        /// [`FULL_STEER_SPEED`]. Slower cars turn proportionally slower, and a car standing still
        /// can't turn at all.
        pub const TURN_RATE: f32 = 3.5;
        /// The speed (either way) at which steering reaches its full [`TURN_RATE`].
        pub const FULL_STEER_SPEED: f32 = 60.0;

        /// The furthest a car centre may get from the middle of the screen, sideways and up or
        /// down. Keeps the whole car on screen whichever way it points.
        pub const CAR_MAX_X: f32 = VIEW_WIDTH / 2.0 - CAR_LENGTH / 2.0;
        /// See [`CAR_MAX_X`].
        pub const CAR_MAX_Y: f32 = VIEW_HEIGHT / 2.0 - CAR_LENGTH / 2.0;
        /// A car pushing against the edge of the screen goes no faster than this, so it can still
        /// turn away from it.
        pub const EDGE_MAX_SPEED: f32 = 40.0;

        /// Top speed either way on grass.
        pub const GRASS_MAX_SPEED: f32 = 80.0;
        /// How fast a car going faster than [`GRASS_MAX_SPEED`] slows down on grass. Keep it above
        /// [`ACCELERATION`], or throttle would beat it.
        pub const GRASS_DECELERATION: f32 = 400.0;

        /// The track is a stadium-shaped ring: the road is every point between
        /// [`TRACK_INNER_RADIUS`] and [`TRACK_OUTER_RADIUS`] from a horizontal centre line running
        /// from `-TRACK_HALF_LENGTH` to `TRACK_HALF_LENGTH` along `y = 0`. Redraw `track.png` if
        /// you change these.
        pub const TRACK_HALF_LENGTH: f32 = 140.0;
        /// See [`TRACK_HALF_LENGTH`].
        pub const TRACK_INNER_RADIUS: f32 = 100.0;
        /// See [`TRACK_HALF_LENGTH`].
        pub const TRACK_OUTER_RADIUS: f32 = 160.0;

        /// `x` of the start line, on the bottom straight. Cars race anticlockwise, so they cross it
        /// heading right.
        pub const START_LINE_X: f32 = 0.0;
        /// Distance between rows of the starting grid, and from the start line to the front row.
        pub const GRID_ROW_SPACING: f32 = 24.0;
        /// Distance of each of the grid's two lanes from the middle of the road.
        pub const GRID_LANE_OFFSET: f32 = 14.0;

        /// Laps in a race.
        pub const LAPS: u32 = 3;
        /// How long the cars wait on the grid before the start, unless
        /// [`RaceRules`](crate::minigames::racing::race::RaceRules) says otherwise.
        pub const COUNTDOWN: Duration = Duration::from_secs(3);
        /// How long the race goes on after the first car finishes, so one slow player can't hold
        /// everyone up. Unless [`RaceRules`](crate::minigames::racing::race::RaceRules) says
        /// otherwise.
        pub const FINISH_GRACE: Duration = Duration::from_secs(10);
        /// How far each checkpoint reaches along the track. A car must be seen inside it on some
        /// tick, so keep it well above how far a car moves in one tick (`MAX_SPEED / TICK_RATE`);
        /// `track.rs` checks this when it compiles.
        pub const CHECKPOINT_DEPTH: f32 = 16.0;
    }
}
