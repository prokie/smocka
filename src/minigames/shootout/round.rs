//! When a shootout ends: knockouts, placements and abandoning if everyone else leaves.
//!
//! The host keeps [`ShootoutDirector`] for the round, recording each knockout with the tick it
//! happened on so players knocked out on the same tick share a place. Once at most one gunner
//! remains, the shootout ends — won if at least one knockout happened, or abandoned if every
//! other player simply left.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::Tick;

use super::config::{
    PICKUP_FIRST_DELAY, PICKUP_INTERVAL, SUDDEN_DEATH_DELAY, SUDDEN_DEATH_SHRINK,
    ZONE_DAMAGE_INTERVAL,
};
use super::gunner::Gunner;
use super::{PHASE, ShootoutSystems};
use crate::minigames::outcome::{MinigameEnded, MinigameResult, Placement};
use crate::network::is_host;

/// Host only: timing for centre pickups and sudden death in the next shootout. Defaults to the
/// constants in [`config`](super::config); tests can shorten them without waiting tens of seconds.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShootoutRules {
    pub pickup_first_delay: Duration,
    pub pickup_interval: Duration,
    pub sudden_death_delay: Duration,
    pub sudden_death_shrink: Duration,
    pub zone_damage_interval: Duration,
}

impl Default for ShootoutRules {
    fn default() -> Self {
        Self {
            pickup_first_delay: PICKUP_FIRST_DELAY,
            pickup_interval: PICKUP_INTERVAL,
            sudden_death_delay: SUDDEN_DEATH_DELAY,
            sudden_death_shrink: SUDDEN_DEATH_SHRINK,
            zone_damage_interval: ZONE_DAMAGE_INTERVAL,
        }
    }
}

/// Host only: knockouts so far in this shootout. Exists while playing.
#[derive(Resource, Debug, Default)]
pub struct ShootoutDirector {
    /// Knocked-out gunners in the order they went out, with the tick they were knocked out on.
    knocked_out: Vec<(Gunner, Tick)>,
    /// Set once the shootout has handed control back to the session, so it only does that once.
    over: bool,
}

impl ShootoutDirector {
    /// Record that `gunner` was knocked out on `tick`.
    pub fn record_knockout(&mut self, gunner: Gunner, tick: Tick) {
        self.knocked_out.push((gunner, tick));
    }
}

/// Host only: ends the shootout.
pub struct RoundPlugin;

impl Plugin for RoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShootoutRules>();
        app.add_systems(OnEnter(PHASE), start_round.run_if(is_host));
        app.add_systems(OnExit(PHASE), |mut commands: Commands| {
            commands.remove_resource::<ShootoutDirector>();
        });
        app.add_systems(
            FixedUpdate,
            end_shootout
                .in_set(ShootoutSystems::Finish)
                .run_if(is_host)
                .run_if(resource_exists::<ShootoutDirector>)
                .run_if(in_state(PHASE)),
        );
    }
}

fn start_round(mut commands: Commands) {
    commands.insert_resource(ShootoutDirector::default());
}

/// Host only: once at most one gunner is left, either finish the shootout with placements or
/// abandon it if nobody was knocked out (meaning every other player left).
fn end_shootout(
    mut director: ResMut<ShootoutDirector>,
    gunners: Query<&Gunner>,
    mut commands: Commands,
) {
    if director.over {
        return;
    }
    let survivors: Vec<Gunner> = gunners.iter().cloned().collect();
    if survivors.len() >= 2 {
        return;
    }
    director.over = true;

    if director.knocked_out.is_empty() {
        match survivors.first() {
            Some(gunner) => info!(
                "Abandoning the shootout: every other player left, leaving {} ({:?}) alone",
                gunner.name, gunner.owner
            ),
            None => info!("Abandoning the shootout: every player has left"),
        }
        commands.trigger(MinigameEnded::Abandoned);
        return;
    }

    let placements = shootout_placements(&survivors, &director.knocked_out);
    let summary = shootout_summary(survivors.len());
    let winners: Vec<&str> = placements
        .iter()
        .filter(|placement| placement.place == 1)
        .map(|placement| placement.name.as_str())
        .collect();
    info!("Shootout over ({summary}): winner(s) {winners:?}");
    commands.trigger(MinigameEnded::Won(MinigameResult {
        placements,
        summary,
    }));
}

