//! The minigames. Each one lives in its own folder, with its own plugins, constants
//! (`config.rs`), a [`MinigameInfo`] called `INFO` (with the [`settings`] the host can
//! pick), and the [`SessionPhase`](crate::session::SessionPhase) it's played in, called `PHASE`.

pub mod outcome;
pub mod racing;
pub mod settings;
pub mod smoothing;
pub mod sounds;
pub mod two_player_pong;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A minigame the host can start from the lobby. This is also how peers name it to each other.
///
/// It's played in [`SessionPhase::Playing`](crate::session::SessionPhase::Playing). Adding a
/// minigame takes a variant here (and in [`ALL`](Self::ALL)), its module's `INFO` in
/// [`info`](Self::info), its plugins in [`MinigamesPlugin`] and [`MinigameVisualsPlugin`], and
/// the [`ControlAction`](crate::controls::ControlAction)s players bind for it.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Minigame {
    /// Two-player Pong: the two lowest-numbered players play, everyone else watches.
    TwoPlayerPong,
    /// A top-down race for every player.
    Racing,
}

/// What the session and the menus need to know about a minigame. Each minigame's module has one,
/// called `INFO`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinigameInfo {
    /// The name shown to players, for example on the lobby's start buttons.
    pub name: &'static str,
    /// The fewest players it can be started with.
    pub min_players: usize,
    /// The most players who can play it at once. With more in the session, the rest watch, or
    /// for a minigame that seats two, it's played as a
    /// [`Tournament`](crate::tournament::Tournament).
    pub max_players: usize,
    /// The picture on its tile in the lobby: a 16:9 image under `assets/`, made by
    /// `tools/make_art.py`.
    pub thumbnail: &'static str,
    /// What the host can pick before it's played, in the order its settings screen shows them.
    /// The minigame names each with a [`SettingId`](settings::SettingId) const.
    pub settings: &'static [settings::Setting],
}

impl Minigame {
    /// Every minigame, in the order the lobby offers them.
    pub const ALL: [Minigame; 2] = [Minigame::TwoPlayerPong, Minigame::Racing];

    /// What the session and the menus need to know about this minigame.
    pub const fn info(self) -> MinigameInfo {
        match self {
            Minigame::TwoPlayerPong => two_player_pong::INFO,
            Minigame::Racing => racing::INFO,
        }
    }
}

/// Replicated types must be registered in the same order on every peer: keep the plugin order.
pub struct MinigamesPlugin;

impl Plugin for MinigamesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            settings::SettingsPlugin,
            two_player_pong::TwoPlayerPongPlugin,
            racing::RacingPlugin,
        ));
    }
}

/// Local-only visuals and sounds. Separate from [`MinigamesPlugin`] so tests can run headless.
pub struct MinigameVisualsPlugin;

impl Plugin for MinigameVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            sounds::MinigameSoundsPlugin,
            two_player_pong::PongVisualsPlugin,
            racing::RacingVisualsPlugin,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{Minigame, racing, two_player_pong};
    use crate::session::SessionPhase;

    #[test]
    fn every_minigame_module_has_its_own_phase() {
        assert_eq!(
            two_player_pong::PHASE,
            SessionPhase::Playing(Minigame::TwoPlayerPong)
        );
        assert_eq!(racing::PHASE, SessionPhase::Playing(Minigame::Racing));
    }

    #[test]
    fn every_minigame_seats_at_least_its_minimum() {
        for game in Minigame::ALL {
            let info = game.info();
            assert!(info.min_players >= 2, "{game:?}");
            assert!(info.max_players >= info.min_players, "{game:?}");
        }
    }

    #[test]
    fn every_minigame_has_its_own_name() {
        for (i, a) in Minigame::ALL.iter().enumerate() {
            for b in &Minigame::ALL[i + 1..] {
                assert_ne!(a.info().name, b.info().name, "{a:?} and {b:?}");
            }
        }
    }
}
