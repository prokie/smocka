//! The race as a whole: the countdown, the finishing order, and when the race ends. The host
//! decides all of it; clients see the replicated [`RaceStatus`] and each car's [`RaceProgress`].

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::RacingSystems;
use super::car::{Car, CarState};
use super::progress::{RaceProgress, running_order};
use crate::config::minigames::racing::{COUNTDOWN, FINISH_GRACE};
use crate::network::is_host;
use crate::session::{MinigameEnded, MinigameResult, Placement, SessionPhase};

/// Where the race is. One replicated entity per race, spawned by the host.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaceStatus {
    /// The cars wait on the grid. The race starts in this many seconds, rounded up.
    Countdown(u32),
    Racing,
    /// Someone has finished. The race ends in this many seconds, rounded up, unless every car
    /// finishes first. Cars still drive.
    Finishing(u32),
}

impl RaceStatus {
    pub fn cars_may_drive(self) -> bool {
        !matches!(self, RaceStatus::Countdown(_))
    }
}

/// Host only: how long the countdown and the finish grace last.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaceRules {
    /// Defaults to [`COUNTDOWN`].
    pub countdown: Duration,
    /// Defaults to [`FINISH_GRACE`].
    pub finish_grace: Duration,
}

impl Default for RaceRules {
    fn default() -> Self {
        Self {
            countdown: COUNTDOWN,
            finish_grace: FINISH_GRACE,
        }
    }
}

/// Host only: the race's clocks and finishing order. Exists while racing.
#[derive(Resource, Debug)]
pub struct RaceDirector {
    countdown: Timer,
    finish_grace: Duration,
    /// Started by the first finish.
    grace: Option<Timer>,
    /// The cars that crossed the line on their last lap, in finishing order. Players who leave
    /// after finishing stay in it.
    finishers: Vec<Car>,
    /// Set once the race has handed control back to the session, so it only does that once.
    over: bool,
}

impl RaceDirector {
    fn new(rules: &RaceRules) -> Self {
        Self {
            countdown: Timer::new(rules.countdown, TimerMode::Once),
            finish_grace: rules.finish_grace,
            grace: None,
            finishers: Vec::new(),
            over: false,
        }
    }

    /// Record `car` as finished and return its place, counting from 1. The first finish starts
    /// the grace period.
    pub fn finish(&mut self, car: &Car) -> u32 {
        if self.grace.is_none() {
            self.grace = Some(Timer::new(self.finish_grace, TimerMode::Once));
        }
        self.finishers.push(car.clone());
        self.finishers.len() as u32
    }

    /// Advance the clocks by `delta` and return the status they give.
    fn tick(&mut self, delta: Duration) -> RaceStatus {
        self.countdown.tick(delta);
        if !self.countdown.is_finished() {
            return RaceStatus::Countdown(whole_seconds(self.countdown.remaining()));
        }
        match &mut self.grace {
            Some(grace) => {
                grace.tick(delta);
                RaceStatus::Finishing(whole_seconds(grace.remaining()))
            }
            None => RaceStatus::Racing,
        }
    }

    fn grace_is_over(&self) -> bool {
        self.grace.as_ref().is_some_and(Timer::is_finished)
    }
}

fn whole_seconds(duration: Duration) -> u32 {
    duration.as_secs_f32().ceil() as u32
}

/// Registers [`RaceStatus`] and runs the race on the host.
pub struct RacePlugin;

impl Plugin for RacePlugin {
    fn build(&self, app: &mut App) {
        app.component::<RaceStatus>().replicate();
        app.init_resource::<RaceRules>();

        app.add_systems(OnEnter(SessionPhase::Racing), start_race.run_if(is_host));
        app.add_systems(OnExit(SessionPhase::Racing), |mut commands: Commands| {
            commands.remove_resource::<RaceDirector>();
        });
        app.add_systems(
            FixedUpdate,
            (
                run_race_clock.in_set(RacingSystems::Status),
                end_race.in_set(RacingSystems::Finish),
            )
                .run_if(is_host)
                .run_if(resource_exists::<RaceDirector>)
                .run_if(in_state(SessionPhase::Racing)),
        );
    }
}

/// Host only: when the race starts, spawn its status and start the countdown.
fn start_race(rules: Res<RaceRules>, mut commands: Commands) {
    info!(
        "Race starts in {} s",
        rules.countdown.as_secs_f32().ceil() as u32
    );
    commands.spawn((
        RaceStatus::Countdown(whole_seconds(rules.countdown)),
        DespawnOnExit(SessionPhase::Racing),
        Replicate::to_clients(NetworkTarget::All),
    ));
    commands.insert_resource(RaceDirector::new(&rules));
}

/// Host only: run the countdown and the finish grace, and publish the status they give.
fn run_race_clock(
    time: Res<Time>,
    mut director: ResMut<RaceDirector>,
    mut status: Single<&mut RaceStatus>,
) {
    let next = director.tick(time.delta());
    if **status != next {
        match (**status, next) {
            (RaceStatus::Countdown(_), RaceStatus::Racing) => info!("Race started"),
            (RaceStatus::Racing, RaceStatus::Finishing(seconds)) => {
                info!("First car finished: the race ends in {seconds} s unless everyone finishes")
            }
            _ => debug!("Race status {next:?}"),
        }
    }
    status.set_if_neq(next);
}

