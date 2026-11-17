pub mod network {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    use std::time::Duration;

    pub const TICK_RATE: f64 = 128.0;
    pub const SEND_INTERVAL: Duration = Duration::from_millis(50);

    pub const SERVER_PORT: u16 = 5000;
    pub const SERVER_ADDR: SocketAddr =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, SERVER_PORT));

    /// Must match between host and joining clients.
    pub const PROTOCOL_ID: u64 = 0;
    pub const PRIVATE_KEY: [u8; 32] = [0; 32];
}

/// Rendering constants shared by every minigame.
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
        /// Horizontal distance from the arena centre to each paddle, in world units.
        pub const PADDLE_X_OFFSET: f32 = 296.0;
        /// Paddle height in world units. Keep `paddle.png` the same height.
        pub const PADDLE_HEIGHT: f32 = 48.0;
        /// Paddle speed in world units per second.
        pub const PADDLE_SPEED: f32 = 300.0;
        /// Highest `y` a paddle centre may reach (the lowest is the negative of this).
        /// Keeps the paddle inside the 4-unit wall at the top and bottom of the arena.
        pub const PADDLE_MAX_Y: f32 =
            crate::config::display::VIEW_HEIGHT / 2.0 - 4.0 - PADDLE_HEIGHT / 2.0;
    }
}
