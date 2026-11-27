//! Lap counting and finishing. The host alone decides it, from where each car is after driving;
//! clients see the replicated result.

use std::cmp::Reverse;

use bevy::ecs::entity::EntityHashMap;
use bevy::math::FloatOrd;
use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PHASE;
use super::RacingSystems;
use super::car::{Car, CarState};
use super::race::{RaceDirector, lap_time_label};
use super::track::Track;
use crate::minigames::outcome::ordinal;
use crate::network::is_host;

/// How far round the track a car has got. Replicated, not predicted: it only changes on the host.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaceProgress {
    /// The index in the race's [`Track::checkpoints`] the car must reach next.
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

    /// Progress after the car is seen in `checkpoint` (or in none) of `track`. Only the next
    /// checkpoint counts, so skipping one or driving the wrong way gets the car nowhere. A
    /// finished car stays as it is.
    pub fn advance(self, checkpoint: Option<usize>, track: &Track) -> Self {
        match checkpoint {
            Some(reached) if reached == self.next_checkpoint && self.place.is_none() => Self {
                next_checkpoint: (reached + 1) % track.checkpoints().len(),
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

    /// How many of `track`'s checkpoints the car has reached since the start, to rank cars that
    /// didn't finish.
    pub fn checkpoints_passed(self, track: &Track) -> u32 {
        let count = track.checkpoints().len();
        self.laps * count as u32 + ((self.next_checkpoint + count - 1) % count) as u32
    }
}

/// Sort key for the running order: lower is further ahead. Finished cars come first, by place;
/// the rest by how many checkpoints they've passed, then by how close they are to the next one.
pub fn running_order(
    track: &Track,
    progress: RaceProgress,
    position: Vec2,
) -> (u32, Reverse<u32>, FloatOrd) {
    let next = track.checkpoints()[progress.next_checkpoint].center();
    (
        progress.place.unwrap_or(u32::MAX),
        Reverse(progress.checkpoints_passed(track)),
        FloatOrd(position.distance(next)),
    )
}

/// What the position display shows for a car in `position` (from 1) of `cars`.
pub fn position_label(position: usize, cars: usize) -> String {
    format!("{} / {cars}", ordinal(position as u32))
}

/// What the lap counter shows for a car with `progress`, in a race of `laps` laps.
pub fn lap_label(progress: RaceProgress, laps: u32) -> String {
    match progress.place {
        Some(place) => format!("Finished {}", ordinal(place)),
        None => format!("Lap {}/{laps}", (progress.laps + 1).min(laps)),
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
                .run_if(in_state(PHASE)),
        );
    }
}

/// Host only: move each car's progress on if it has reached its next checkpoint, and give it the
/// next place when it completes its last lap. `inside` remembers the checkpoint each car was in on
/// the last tick, so entering one is logged once rather than every tick spent in it.
fn count_laps(
    mut cars: Query<(Entity, &Car, &CarState, &mut RaceProgress)>,
    mut director: ResMut<RaceDirector>,
    mut inside: Local<EntityHashMap<Option<usize>>>,
) {
    inside.retain(|entity, _| cars.contains(*entity));
    let track = director.track();
    for (entity, car, state, mut progress) in &mut cars {
        let checkpoint = track.checkpoint_at(state.position);
        let entered = inside.insert(entity, checkpoint) != Some(checkpoint);
        let mut next = progress.advance(checkpoint, track);
        match checkpoint {
            Some(reached) if entered && next != *progress => debug!(
                "{} ({:?}) reached checkpoint {reached} on lap {}",
                car.name,
                car.owner,
                progress.laps + 1
            ),
            Some(reached) if entered && progress.place.is_none() => debug!(
                "{} ({:?}) entered checkpoint {reached} out of order: needs checkpoint {} \
                 first, so this doesn't count",
                car.name, car.owner, progress.next_checkpoint
            ),
            _ => {}
        }
        if next.laps != progress.laps {
            let lap_time = director.complete_lap(car);
            debug!(
                "{} ({:?}) completed lap {}/{} in {}",
                car.name,
                car.owner,
                next.laps,
                director.laps(),
                lap_time_label(lap_time)
            );
            if next.laps >= director.laps() {
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
    use crate::minigames::racing::track::{Track, TrackId};

    const LAPS: u32 = 3;

    /// The track these tests race on.
    fn track() -> &'static Track {
        TrackId::default().track()
    }

    /// The checkpoints of one lap from the grid, in order, ending on the line.
    fn lap() -> Vec<usize> {
        (1..track().checkpoints().len()).chain([0]).collect()
    }

    /// `progress` after reaching each of `checkpoints` in turn.
    fn reach(progress: RaceProgress, checkpoints: &[usize]) -> RaceProgress {
        checkpoints.iter().fold(progress, |progress, &checkpoint| {
            progress.advance(Some(checkpoint), track())
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
        // Over the line from the grid first, which doesn't count.
        let progress = reach(RaceProgress::on_grid(), &[vec![0], lap()].concat());
        assert_eq!(progress.laps, 1);
        assert_eq!(progress.next_checkpoint, 1);
        let progress = reach(progress, &lap().repeat(2));
        assert_eq!(progress.laps, 3);
    }

    #[test]
    fn staying_in_a_checkpoint_counts_it_once() {
        let lingering: Vec<usize> = lap().into_iter().flat_map(|c| [c, c, c]).collect();
        let progress = reach(RaceProgress::on_grid(), &lingering);
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
        let backwards: Vec<usize> = lap().into_iter().rev().collect();
        let progress = reach(RaceProgress::on_grid(), &backwards);
        assert_eq!(progress.laps, 0);
    }

    #[test]
    fn being_in_no_checkpoint_changes_nothing() {
        let progress = RaceProgress::on_grid();
        assert_eq!(progress.advance(None, track()), progress);
    }

    #[test]
    fn the_label_counts_the_lap_being_driven_then_the_place() {
        let on_lap = |laps| RaceProgress {
            laps,
            ..RaceProgress::on_grid()
        };
        assert_eq!(lap_label(on_lap(0), LAPS), "Lap 1/3");
        assert_eq!(lap_label(on_lap(LAPS - 1), LAPS), "Lap 3/3");
        assert_eq!(lap_label(on_lap(0), 1), "Lap 1/1");
        let finished = RaceProgress {
            place: Some(2),
            ..on_lap(LAPS)
        };
        assert_eq!(lap_label(finished, LAPS), "Finished 2nd");
    }

    #[test]
    fn a_finished_car_stops_counting() {
        let finished = RaceProgress {
            next_checkpoint: 1,
            laps: LAPS,
            place: Some(1),
        };
        assert_eq!(reach(finished, &lap()), finished);
    }

    #[test]
    fn the_running_order_ranks_places_then_checkpoints_then_distance() {
        let at_next =
            |progress: RaceProgress| track().checkpoints()[progress.next_checkpoint].center();
        let running_order = |progress, position| running_order(track(), progress, position);
        let grid = RaceProgress::on_grid();
        let one_ahead = reach(grid, &[1]);
        let finished = RaceProgress {
            place: Some(2),
            ..reach(grid, &lap())
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
        let passed = |progress: RaceProgress| progress.checkpoints_passed(track());
        assert_eq!(passed(grid), 0);
        assert_eq!(passed(reach(grid, &[1])), 1);
        assert_eq!(passed(reach(grid, &[1, 2, 3])), 3);
        let count = track().checkpoints().len() as u32;
        assert_eq!(passed(reach(grid, &lap())), count);
        assert_eq!(passed(reach(grid, &[lap(), vec![1]].concat())), count + 1);
    }
}
