//! The race as a whole: the countdown, the finishing order, and when the race ends. The host
//! decides all of it; clients see the replicated [`RaceStatus`] and each car's [`RaceProgress`].

use std::collections::HashMap;
use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::car::{Car, CarState};
use super::config::{COUNTDOWN, FINISH_GRACE};
use super::progress::{RaceProgress, running_order};
use super::track::{Track, TrackId};
use super::{LAPS_SETTING, PHASE, RacingSystems, TRACK_SETTING};
use crate::minigames::outcome::{MinigameEnded, MinigameResult, Placement};
use crate::minigames::settings::MinigameSettings;
use crate::network::is_host;

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

/// The tick the race starts on: cars drive from it on. The host picks it as the race is set up, a
/// whole countdown ahead, and it's replicated with the [`RaceStatus`], so every peer starts the
/// cars it simulates on the same tick. A joiner predicts ahead of the host, so it would otherwise
/// start them only once the host's [`RaceStatus::Racing`] arrived, too late, and its cars would
/// jump when corrected. Rollbacks that replay ticks from before the start keep the cars still,
/// too, which the current [`RaceStatus`] can't tell them.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaceStart(pub Tick);

impl RaceStart {
    /// Whether cars may drive on tick `tick`.
    pub fn cars_may_drive(self, tick: Tick) -> bool {
        tick >= self.0
    }
}

/// How many fixed ticks of `timestep` a countdown of `countdown` lasts: the tick its timer
/// finishes on, counting the first tick after it starts as 1. (A countdown of 0 is over at once.)
pub fn countdown_ticks(countdown: Duration, timestep: Duration) -> u32 {
    countdown.as_nanos().div_ceil(timestep.as_nanos()) as u32
}

/// Host only: how the next race is timed. How many laps it has is a setting the host picks in
/// the lobby: [`LAPS_SETTING`].
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

/// The quickest lap anyone drove in a race.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastestLap {
    pub name: String,
    pub time: Duration,
}

/// Host only: the race's clocks, lap times and finishing order. Exists while racing.
#[derive(Resource, Debug)]
pub struct RaceDirector {
    laps: u32,
    track: TrackId,
    countdown: Timer,
    /// Time since the race was set up, countdown included. Lap times are measured on it.
    clock: Duration,
    /// When each car last crossed the line. A car not in it is on its first lap, which started
    /// when the countdown ended.
    lap_started: HashMap<PeerId, Duration>,
    fastest_lap: Option<FastestLap>,
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
    fn new(rules: &RaceRules, laps: u32, track: TrackId) -> Self {
        Self {
            laps,
            track,
            countdown: Timer::new(rules.countdown, TimerMode::Once),
            clock: Duration::ZERO,
            lap_started: HashMap::new(),
            fastest_lap: None,
            finish_grace: rules.finish_grace,
            grace: None,
            finishers: Vec::new(),
            over: false,
        }
    }

    /// How many laps a car must complete to finish.
    pub fn laps(&self) -> u32 {
        self.laps
    }

    /// The track the race is on.
    pub fn track(&self) -> &'static Track {
        self.track.track()
    }

    /// Record that `car` just crossed the line to complete a lap, and return how long the lap
    /// took. The first lap is timed from the end of the countdown, so it includes the run-up from
    /// the grid.
    pub fn complete_lap(&mut self, car: &Car) -> Duration {
        let started = self
            .lap_started
            .insert(car.owner, self.clock)
            .unwrap_or(self.countdown.duration());
        let time = self.clock.saturating_sub(started);
        if self
            .fastest_lap
            .as_ref()
            .is_none_or(|fastest| time < fastest.time)
        {
            self.fastest_lap = Some(FastestLap {
                name: car.name.clone(),
                time,
            });
        }
        time
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
        self.clock += delta;
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
        app.component::<RaceStart>().replicate();
        app.component::<TrackId>().replicate();
        app.init_resource::<RaceRules>();

        app.add_systems(OnEnter(PHASE), start_race.run_if(is_host));
        app.add_systems(OnExit(PHASE), |mut commands: Commands| {
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
                .run_if(in_state(PHASE)),
        );
    }
}

