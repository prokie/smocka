//! Lap counting. The host alone decides it, from where each car is after driving; clients see the
//! replicated result.

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::RacingSystems;
use super::car::{Car, CarState};
use super::track::{CHECKPOINTS, checkpoint_at};
use crate::config::minigames::racing::LAPS;
use crate::network::is_host;
use crate::session::SessionPhase;

/// How far round the track a car has got. Replicated, not predicted: it only changes on the host.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaceProgress {
    /// The index in [`CHECKPOINTS`] the car must reach next.
    pub next_checkpoint: usize,
    /// Laps completed.
    pub laps: u32,
}

impl RaceProgress {
    /// A car on the starting grid. The grid is just behind the start line, so the car counts as
    /// having passed checkpoint 0: crossing the line at the start isn't a lap.
    pub fn on_grid() -> Self {
        Self {
            next_checkpoint: 1,
            laps: 0,
        }
    }

    /// Progress after the car is seen in `checkpoint` (or in none). Only the next checkpoint
    /// counts, so skipping one or driving the wrong way gets the car nowhere.
    pub fn advance(self, checkpoint: Option<usize>) -> Self {
        match checkpoint {
            Some(reached) if reached == self.next_checkpoint => Self {
                next_checkpoint: (reached + 1) % CHECKPOINTS.len(),
                laps: if reached == 0 {
                    self.laps + 1
                } else {
                    self.laps
                },
            },
            _ => self,
        }
    }
}

/// What the lap counter shows for a car that has completed `laps` laps.
pub fn lap_label(laps: u32) -> String {
    if laps >= LAPS {
        "Finished".into()
    } else {
        format!("Lap {}/{LAPS}", laps + 1)
    }
}

/// Registers [`RaceProgress`] and counts laps on the host.
pub struct ProgressPlugin;

impl Plugin for ProgressPlugin {
    fn build(&self, app: &mut App) {
        app.component::<RaceProgress>().replicate();

        app.add_systems(
            FixedUpdate,
            count_laps
                .in_set(RacingSystems::Checkpoints)
                .run_if(is_host)
                .run_if(in_state(SessionPhase::Racing)),
        );
    }
}

/// Host only: move each car's progress on if it has reached its next checkpoint.
fn count_laps(mut cars: Query<(&Car, &CarState, &mut RaceProgress)>) {
    for (car, state, mut progress) in &mut cars {
        let next = progress.advance(checkpoint_at(state.position));
        if next.laps != progress.laps {
            debug!(
                "Player {} ({:?}) completed lap {}/{LAPS}",
                car.number, car.owner, next.laps
            );
        }
        progress.set_if_neq(next);
    }
}

#[cfg(test)]
mod tests {
    use super::{RaceProgress, lap_label};
    use crate::config::minigames::racing::LAPS;

    /// `progress` after reaching each of `checkpoints` in turn.
    fn reach(progress: RaceProgress, checkpoints: &[usize]) -> RaceProgress {
        checkpoints.iter().fold(progress, |progress, &checkpoint| {
            progress.advance(Some(checkpoint))
        })
    }

    #[test]
    fn crossing_the_line_at_the_start_isnt_a_lap() {
        assert_eq!(
            reach(RaceProgress::on_grid(), &[0]),
            RaceProgress::on_grid()
        );
    }

    #[test]
    fn reaching_every_checkpoint_in_order_completes_a_lap() {
        let progress = reach(RaceProgress::on_grid(), &[0, 1, 2, 3, 0]);
        assert_eq!(progress.laps, 1);
        assert_eq!(progress.next_checkpoint, 1);
        let progress = reach(progress, &[1, 2, 3, 0, 1, 2, 3, 0]);
        assert_eq!(progress.laps, 3);
    }

    #[test]
    fn staying_in_a_checkpoint_counts_it_once() {
        let progress = reach(RaceProgress::on_grid(), &[1, 1, 1, 2, 2, 3, 0, 0, 0]);
        assert_eq!(progress.laps, 1);
        assert_eq!(progress.next_checkpoint, 1);
    }

    #[test]
    fn skipping_a_checkpoint_isnt_a_lap() {
        let progress = reach(RaceProgress::on_grid(), &[1, 3, 0]);
        assert_eq!(progress.laps, 0);
        assert_eq!(progress.next_checkpoint, 2);
    }

    #[test]
    fn driving_the_wrong_way_isnt_a_lap() {
        let progress = reach(RaceProgress::on_grid(), &[3, 2, 1, 0]);
        assert_eq!(progress.laps, 0);
    }

    #[test]
    fn being_in_no_checkpoint_changes_nothing() {
        let progress = RaceProgress::on_grid();
        assert_eq!(progress.advance(None), progress);
    }

    #[test]
    fn the_label_counts_the_lap_being_driven() {
        assert_eq!(lap_label(0), format!("Lap 1/{LAPS}"));
        assert_eq!(lap_label(LAPS - 1), format!("Lap {LAPS}/{LAPS}"));
        assert_eq!(lap_label(LAPS), "Finished");
    }
}
