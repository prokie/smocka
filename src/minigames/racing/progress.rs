//! Lap counting and finishing. The host alone decides it, from where each car is after driving;
//! clients see the replicated result.

use std::cmp::Reverse;

use bevy::math::FloatOrd;
use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::RacingSystems;
use super::car::{Car, CarState};
use super::race::{RaceDirector, ordinal};
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
    /// Where the car finished, counting from 1, once it has completed every lap.
    pub place: Option<u32>,
}

impl RaceProgress {
    /// A car on the starting grid. The grid is just behind the start line, so the car counts as
    /// having passed checkpoint 0: crossing the line at the start isn't a lap.
    pub fn on_grid() -> Self {
        Self {
            next_checkpoint: 1,
            laps: 0,
            place: None,
        }
    }

    /// Progress after the car is seen in `checkpoint` (or in none). Only the next checkpoint
    /// counts, so skipping one or driving the wrong way gets the car nowhere. A finished car
    /// stays as it is.
    pub fn advance(self, checkpoint: Option<usize>) -> Self {
        match checkpoint {
            Some(reached) if reached == self.next_checkpoint && self.place.is_none() => Self {
                next_checkpoint: (reached + 1) % CHECKPOINTS.len(),
                laps: if reached == 0 {
                    self.laps + 1
                } else {
                    self.laps
                },
                ..self
            },
            _ => self,
        }
    }

    /// How many checkpoints the car has reached since the start, to rank cars that didn't finish.
    pub fn checkpoints_passed(self) -> u32 {
        let count = CHECKPOINTS.len();
        self.laps * count as u32 + ((self.next_checkpoint + count - 1) % count) as u32
    }
}

/// Sort key for the running order: lower is further ahead. Finished cars come first, by place;
/// the rest by how many checkpoints they've passed, then by how close they are to the next one.
pub fn running_order(progress: RaceProgress, position: Vec2) -> (u32, Reverse<u32>, FloatOrd) {
    let next = CHECKPOINTS[progress.next_checkpoint].center();
    (
        progress.place.unwrap_or(u32::MAX),
        Reverse(progress.checkpoints_passed()),
        FloatOrd(position.distance(next)),
    )
}

/// What the position display shows for a car in `position` (from 1) of `cars`.
pub fn position_label(position: usize, cars: usize) -> String {
    format!("{} / {cars}", ordinal(position as u32))
}

/// What the lap counter shows for a car with `progress`.
pub fn lap_label(progress: RaceProgress) -> String {
    match progress.place {
        Some(place) => format!("Finished {}", ordinal(place)),
        None => format!("Lap {}/{LAPS}", (progress.laps + 1).min(LAPS)),
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
                .run_if(resource_exists::<RaceDirector>)
                .run_if(in_state(SessionPhase::Racing)),
        );
    }
}

/// Host only: move each car's progress on if it has reached its next checkpoint, and give it the
/// next place when it completes its last lap.
fn count_laps(
    mut cars: Query<(&Car, &CarState, &mut RaceProgress)>,
    mut director: ResMut<RaceDirector>,
) {
    for (car, state, mut progress) in &mut cars {
        let mut next = progress.advance(checkpoint_at(state.position));
        if next.laps != progress.laps {
            debug!(
                "{} ({:?}) completed lap {}/{LAPS}",
                car.name, car.owner, next.laps
            );
            if next.laps >= LAPS {
                let place = director.finish(car);
                info!("{} ({:?}) finished {}", car.name, car.owner, ordinal(place));
                next.place = Some(place);
            }
        }
        progress.set_if_neq(next);
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;

    use super::{RaceProgress, lap_label, position_label, running_order};
    use crate::config::minigames::racing::LAPS;
    use crate::minigames::racing::track::CHECKPOINTS;

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
    fn the_label_counts_the_lap_being_driven_then_the_place() {
        let on_lap = |laps| RaceProgress {
            laps,
            ..RaceProgress::on_grid()
        };
        assert_eq!(lap_label(on_lap(0)), format!("Lap 1/{LAPS}"));
        assert_eq!(lap_label(on_lap(LAPS - 1)), format!("Lap {LAPS}/{LAPS}"));
        let finished = RaceProgress {
            place: Some(2),
            ..on_lap(LAPS)
        };
        assert_eq!(lap_label(finished), "Finished 2nd");
    }

    #[test]
    fn a_finished_car_stops_counting() {
        let finished = RaceProgress {
            next_checkpoint: 1,
            laps: LAPS,
            place: Some(1),
        };
        assert_eq!(reach(finished, &[1, 2, 3, 0]), finished);
    }

    #[test]
    fn the_running_order_ranks_places_then_checkpoints_then_distance() {
        let at_next = |progress: RaceProgress| CHECKPOINTS[progress.next_checkpoint].center();
        let grid = RaceProgress::on_grid();
        let one_ahead = reach(grid, &[1]);
        let finished = RaceProgress {
            place: Some(2),
            ..reach(grid, &[1, 2, 3, 0])
        };
        let near = running_order(grid, at_next(grid));
        let far = running_order(grid, at_next(grid) + Vec2::new(-50.0, 0.0));

        assert!(near < far, "closer to the next checkpoint is ahead");
        assert!(
            running_order(one_ahead, Vec2::ZERO) < near,
            "more checkpoints is ahead"
        );
        assert!(running_order(finished, Vec2::ZERO) < running_order(one_ahead, Vec2::ZERO));
    }

    #[test]
    fn the_position_label_shows_place_and_field() {
        assert_eq!(position_label(2, 4), "2nd / 4");
    }

    #[test]
    fn checkpoints_passed_counts_from_the_grid() {
        let grid = RaceProgress::on_grid();
        assert_eq!(grid.checkpoints_passed(), 0);
        assert_eq!(reach(grid, &[1]).checkpoints_passed(), 1);
        assert_eq!(reach(grid, &[1, 2, 3]).checkpoints_passed(), 3);
        assert_eq!(reach(grid, &[1, 2, 3, 0]).checkpoints_passed(), 4);
        assert_eq!(reach(grid, &[1, 2, 3, 0, 1]).checkpoints_passed(), 5);
    }
}