/// Host only: end the race once every car still in it has finished, or the grace period after the
/// first finish runs out. If every car has gone, abandon it.
fn end_race(
    mut director: ResMut<RaceDirector>,
    cars: Query<(&Car, &RaceProgress, &CarState)>,
    mut commands: Commands,
) {
    if director.over {
        return;
    }
    if cars.is_empty() {
        info!("Abandoning the race: every car's player has left");
        director.over = true;
        commands.trigger(MinigameEnded::Abandoned);
        return;
    }
    let everyone_finished = cars.iter().all(|(_, progress, _)| progress.place.is_some());
    if !everyone_finished && !director.grace_is_over() {
        return;
    }
    if director.finishers.is_empty() {
        return;
    }
    let unfinished = cars
        .iter()
        .filter(|(_, progress, _)| progress.place.is_none())
        .map(|(car, progress, state)| (car.clone(), *progress, state.position))
        .collect();
    let placements = race_placements(&director.finishers, unfinished);
    let summary = if everyone_finished {
        "Every car finished"
    } else {
        "Time ran out for the rest"
    };
    info!("Race over: {summary}");
    director.over = true;
    commands.trigger(MinigameEnded::Won(MinigameResult {
        placements,
        summary: summary.into(),
    }));
}

/// What the middle of the screen shows during `status`. `just_started` is whether the race began a
/// moment ago, to show "GO!".
pub fn status_label(status: RaceStatus, just_started: bool) -> String {
    match status {
        RaceStatus::Countdown(seconds) => seconds.max(1).to_string(),
        RaceStatus::Racing if just_started => "GO!".into(),
        RaceStatus::Racing => String::new(),
        RaceStatus::Finishing(seconds) => format!("Race ends in {seconds}"),
    }
}

/// The final order: the finishers in the order they finished, then the cars that didn't finish
/// (marked "DNF"), in [`running_order`].
pub fn race_placements(
    finishers: &[Car],
    mut unfinished: Vec<(Car, RaceProgress, Vec2)>,
) -> Vec<Placement> {
    unfinished.sort_by_key(|(_, progress, position)| running_order(*progress, *position));
    finishers
        .iter()
        .map(|car| (car, ""))
        .chain(unfinished.iter().map(|(car, _, _)| (car, "DNF")))
        .zip(1..)
        .map(|((car, note), place)| Placement {
            player: car.owner,
            name: car.name.clone(),
            color: car.color,
            place,
            note: note.into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::math::Vec2;
    use lightyear::prelude::PeerId;

    use super::{Car, RaceDirector, RaceRules, RaceStatus, race_placements, status_label};
    use crate::minigames::racing::progress::RaceProgress;

    fn car(number: u32) -> Car {
        Car {
            owner: PeerId::Netcode(number.into()),
            name: format!("Player {number}"),
            color: 0,
        }
    }

    fn rules(countdown: u64, finish_grace: u64) -> RaceRules {
        RaceRules {
            countdown: Duration::from_secs(countdown),
            finish_grace: Duration::from_secs(finish_grace),
        }
    }

    #[test]
    fn counts_down_then_races() {
        let mut director = RaceDirector::new(&rules(3, 10));
        assert_eq!(director.tick(Duration::ZERO), RaceStatus::Countdown(3));
        assert_eq!(
            director.tick(Duration::from_millis(500)),
            RaceStatus::Countdown(3)
        );
        assert_eq!(
            director.tick(Duration::from_secs(1)),
            RaceStatus::Countdown(2)
        );
        assert_eq!(director.tick(Duration::from_secs(2)), RaceStatus::Racing);
    }

    #[test]
    fn the_first_finish_starts_the_grace_period() {
        let mut director = RaceDirector::new(&rules(0, 10));
        assert_eq!(director.tick(Duration::ZERO), RaceStatus::Racing);
        assert_eq!(director.finish(&car(2)), 1);
        assert_eq!(director.finish(&car(1)), 2);
        assert_eq!(
            director.tick(Duration::from_secs(4)),
            RaceStatus::Finishing(6)
        );
        assert!(!director.grace_is_over());
        director.tick(Duration::from_secs(6));
        assert!(director.grace_is_over());
    }

    #[test]
    fn only_the_countdown_keeps_cars_still() {
        assert!(!RaceStatus::Countdown(1).cars_may_drive());
        assert!(RaceStatus::Racing.cars_may_drive());
        assert!(RaceStatus::Finishing(5).cars_may_drive());
    }

    #[test]
    fn placements_list_finishers_then_the_rest_by_progress() {
        let finishers = [car(2), car(4)];
        let behind = RaceProgress {
            next_checkpoint: 2,
            laps: 1,
            place: None,
        };
        let further = RaceProgress {
            next_checkpoint: 0,
            laps: 1,
            place: None,
        };
        let placements = race_placements(
            &finishers,
            vec![(car(1), behind, Vec2::ZERO), (car(3), further, Vec2::ZERO)],
        );
        let shown: Vec<String> = placements.iter().map(|p| p.describe()).collect();
        assert_eq!(
            shown,
            [
                "1st Player 2",
                "2nd Player 4",
                "3rd Player 3 (DNF)",
                "4th Player 1 (DNF)"
            ]
        );
        assert_eq!(placements[0].player, car(2).owner);
    }

    #[test]
    fn the_status_label_counts_down_then_says_go() {
        assert_eq!(status_label(RaceStatus::Countdown(3), false), "3");
        assert_eq!(status_label(RaceStatus::Countdown(0), false), "1");
        assert_eq!(status_label(RaceStatus::Racing, true), "GO!");
        assert_eq!(status_label(RaceStatus::Racing, false), "");
        assert_eq!(
            status_label(RaceStatus::Finishing(7), false),
            "Race ends in 7"
        );
    }
}