/// Host only: the track the host picked for the next race.
pub fn picked_track(settings: &MinigameSettings) -> TrackId {
    TrackId::from_setting(settings.get(TRACK_SETTING))
}

/// Host only: when the race starts, spawn its status and start the countdown.
///
/// The countdown's timer (in [`RaceDirector`]) finishes on the same tick as [`RaceStart`], so the
/// status says "GO!" on the tick the cars may first drive. The race's [`TrackId`] goes on the same
/// entity as [`RaceStart`], so they reach every peer together.
fn start_race(
    rules: Res<RaceRules>,
    settings: Res<MinigameSettings>,
    timeline: Res<LocalTimeline>,
    fixed: Res<Time<Fixed>>,
    mut commands: Commands,
) {
    let laps = settings.get(LAPS_SETTING);
    let track = picked_track(&settings);
    let start = timeline.tick() + countdown_ticks(rules.countdown, fixed.timestep()) as i32;
    info!(
        "Race of {laps} laps on the {} track starts in {} s, on tick {}",
        track.name(),
        rules.countdown.as_secs_f32().ceil() as u32,
        start.0
    );
    commands.spawn((
        RaceStatus::Countdown(whole_seconds(rules.countdown)),
        RaceStart(start),
        track,
        DespawnOnExit(PHASE),
        Replicate::to_clients(NetworkTarget::All),
    ));
    commands.insert_resource(RaceDirector::new(&rules, laps, track));
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
/// first finish runs out. If fewer than two cars are left before anyone has finished, abandon it:
/// there's nobody left to race against.
fn end_race(
    mut director: ResMut<RaceDirector>,
    cars: Query<(&Car, &RaceProgress, &CarState)>,
    mut commands: Commands,
) {
    if director.over {
        return;
    }
    if cars.iter().len() < 2 && director.finishers.is_empty() {
        match cars.iter().next() {
            Some((car, ..)) => info!(
                "Abandoning the race: every other player left, leaving {} ({:?}) alone before \
                 anyone finished",
                car.name, car.owner
            ),
            None => info!("Abandoning the race: every car's player has left"),
        }
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
    let placements = race_placements(director.track(), &director.finishers, unfinished);
    let summary = race_summary(director.fastest_lap.as_ref());
    info!(
        "Race over ({}). {summary}",
        if everyone_finished {
            "every car finished"
        } else {
            "time ran out after the first finish"
        }
    );
    director.over = true;
    commands.trigger(MinigameEnded::Won(MinigameResult {
        placements,
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

/// The results screen's line under the headline, e.g. "Fastest lap: Ada, 7.42 s".
pub fn race_summary(fastest_lap: Option<&FastestLap>) -> String {
    match fastest_lap {
        Some(lap) => format!("Fastest lap: {}, {}", lap.name, lap_time_label(lap.time)),
        None => String::new(),
    }
}

/// A lap time to the hundredth of a second: "7.42 s", or "1:02.35" from a minute up.
pub fn lap_time_label(time: Duration) -> String {
    let hundredths = (time.as_millis() + 5) / 10;
    let (minutes, seconds, hundredths) =
        (hundredths / 6000, hundredths / 100 % 60, hundredths % 100);
    if minutes == 0 {
        format!("{seconds}.{hundredths:02} s")
    } else {
        format!("{minutes}:{seconds:02}.{hundredths:02}")
    }
}

/// The final order: the finishers in the order they finished, then the cars that didn't finish
/// (marked "DNF"), in [`running_order`].
pub fn race_placements(
    track: &Track,
    finishers: &[Car],
    mut unfinished: Vec<(Car, RaceProgress, Vec2)>,
) -> Vec<Placement> {
    unfinished.sort_by_key(|(_, progress, position)| running_order(track, *progress, *position));
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

    use lightyear::prelude::Tick;

    use super::{
        Car, FastestLap, RaceDirector, RaceRules, RaceStart, RaceStatus, countdown_ticks,
        lap_time_label, race_placements, race_summary, status_label,
    };
    use crate::minigames::racing::progress::RaceProgress;
    use crate::minigames::racing::track::TrackId;

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
        let mut director = RaceDirector::new(&rules(3, 10), 3, TrackId::default());
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
        let mut director = RaceDirector::new(&rules(0, 10), 3, TrackId::default());
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
    fn times_laps_and_keeps_the_fastest() {
        let mut director = RaceDirector::new(&rules(3, 10), 3, TrackId::default());
        director.tick(Duration::from_secs(3));
        // First laps are timed from the end of the countdown.
        director.tick(Duration::from_secs(9));
        assert_eq!(director.complete_lap(&car(1)), Duration::from_secs(9));
        director.tick(Duration::from_secs(1));
        assert_eq!(director.complete_lap(&car(2)), Duration::from_secs(10));
        // Later laps from the car's last crossing.
        director.tick(Duration::from_secs(7));
        assert_eq!(director.complete_lap(&car(1)), Duration::from_secs(8));
        director.tick(Duration::from_secs(1));
        assert_eq!(director.complete_lap(&car(2)), Duration::from_secs(8));
        // A tie keeps the first to set it.
        assert_eq!(
            director.fastest_lap,
            Some(FastestLap {
                name: "Player 1".into(),
                time: Duration::from_secs(8),
            })
        );
    }

    #[test]
    fn the_summary_names_the_fastest_lap() {
        let lap = FastestLap {
            name: "Ada".into(),
            time: Duration::from_millis(7_423),
        };
        assert_eq!(race_summary(Some(&lap)), "Fastest lap: Ada, 7.42 s");
        assert_eq!(race_summary(None), "");
    }

    #[test]
    fn lap_times() {
        let shown: Vec<String> = [0, 7_423, 7_425, 59_994, 59_996, 62_350]
            .into_iter()
            .map(|ms| lap_time_label(Duration::from_millis(ms)))
            .collect();
        assert_eq!(
            shown,
            [
                "0.00 s", "7.42 s", "7.43 s", "59.99 s", "1:00.00", "1:02.35"
            ]
        );
    }

    #[test]
    fn cars_drive_from_the_start_tick_on() {
        let start = RaceStart(Tick(100));
        assert!(!start.cars_may_drive(Tick(99)));
        assert!(start.cars_may_drive(Tick(100)));
        assert!(start.cars_may_drive(Tick(5000)));
    }

    #[test]
    fn the_countdown_timer_finishes_on_the_start_tick() {
        // At 128 Hz and at 60 Hz, where a tick isn't a whole number of nanoseconds.
        for rate in [128.0, 60.0] {
            let timestep = Duration::from_secs_f64(1.0 / rate);
            for countdown in [0, 1, 3, 10].map(Duration::from_secs) {
                let ticks = countdown_ticks(countdown, timestep);
                let mut director =
                    RaceDirector::new(&rules_with_countdown(countdown), 3, TrackId::default());
                // The timer is ticked once per fixed tick, from the first one after it starts.
                let mut finished_on = None;
                for tick in 1..=ticks + 1 {
                    if director.tick(timestep) == RaceStatus::Racing {
                        finished_on = Some(tick);
                        break;
                    }
                }
                assert_eq!(
                    finished_on,
                    Some(ticks.max(1)),
                    "{countdown:?} at {rate} Hz"
                );
            }
        }
    }

    fn rules_with_countdown(countdown: Duration) -> RaceRules {
        RaceRules {
            countdown,
            ..Default::default()
        }
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
            TrackId::default().track(),
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
