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
use crate::session::{MinigameEnded, MinigameResult, SessionPhase};

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

/// A car that crossed the line on its last lap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finisher {
    pub owner: PeerId,
    pub number: u32,
}

/// Host only: the race's clocks and finishing order. Exists while racing.
#[derive(Resource, Debug)]
pub struct RaceDirector {
    countdown: Timer,
    finish_grace: Duration,
    /// Started by the first finish.
    grace: Option<Timer>,
    /// In finishing order. Players who leave after finishing stay in it.
    finishers: Vec<Finisher>,
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
        self.finishers.push(Finisher {
            owner: car.owner,
            number: car.number,
        });
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
    let Some(winner) = director.finishers.first().copied() else {
        return;
    };
    let unfinished = cars
        .iter()
        .filter(|(_, progress, _)| progress.place.is_none())
        .map(|(car, progress, state)| (*car, *progress, state.position))
        .collect();
    let summary = race_summary(&director.finishers, unfinished);
    info!(
        "Race over ({}): {summary}",
        if everyone_finished {
            "every car finished"
        } else {
            "time ran out after the first finish"
        }
    );
    director.over = true;
    commands.trigger(MinigameEnded::Won(MinigameResult {
        winner: winner.owner,
        summary,
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

/// The final order, e.g. "1st Player 2 · 2nd Player 1 · 3rd Player 3 (DNF)": the finishers in the
/// order they finished, then the cars that didn't finish, in [`running_order`].
pub fn race_summary(
    finishers: &[Finisher],
    mut unfinished: Vec<(Car, RaceProgress, Vec2)>,
) -> String {
    unfinished.sort_by_key(|(_, progress, position)| running_order(*progress, *position));
    finishers
        .iter()
        .map(|finisher| (finisher.number, true))
        .chain(unfinished.iter().map(|(car, _, _)| (car.number, false)))
        .enumerate()
        .map(|(i, (number, finished))| {
            let place = ordinal(i as u32 + 1);
            let dnf = if finished { "" } else { " (DNF)" };
            format!("{place} Player {number}{dnf}")
        })
        .collect::<Vec<_>>()
        .join(" · ")
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::math::Vec2;
    use lightyear::prelude::PeerId;

    use super::{
        Car, Finisher, RaceDirector, RaceRules, RaceStatus, ordinal, race_summary, status_label,
    };
    use crate::minigames::racing::progress::RaceProgress;

    fn car(number: u32) -> Car {
        Car {
            owner: PeerId::Netcode(number.into()),
            number,
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
    fn the_summary_lists_finishers_then_the_rest_by_progress() {
        let finishers = [car(2), car(4)].map(|car| Finisher {
            owner: car.owner,
            number: car.number,
        });
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
        let summary = race_summary(
            &finishers,
            vec![(car(1), behind, Vec2::ZERO), (car(3), further, Vec2::ZERO)],
        );
        assert_eq!(
            summary,
            "1st Player 2 · 2nd Player 4 · 3rd Player 3 (DNF) · 4th Player 1 (DNF)"
        );
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
