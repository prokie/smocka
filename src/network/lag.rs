//! Pretend the network is worse than it is, to see how the game holds up over the internet. With
//! `--lag`, every packet this app receives is held back, jittered, and now and then dropped
//! (Lightyear's link conditioner), whether it hosts or joins.
//!
//! A conditioner only works on what its own app receives, so each player's `--lag` adds half of
//! the round trip it names. Give it to every player (`tools/dev.sh` does, with `SMOCKA_LAG`) for
//! the whole connection. The host's own player is on the same machine as the session, so it never
//! lags: only joiners do, as for real.

use std::fmt;
use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::{LinkConditionerConfig, RecvLinkConditioner};

/// A connection `--lag` can pretend to have, named after Lightyear's presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LagPreset {
    /// About 40 ms there and back, give or take 6 ms, and 0.2% of packets lost.
    Good,
    /// About 100 ms there and back, give or take 15 ms, and 2% of packets lost. A player in
    /// another country.
    #[default]
    Average,
    /// About 200 ms there and back, give or take 30 ms, and 10% of packets lost. Bad Wi-Fi on the
    /// other side of the world.
    Poor,
}

impl LagPreset {
    /// The preset `--lag=<name>` names: "good", "average" or "poor", in any case.
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "good" => Some(Self::Good),
            "average" => Some(Self::Average),
            "poor" => Some(Self::Poor),
            _ => None,
        }
    }

    /// What one player's app applies to the packets it receives: half of Lightyear's preset of the
    /// same name, so with every player using it, a round trip matches the preset.
    pub fn conditions(self) -> LinkConditionerConfig {
        match self {
            Self::Good => LinkConditionerConfig::good_condition(),
            Self::Average => LinkConditionerConfig::average_condition(),
            Self::Poor => LinkConditionerConfig::poor_condition(),
        }
        .half()
    }
}

impl fmt::Display for LagPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Good => "good",
            Self::Average => "average",
            Self::Poor => "poor",
        })
    }
}

/// Set (by `--lag`) to make this app's connection worse than it is: [`HostSession`] and
/// [`JoinSession`] give every link they make a conditioner with these conditions.
///
/// [`HostSession`]: super::HostSession
/// [`JoinSession`]: super::JoinSession
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct SimulatedLag(pub LinkConditionerConfig);

impl From<LagPreset> for SimulatedLag {
    fn from(preset: LagPreset) -> Self {
        Self(preset.conditions())
    }
}

impl SimulatedLag {
    /// How it's described in the log, e.g. "packets held back 50 ms ± 7 ms, 1.0% lost".
    pub fn describe(&self) -> String {
        let ms = |duration: Duration| duration.as_millis();
        format!(
            "packets held back {} ms ± {} ms, {:.1}% lost",
            ms(self.0.incoming_latency),
            ms(self.0.incoming_jitter),
            self.0.mean_loss() * 100.0
        )
    }
}

/// A fresh conditioner for a new link, if this app simulates lag.
pub(super) fn receive_conditioner(world: &World) -> Option<RecvLinkConditioner> {
    world
        .get_resource::<SimulatedLag>()
        .map(|lag| RecvLinkConditioner::new(lag.0.clone()))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{LagPreset, SimulatedLag};

    #[test]
    fn presets_are_named_in_any_case() {
        assert_eq!(LagPreset::parse("good"), Some(LagPreset::Good));
        assert_eq!(LagPreset::parse("Average"), Some(LagPreset::Average));
        assert_eq!(LagPreset::parse("POOR"), Some(LagPreset::Poor));
        assert_eq!(LagPreset::parse("awful"), None);
        assert_eq!(LagPreset::parse(""), None);
        for preset in [LagPreset::Good, LagPreset::Average, LagPreset::Poor] {
            assert_eq!(LagPreset::parse(&preset.to_string()), Some(preset));
        }
    }

    #[test]
    fn each_player_adds_half_the_round_trip() {
        let average = LagPreset::Average.conditions();
        assert_eq!(average.incoming_latency, Duration::from_millis(50));
        assert!(
            LagPreset::Good.conditions().incoming_latency < average.incoming_latency
                && average.incoming_latency < LagPreset::Poor.conditions().incoming_latency
        );
    }

    #[test]
    fn describes_itself_for_the_log() {
        let average = SimulatedLag::from(LagPreset::Average);
        assert_eq!(
            average.describe(),
            "packets held back 50 ms ± 7 ms, 1.0% lost"
        );
    }
}
