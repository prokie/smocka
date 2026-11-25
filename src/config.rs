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

    /// Must match between host and joining clients.
    pub const PROTOCOL_ID: u64 = 0;
    pub const PRIVATE_KEY: [u8; 32] = [0; 32];
}

pub mod session {
    use std::time::Duration;

    /// The fewest players the host can start a minigame with.
    pub const MIN_PLAYERS: usize = 2;
    /// How long the results of a minigame stay up before everyone goes back to the lobby.
    pub const RESULTS_DURATION: Duration = Duration::from_secs(5);
}

pub mod display {
    /// Width of the virtual screen in world units (one unit = one art pixel).
    ///
    /// 640×360 scales by whole numbers to 720p (2×), 1080p (3×), 1440p (4×) and 4K (6×),
    /// which keeps pixel art crisp.
    pub const VIEW_WIDTH: f32 = 640.0;
    /// Height of the virtual screen in world units. See [`VIEW_WIDTH`].
    pub const VIEW_HEIGHT: f32 = 360.0;
}

pub mod minigames {
    pub mod two_player_pong {
        use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

        /// Thickness of the wall drawn along the top and bottom of `arena.png`, in world units.
        pub const WALL_THICKNESS: f32 = 4.0;

        /// Horizontal distance from the arena centre to each paddle, in world units.
        pub const PADDLE_X_OFFSET: f32 = 296.0;
        /// Paddle width in world units. Keep `paddle.png` the same width.
        pub const PADDLE_WIDTH: f32 = 8.0;
        /// Paddle height in world units. Keep `paddle.png` the same height.
        pub const PADDLE_HEIGHT: f32 = 48.0;
        /// Paddle speed in world units per second.
        pub const PADDLE_SPEED: f32 = 300.0;
        /// Highest `y` a paddle centre may reach (the lowest is the negative of this).
        /// Keeps the paddle inside the walls at the top and bottom of the arena.
        pub const PADDLE_MAX_Y: f32 = VIEW_HEIGHT / 2.0 - WALL_THICKNESS - PADDLE_HEIGHT / 2.0;

        /// Ball width and height in world units. Keep `ball.png` the same size.
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
        use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};

        /// Car length (nose to tail) in world units. Keep `car.png` the same width.
        pub const CAR_LENGTH: f32 = 16.0;
        /// Car width in world units. Keep `car.png` the same height.
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

        /// `x` of the starting grid's column of cars.
        pub const GRID_X: f32 = -240.0;
        /// Distance between neighbouring cars on the starting grid.
        pub const GRID_SPACING: f32 = 28.0;
    }
}
