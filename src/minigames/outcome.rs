//! What a minigame and the session share: who plays ([`Participants`]), and how it went
//! ([`MinigameEnded`], with a [`MinigameResult`]). A minigame reads the first and triggers the
//! second; it doesn't need anything else from the session.

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use crate::players::Player;

/// Who plays the current minigame, in seat order: Pong's left then right paddle, racing's grid
/// slots. In a [`Tournament`](crate::tournament::Tournament), the two in the current (or next)
/// match. Chosen by the host, kept next to the [`CurrentPhase`](crate::session::CurrentPhase) and
/// replicated, so every peer knows who plays and who watches. Minigames seat exactly these
/// players rather than picking their own.
#[derive(Component, Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Participants(pub Vec<PeerId>);

impl Participants {
    /// Whether `player` plays the current minigame.
    pub fn contains(&self, player: PeerId) -> bool {
        self.0.contains(&player)
    }
}

/// How the last minigame went, shown during
/// [`SessionPhase::Results`](crate::session::SessionPhase::Results). Spawned by the host and
/// replicated to everyone; removed when the results end.
#[derive(Component, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct MinigameResult {
    /// Everyone who played, best first. Players who only watched (such as Pong's spectators)
    /// aren't in it.
    pub placements: Vec<Placement>,
    /// A short summary of how the game ended, such as the final score. Can be empty.
    pub summary: String,
}

impl MinigameResult {
    /// Who came first: one player, or several if they tied.
    pub fn winners(&self) -> impl Iterator<Item = &Placement> {
        self.placements
            .iter()
            .filter(|placement| placement.place == 1)
    }
}

/// One player's place in a [`MinigameResult`].
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The [`Player::id`].
    pub player: PeerId,
    /// The player's name and [`Player::color`] when the minigame ended, so they can still be
    /// shown if the player has left since.
    pub name: String,
    pub color: u8,
    /// 1 for the winner, counting up. Players who tie share a place, and the next place is
    /// skipped (1st, 1st, 3rd).
    pub place: u32,
    /// A short note on how they got there, such as "DNF". Usually empty.
    pub note: String,
}

impl Placement {
    /// `player` in place `place`, with no note.
    pub fn new(player: &Player, place: u32) -> Self {
        Self {
            player: player.id,
            name: player.name.clone(),
            color: player.color,
            place,
            note: String::new(),
        }
    }

    /// For example "1st Ada" or "3rd Bo (DNF)".
    pub fn describe(&self) -> String {
        let mut line = format!("{} {}", ordinal(self.place), self.name);
        if !self.note.is_empty() {
            line.push_str(&format!(" ({})", self.note));
        }
        line
    }
}

/// `1st`, `2nd`, `3rd`, `4th`, …, `11th`, `12th`, `13th`, `21st`, …
pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Host only: trigger this when the current minigame is over. Triggering it outside a minigame,
/// or more than once, does nothing.
#[derive(Event, Debug, Clone, PartialEq, Eq)]
pub enum MinigameEnded {
    /// The game was played to the end: show the result and the standings, then go back to the
    /// lobby, or on to a [`Party`](crate::party::Party)'s next round. In a
    /// [`Tournament`](crate::tournament::Tournament), a match ended instead: the next match
    /// follows, or after the final, the tournament's result.
    Won(MinigameResult),
    /// The game can't go on (for example, a player left): go straight back to the lobby, or in
    /// a [`Party`](crate::party::Party), on to the round again with a new minigame. In a
    /// [`Tournament`](crate::tournament::Tournament), a player in the match who left loses it by
    /// walkover; if both are still here, the tournament ends.
    Abandoned,
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::PeerId;

    use super::{Placement, ordinal};

    #[test]
    fn describes_a_placement() {
        let mut placement = Placement {
            player: PeerId::Netcode(2),
            name: "Bo".into(),
            color: 0,
            place: 3,
            note: String::new(),
        };
        assert_eq!(placement.describe(), "3rd Bo");
        placement.note = "DNF".into();
        assert_eq!(placement.describe(), "3rd Bo (DNF)");
    }

    #[test]
    fn ordinals() {
        let shown: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111]
            .into_iter()
            .map(ordinal)
            .collect();
        assert_eq!(
            shown,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st",
                "111th"
            ]
        );
    }
}