/// The results screen's line under the headline.
pub fn shootout_summary(survivors: usize) -> String {
    if survivors == 0 {
        "No survivors".into()
    } else {
        "Last one standing".into()
    }
}

/// Final placements: any surviving gunner(s) first, then knocked-out gunners from last-out to
/// first-out. Players knocked out on the same tick share a place, and the next place skips ahead.
pub fn shootout_placements(survivors: &[Gunner], knocked_out: &[(Gunner, Tick)]) -> Vec<Placement> {
    let mut placements = Vec::with_capacity(survivors.len() + knocked_out.len());
    for gunner in survivors {
        placements.push(placement_for(gunner, 1));
    }

    let mut eliminated: Vec<&(Gunner, Tick)> = knocked_out.iter().collect();
    // Latest knockout first; preserve insertion order within the same tick.
    eliminated.sort_by(|(_, a), (_, b)| b.cmp(a));

    let mut previous_tick: Option<Tick> = None;
    let mut current_place = (survivors.len() + 1) as u32;
    for (index, (gunner, tick)) in eliminated.into_iter().enumerate() {
        if previous_tick != Some(*tick) {
            current_place = (survivors.len() + index + 1) as u32;
            previous_tick = Some(*tick);
        }
        placements.push(placement_for(gunner, current_place));
    }
    placements
}

fn placement_for(gunner: &Gunner, place: u32) -> Placement {
    Placement {
        player: gunner.owner,
        name: gunner.name.clone(),
        color: gunner.color,
        place,
        note: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::{PeerId, Tick};

    use super::{shootout_placements, shootout_summary};
    use crate::minigames::shootout::gunner::Gunner;

    fn gunner(id: u64, name: &str) -> Gunner {
        Gunner {
            owner: PeerId::Netcode(id),
            name: name.into(),
            color: id as u8,
        }
    }

    #[test]
    fn survivor_wins_and_knocked_out_place_from_last_out_to_first() {
        let placements = shootout_placements(
            &[gunner(1, "Ada")],
            &[(gunner(2, "Bo"), Tick(30)), (gunner(3, "Cy"), Tick(60))],
        );
        let labels: Vec<String> = placements.iter().map(|p| p.describe()).collect();
        assert_eq!(labels, ["1st Ada", "2nd Cy", "3rd Bo"]);
        assert_eq!(shootout_summary(1), "Last one standing");
    }

    #[test]
    fn simultaneous_knockouts_share_a_place() {
        // Double knockout leaves no survivors: both tie for 1st.
        let tie_win = shootout_placements(
            &[],
            &[(gunner(1, "Ada"), Tick(50)), (gunner(2, "Bo"), Tick(50))],
        );
        let labels: Vec<String> = tie_win.iter().map(|p| p.describe()).collect();
        assert_eq!(labels, ["1st Ada", "1st Bo"]);
        assert_eq!(shootout_summary(0), "No survivors");

        // Mid-round double knockout shares 2nd and skips 3rd.
        let placements = shootout_placements(
            &[gunner(1, "Ada")],
            &[
                (gunner(4, "Di"), Tick(20)),
                (gunner(2, "Bo"), Tick(40)),
                (gunner(3, "Cy"), Tick(40)),
            ],
        );
        let labels: Vec<String> = placements.iter().map(|p| p.describe()).collect();
        assert_eq!(labels, ["1st Ada", "2nd Bo", "2nd Cy", "4th Di"]);
    }
}
