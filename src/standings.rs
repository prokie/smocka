//! Points across minigames. Each finished minigame gives every player who played it one point
//! for each player they beat, so the scale grows with the field: winning a two-player game is
//! worth 1, winning a four-car race 3. Players who tie share a place, and the points for it.
//! Points last for the session; a player who leaves loses theirs.

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use crate::players::Player;
use crate::session::{MinigameResult, Placement};

/// A player's points this session. Kept by the host on each [`Player`] entity and replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Points {
    pub total: u32,
    /// What the last finished minigame gave, to show next to the total. 0 for players who didn't
    /// play it.
    pub gained: u32,
}

/// Registers [`Points`] for replication. Must be added after
/// [`PlayersPlugin`](crate::players::PlayersPlugin).
pub struct StandingsPlugin;

impl Plugin for StandingsPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Points>().replicate();
    }
}

/// The points `placement` earns in `result`: one for every player placed below it.
pub fn points_for(placement: &Placement, result: &MinigameResult) -> u32 {
    result
        .placements
        .iter()
        .filter(|other| other.place > placement.place)
        .count() as u32
}

/// Host only: add what `result` earns to every player's [`Points`], and set what each gained.
pub fn award_points(result: &MinigameResult, players: &mut Query<(&Player, &mut Points)>) {
    for (player, mut points) in players.iter_mut() {
        let gained = result
            .placements
            .iter()
            .find(|placement| placement.player == player.id)
            .map_or(0, |placement| points_for(placement, result));
        points.total += gained;
        points.gained = gained;
        if gained > 0 {
            info!(
                "{} ({:?}) gains {gained}, now has {}",
                player.name(),
                player.id,
                points.total
            );
        }
    }
}

/// A player's place in the standings.
#[derive(Debug, Clone, PartialEq)]
pub struct Standing {
    /// 1 for the leader. Players with the same total share a place (1st, 1st, 3rd).
    pub place: u32,
    pub player: Player,
    pub points: Points,
}

/// Everyone ranked by total points, most first. Players with the same total share a place and
/// are listed in joining order.
pub fn standings(players: impl IntoIterator<Item = (Player, Points)>) -> Vec<Standing> {
    let mut players: Vec<(Player, Points)> = players.into_iter().collect();
    players.sort_by_key(|(player, points)| (std::cmp::Reverse(points.total), player.number));
    let totals: Vec<u32> = players.iter().map(|(_, points)| points.total).collect();
    players
        .into_iter()
        .map(|(player, points)| Standing {
            place: 1 + totals.iter().filter(|&&other| other > points.total).count() as u32,
            player,
            points,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::PeerId;

    use super::{Points, points_for, standings};
    use crate::players::Player;
    use crate::session::{MinigameResult, Placement};

    fn player(number: u32) -> Player {
        Player {
            id: PeerId::Netcode(number.into()),
            number,
            name: format!("Player {number}"),
            color: 0,
        }
    }

    fn result(places: &[u32]) -> MinigameResult {
        MinigameResult {
            placements: places
                .iter()
                .zip(1..)
                .map(|(&place, number)| Placement::new(&player(number), place))
                .collect(),
            summary: String::new(),
        }
    }

    fn points(result: &MinigameResult) -> Vec<u32> {
        result
            .placements
            .iter()
            .map(|placement| points_for(placement, result))
            .collect()
    }

    #[test]
    fn one_point_for_each_player_beaten() {
        assert_eq!(points(&result(&[1, 2])), [1, 0]);
        assert_eq!(points(&result(&[1, 2, 3, 4])), [3, 2, 1, 0]);
    }

    #[test]
    fn a_tie_shares_the_points() {
        assert_eq!(points(&result(&[1, 1, 3])), [1, 1, 0]);
        assert_eq!(points(&result(&[1, 2, 2])), [2, 0, 0]);
    }

    #[test]
    fn standings_rank_by_total_and_share_places() {
        let with = |number, total| (player(number), Points { total, gained: 0 });
        let ranked: Vec<(u32, u32)> = standings([with(1, 2), with(2, 5), with(3, 2), with(4, 0)])
            .iter()
            .map(|standing| (standing.place, standing.player.number))
            .collect();
        assert_eq!(ranked, [(1, 2), (2, 1), (2, 3), (4, 4)]);
    }
}
