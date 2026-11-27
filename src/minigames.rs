pub mod racing;
pub mod smoothing;
pub mod two_player_pong;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::config::minigames::racing::GRID_SLOTS;
use crate::session::SessionPhase;

/// A minigame the host can start from the lobby.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Minigame {
    /// Two-player Pong: the two lowest-numbered players play, everyone else watches.
    TwoPlayerPong,
    /// A top-down race for every player.
    Racing,
}

impl Minigame {
    /// Every minigame, in the order the lobby offers them.
    pub const ALL: [Minigame; 2] = [Minigame::TwoPlayerPong, Minigame::Racing];

    /// The [`SessionPhase`] this minigame is played in.
    pub fn phase(self) -> SessionPhase {
        match self {
            Minigame::TwoPlayerPong => SessionPhase::TwoPlayerPong,
            Minigame::Racing => SessionPhase::Racing,
        }
    }

    /// The fewest players this minigame can be started with.
    pub fn min_players(self) -> usize {
        match self {
            Minigame::TwoPlayerPong | Minigame::Racing => 2,
        }
    }

    /// The most players who can play it at once. With more in the session, the rest watch.
    pub fn max_players(self) -> usize {
        match self {
            Minigame::TwoPlayerPong => 2,
            Minigame::Racing => GRID_SLOTS,
        }
    }

    /// The name shown to players, for example on the lobby's start buttons.
    pub fn name(self) -> &'static str {
        match self {
            Minigame::TwoPlayerPong => "Pong",
            Minigame::Racing => "Racing",
        }
    }
}

/// Replicated types must be registered in the same order on every peer: keep the plugin order.
pub struct MinigamesPlugin;

impl Plugin for MinigamesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((two_player_pong::TwoPlayerPongPlugin, racing::RacingPlugin));
    }
}

/// Local-only visuals. Separate from [`MinigamesPlugin`] so tests can run headless.
pub struct MinigameVisualsPlugin;

impl Plugin for MinigameVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            two_player_pong::PongVisualsPlugin,
            racing::RacingVisualsPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::Minigame;

    #[test]
    fn every_minigame_is_played_in_a_minigame_phase() {
        for game in Minigame::ALL {
            assert!(game.phase().is_minigame(), "{game:?}");
        }
    }

    #[test]
    fn every_minigame_seats_at_least_its_minimum() {
        for game in Minigame::ALL {
            assert!(game.min_players() >= 2, "{game:?}");
            assert!(game.max_players() >= game.min_players(), "{game:?}");
        }
    }

    #[test]
    fn every_minigame_has_its_own_phase() {
        for (i, a) in Minigame::ALL.iter().enumerate() {
            for b in &Minigame::ALL[i + 1..] {
                assert_ne!(a.phase(), b.phase(), "{a:?} and {b:?}");
            }
        }
    }
}
