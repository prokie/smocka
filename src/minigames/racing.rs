mod boost_visuals;
pub mod bumps;
pub mod car;
mod checkpoint_guide;
pub mod config;
pub mod progress;
pub mod race;
mod racing_input;
mod skid_marks;
pub mod slipstream;
pub mod sounds;
pub mod track;
mod visuals;

pub use racing_input::RacingInput;
pub use visuals::RacingVisualsPlugin;

use bevy::prelude::*;

use super::settings::{Setting, SettingId, SettingValues};
use super::{Minigame, MinigameInfo};
use crate::session::SessionPhase;
use config::{GRID_SLOTS, LAPS, MAX_LAPS, MIN_LAPS};
use track::TrackId;

/// The phase a race is played in.
pub const PHASE: SessionPhase = SessionPhase::Playing(Minigame::Racing);

/// What the session and the menus need to know about racing. Every player can race, up to the
/// starting grid's [`GRID_SLOTS`].
pub const INFO: MinigameInfo = MinigameInfo {
    name: "Racing",
    min_players: 2,
    max_players: GRID_SLOTS,
    // The default track's: see `thumbnail`.
    thumbnail: track::FIGURE_EIGHT.thumbnail,
    settings: &[
        Setting {
            name: "Laps",
            min: MIN_LAPS,
            max: MAX_LAPS,
            default: LAPS,
            labels: &[],
        },
        Setting {
            name: "Track",
            min: 0,
            max: TrackId::ALL.len() as u32 - 1,
            default: TrackId::FigureEight as u32,
            labels: &TrackId::NAMES,
        },
    ],
};

/// How many laps a race has, picked by the host (see [`settings`](super::settings)).
pub const LAPS_SETTING: SettingId = SettingId::new(Minigame::Racing, 0);

/// Which track the race is on, picked by the host: an index in [`TrackId::ALL`] (see
/// [`TrackId::from_setting`]).
pub const TRACK_SETTING: SettingId = SettingId::new(Minigame::Racing, 1);

/// Racing's picture on its lobby tile with `settings` picked: the picked track's.
pub fn thumbnail(settings: &SettingValues) -> &'static str {
    TrackId::from_setting(settings.get(TRACK_SETTING))
        .layout()
        .thumbnail
}

/// The order racing's game logic runs in each `FixedUpdate` tick.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RacingSystems {
    /// Host only: run the countdown and the finish grace.
    Status,
    /// Move the cars from this tick's input, boosted by boost pads and slipstreams.
    Drive,
    /// Push touching cars apart, and bounce cars that hit each other.
    Bump,
    /// Host only: count checkpoints, laps and finishes from where the cars ended up.
    Checkpoints,
    /// Host only: end the race when it's over.
    Finish,
}

/// Registers racing's replicated components, input and game logic.
pub struct RacingPlugin;

impl Plugin for RacingPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedUpdate,
            (
                RacingSystems::Status,
                RacingSystems::Drive,
                RacingSystems::Bump,
                RacingSystems::Checkpoints,
                RacingSystems::Finish,
            )
                .chain(),
        );

        // Order matters: replicated types must be registered in the same order everywhere.
        app.add_plugins((
            car::CarPlugin,
            racing_input::RacingInputPlugin,
            progress::ProgressPlugin,
            race::RacePlugin,
            bumps::BumpPlugin,
        ));
    }
}
