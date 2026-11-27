use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use lightyear::prelude::input::native::ActionState;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PHASE;
use super::RacingSystems;
use super::config::{
    ACCELERATION, BRAKE_DECELERATION, CAR_MAX_X, CAR_MAX_Y, COAST_DECELERATION, EDGE_MAX_SPEED,
    FULL_STEER_SPEED, GRASS_DECELERATION, MAX_REVERSE_SPEED, MAX_SPEED, REVERSE_ACCELERATION,
    SLIDE_DECELERATION, TURN_RATE,
};
use super::progress::RaceProgress;
use super::race::picked_track;
use super::racing_input::RacingInput;
use super::track::{Surface, TrackId};
use crate::minigames::countdown::StartTick;
use crate::minigames::outcome::Participants;
use crate::minigames::settings::MinigameSettings;
use crate::network::is_host;
use crate::players::Player;
use crate::standings::Points;
use crate::standings::standings;

/// Who drives this car. Set once at spawn, copied from the driver's [`Player`] so the car keeps
/// them even if the player leaves mid-race.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Car {
    pub owner: PeerId,
    pub name: String,
    /// The driver's [`Player::color`], an index for [`player_color`](crate::players::player_color).
    pub color: u8,
}

/// Predicted by every driver, so they all see bumps (see [`bumps`](super::bumps)) the way the host
/// works them out, and interpolated for spectators. Everything [`drive`] and
/// [`bump`](super::bumps::bump) read or write must live here, or rollback would re-run them from
/// the wrong state.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CarState {
    /// Centre of the car, in world units.
    pub position: Vec2,
    /// Which way the car points, in radians anticlockwise from facing right (+x). Always in
    /// `-π..π`.
    pub heading: f32,
    /// Speed along [`heading`](Self::heading), in world units per second. Negative in reverse.
    pub speed: f32,
    /// Movement on top of [`speed`](Self::speed) from bumps, in world units per second: a shove
    /// sideways, or past the car's top speed. It wears off at [`SLIDE_DECELERATION`].
    pub slide: Vec2,
}

impl CarState {
    pub fn parked(position: Vec2, heading: f32) -> Self {
        Self {
            position,
            heading,
            speed: 0.0,
            slide: Vec2::ZERO,
        }
    }

    /// Where the car is going and how fast, in world units per second.
    pub fn velocity(self) -> Vec2 {
        Vec2::from_angle(self.heading) * self.speed + self.slide
    }
}

impl Ease for CarState {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| CarState {
            position: start.position.lerp(end.position, t),
            heading: lerp_angle(start.heading, end.heading, t),
            speed: start.speed.lerp(end.speed, t),
            slide: start.slide.lerp(end.slide, t),
        })
    }
}

/// Registers the replicated car components, spawns the cars, and drives them.
pub struct CarPlugin;

impl Plugin for CarPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Car>().replicate();

        app.component::<CarState>()
            .replicate()
            .predict()
            .add_linear_interpolation();

        app.add_systems(OnEnter(PHASE), spawn_cars.run_if(is_host));
        app.add_systems(
            FixedUpdate,
            drive_cars
                .in_set(RacingSystems::Drive)
                .run_if(in_state(PHASE)),
        );
    }
}

/// One car per [`Participants`] player still here, on the grid in reverse standings order: the
/// player with the fewest points starts in front. Players on the same points keep joining order.
/// Every driver predicts every car, with the other drivers' inputs rebroadcast by the host (see
/// [`RacingInputPlugin`](super::racing_input::RacingInputPlugin)): cars that can bump each other
/// must be simulated on the same timeline. Spectators interpolate them.
fn spawn_cars(
    mut commands: Commands,
    participants: Single<&Participants>,
    players: Query<(&Player, &Points, &ControlledBy)>,
    settings: Res<MinigameSettings>,
) {
    let track = picked_track(&settings).track();
    let active_participants: Vec<(&Player, Points, &ControlledBy)> = participants
        .0
        .iter()
        .filter_map(|id| {
            players
                .iter()
                .find(|(player, _, _)| player.id == *id)
                .map(|(player, points, link)| (player, *points, link))
        })
        .collect();

    let drivers: Vec<PeerId> = active_participants
        .iter()
        .map(|(player, _, _)| player.id)
        .collect();

    let mut inverted_grid = standings(
        active_participants
            .iter()
            .map(|(player, points, _)| ((*player).clone(), *points)),
    );
    // Last place first, so the fewest points start on pole (slot 0). The sort is stable, so
    // players who share a place stay in joining order, as `standings` lists them.
    inverted_grid.sort_by_key(|standing| std::cmp::Reverse(standing.place));

    debug!(
        "Spawning {} cars in inverted standing order: {:?}",
        inverted_grid.len(),
        inverted_grid
            .iter()
            .map(|s| format!("{} (#{})", s.player.name(), s.place))
            .collect::<Vec<_>>()
    );

    for (slot, standing) in inverted_grid.into_iter().enumerate() {
        let link = active_participants
            .iter()
            .find(|(player, _, _)| player.id == standing.player.id)
            .map(|(_, _, link)| *link)
            .expect("participant exists");

        commands.spawn((
            Car {
                owner: standing.player.id,
                name: standing.player.name,
                color: standing.player.color,
            },
            CarState::parked(track.grid_position(slot), track.layout().start_heading),
            RaceProgress::on_grid(),
            DespawnOnExit(PHASE),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::from(drivers.clone())),
            InterpolationTarget::to_clients(NetworkTarget::from_exclude(drivers.iter().copied())),
            ControlledBy {
                owner: link.owner,
                lifetime: Lifetime::default(),
            },
        ));
    }
}

/// Runs on the host for every car and on each driver's machine for the cars it predicts (all of
/// them). Spectators' cars are interpolated, so it leaves them alone, even though the host's
/// rebroadcast gives them inputs too. A car with no input yet coasts. Before the race's
/// race's [`StartTick`], input is ignored: judged by the tick being simulated, so a rollback
/// replaying ticks from before the start keeps the cars still. Until the race's entity (with its
/// start and its [`TrackId`]) has arrived, cars stay parked on the grid where they were spawned.
fn drive_cars(
    time: Res<Time>,
    timeline: Res<LocalTimeline>,
    race: Option<Single<(&StartTick, &TrackId)>>,
    mut cars: Query<(&mut CarState, Option<&ActionState<RacingInput>>), Without<Interpolated>>,
) {
    let Some((start, track)) = race.map(|race| (*race.0, race.1.track())) else {
        return;
    };
    let may_drive = start.has_started(timeline.tick());
    for (mut state, input) in &mut cars {
        let input = match input {
            Some(input) if may_drive => input.0,
            _ => RacingInput::default(),
        };
        let next = drive(
            *state,
            input,
            track.surface_at(state.position),
            time.delta_secs(),
        );
        state.set_if_neq(next);
    }
}

/// The car's state after driving for `dt` seconds with `input` on `surface`. A pure function, so
/// Lightyear can re-run it during rollback.
pub fn drive(state: CarState, input: RacingInput, surface: Surface, dt: f32) -> CarState {
    let pedal: f32 = match (input.throttle, input.brake) {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => 0.0,
    };
    let speed = state.speed;
    let mut speed = if pedal == 0.0 {
        approach_zero(speed, COAST_DECELERATION * dt)
    } else if speed * pedal < 0.0 {
        approach_zero(speed, BRAKE_DECELERATION * dt)
    } else if pedal > 0.0 {
        (speed + ACCELERATION * dt).min(MAX_SPEED)
    } else {
        (speed - REVERSE_ACCELERATION * dt).max(-MAX_REVERSE_SPEED)
    };
    // Grass slows a fast car down over a moment instead of all at once.
    let surface_max = surface.max_speed();
    if speed.abs() > surface_max {
        speed = (speed.abs() - GRASS_DECELERATION * dt).max(surface_max) * speed.signum();
    }

    let steer = f32::from(input.steer.clamp(-1, 1));
    let grip = (speed.abs() / FULL_STEER_SPEED).min(1.0);
    let turn = steer * TURN_RATE * grip * dt;
    let heading = wrap_angle(if speed < 0.0 {
        state.heading - turn
    } else {
        state.heading + turn
    });

    let mut slide = approach_zero_vec(state.slide, SLIDE_DECELERATION * dt);
    let moved = state.position + (Vec2::from_angle(heading) * speed + slide) * dt;
    let position = on_screen(moved);
    if position != moved {
        speed = speed.clamp(-EDGE_MAX_SPEED, EDGE_MAX_SPEED);
        // The edge also stops a slide across it.
        if moved.x.abs() > CAR_MAX_X {
            slide.x = 0.0;
        }
        if moved.y.abs() > CAR_MAX_Y {
            slide.y = 0.0;
        }
    }

    CarState {
        position,
        heading,
        speed,
        slide,
    }
}

/// `position`, moved back on screen if a car centred there would be partly off it.
pub fn on_screen(position: Vec2) -> Vec2 {
    let limit = Vec2::new(CAR_MAX_X, CAR_MAX_Y);
    position.clamp(-limit, limit)
}

fn approach_zero(speed: f32, amount: f32) -> f32 {
    if speed > 0.0 {
        (speed - amount).max(0.0)
    } else {
        (speed + amount).min(0.0)
    }
}

/// `velocity` slowed down by `amount`, without turning round.
fn approach_zero_vec(velocity: Vec2, amount: f32) -> Vec2 {
    velocity.clamp_length_max((velocity.length() - amount).max(0.0))
}

/// `angle` wrapped into `-π..π`.
fn wrap_angle(angle: f32) -> f32 {
    (angle + PI).rem_euclid(TAU) - PI
}

/// Turns the short way round: a plain lerp across `±π` would spin almost a full turn.
fn lerp_angle(start: f32, end: f32, t: f32) -> f32 {
    wrap_angle(start + wrap_angle(end - start) * t)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, PI};
    use std::time::Duration;

    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use lightyear::prelude::Interpolated;
    use lightyear::prelude::input::native::ActionState;

    use lightyear::prelude::{LocalTimeline, Tick};

    use super::{CarState, RacingInput, StartTick, drive, drive_cars, lerp_angle, wrap_angle};
    use crate::minigames::racing::config::{
        CAR_MAX_X, EDGE_MAX_SPEED, GRASS_MAX_SPEED, MAX_REVERSE_SPEED, MAX_SPEED,
        SLIDE_DECELERATION,
    };
    use crate::minigames::racing::race::RaceStatus;
    use crate::minigames::racing::track::{Surface, TrackId};

    const DT: f32 = 1.0 / 128.0;

    const THROTTLE: RacingInput = RacingInput {
        throttle: true,
        brake: false,
        steer: 0,
    };
    const BRAKE: RacingInput = RacingInput {
        throttle: false,
        brake: true,
        steer: 0,
    };
    const COAST: RacingInput = RacingInput {
        throttle: false,
        brake: false,
        steer: 0,
    };

    fn moving(speed: f32) -> CarState {
        CarState {
            speed,
            ..CarState::parked(Vec2::ZERO, 0.0)
        }
    }

    /// `state` after driving with `input` for `seconds`, one tick at a time.
    fn drive_for(
        mut state: CarState,
        input: RacingInput,
        surface: Surface,
        seconds: f32,
    ) -> CarState {
        for _ in 0..(seconds / DT) as usize {
            state = drive(state, input, surface, DT);
        }
        state
    }

    // Keep these drives short (about a second), or the car reaches the edge of the screen and is
    // slowed down by it.

    #[test]
    fn throttle_drives_forwards_up_to_top_speed() {
        let state = drive_for(moving(0.0), THROTTLE, Surface::Road, 1.0);
        assert_eq!(state.speed, MAX_SPEED);
        assert!(state.position.x > 0.0);
        assert_eq!(state.heading, 0.0);
    }

    #[test]
    fn brake_stops_then_reverses_up_to_reverse_speed() {
        let state = drive(moving(100.0), BRAKE, Surface::Road, DT);
        assert!(state.speed > 0.0 && state.speed < 100.0);
        let state = drive_for(moving(100.0), BRAKE, Surface::Road, 1.0);
        assert_eq!(state.speed, -MAX_REVERSE_SPEED);
    }

    #[test]
    fn coasting_rolls_to_a_stop_without_reversing() {
        assert_eq!(
            drive_for(moving(100.0), COAST, Surface::Road, 1.0).speed,
            0.0
        );
        assert_eq!(
            drive_for(moving(-50.0), COAST, Surface::Road, 1.0).speed,
            0.0
        );
    }

    #[test]
    fn a_car_standing_still_cannot_turn() {
        let input = RacingInput { steer: 1, ..COAST };
        assert_eq!(drive(moving(0.0), input, Surface::Road, DT).heading, 0.0);
    }

    #[test]
    fn steering_left_turns_anticlockwise_forwards_and_clockwise_in_reverse() {
        let input = RacingInput { steer: 1, ..COAST };
        assert!(drive(moving(100.0), input, Surface::Road, DT).heading > 0.0);
        assert!(drive(moving(-50.0), input, Surface::Road, DT).heading < 0.0);
    }

    #[test]
    fn the_car_stays_on_screen_and_slows_at_the_edge() {
        let state = CarState {
            speed: MAX_SPEED,
            ..CarState::parked(Vec2::new(CAR_MAX_X, 0.0), 0.0)
        };
        let state = drive(state, THROTTLE, Surface::Road, DT);
        assert_eq!(state.position.x, CAR_MAX_X);
        assert_eq!(state.speed, EDGE_MAX_SPEED);
    }

    #[test]
    fn headings_stay_between_minus_pi_and_pi() {
        assert!((wrap_angle(3.0 * FRAC_PI_2) + FRAC_PI_2).abs() < 1e-5);
        assert!((wrap_angle(-3.0 * FRAC_PI_2) - FRAC_PI_2).abs() < 1e-5);
        assert!((wrap_angle(1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn interpolation_turns_the_short_way_round() {
        let start = PI - 0.1;
        let end = -PI + 0.1;
        let halfway = lerp_angle(start, end, 0.5);
        assert!(halfway.abs() > PI - 1e-3, "went the long way: {halfway}");
    }

    #[test]
    fn a_slide_moves_the_car_and_wears_off() {
        let sliding = CarState {
            slide: Vec2::new(0.0, 120.0),
            ..CarState::parked(Vec2::ZERO, 0.0)
        };
        let state = drive(sliding, COAST, Surface::Road, DT);
        assert!(state.position.y > 0.0 && state.position.x.abs() < 1e-6);
        assert!(state.slide.y < 120.0 && state.slide.y > 0.0);
        assert_eq!(state.slide.x, 0.0);
        let state = drive_for(
            sliding,
            COAST,
            Surface::Road,
            120.0 / SLIDE_DECELERATION + 0.1,
        );
        assert_eq!(state.slide, Vec2::ZERO);
        // It went about as far as a steady slow-down from 120 to 0 goes.
        let expected = 120.0 * 120.0 / (2.0 * SLIDE_DECELERATION);
        assert!((state.position.y - expected).abs() < 1.0, "{state:?}");
    }

    #[test]
    fn the_edge_stops_a_slide_across_it() {
        let state = CarState {
            slide: Vec2::new(100.0, 50.0),
            ..CarState::parked(Vec2::new(CAR_MAX_X, 0.0), FRAC_PI_2)
        };
        let state = drive(state, COAST, Surface::Road, DT);
        assert_eq!(state.position.x, CAR_MAX_X);
        assert_eq!(state.slide.x, 0.0);
        assert!(
            state.slide.y > 0.0,
            "the slide along the edge goes on: {state:?}"
        );
    }

    #[test]
    fn velocity_adds_the_slide_to_the_speed_along_the_heading() {
        let state = CarState {
            speed: 100.0,
            slide: Vec2::new(0.0, -30.0),
            ..CarState::parked(Vec2::ZERO, FRAC_PI_2)
        };
        assert!(state.velocity().abs_diff_eq(Vec2::new(0.0, 70.0), 1e-4));
    }

    #[test]
    fn grass_limits_the_top_speed() {
        let state = drive_for(moving(0.0), THROTTLE, Surface::Grass, 1.0);
        assert_eq!(state.speed, GRASS_MAX_SPEED);
    }

    #[test]
    fn grass_slows_a_fast_car_gradually() {
        let state = drive(moving(MAX_SPEED), THROTTLE, Surface::Grass, DT);
        assert!(state.speed < MAX_SPEED && state.speed > GRASS_MAX_SPEED);
        let state = drive_for(moving(MAX_SPEED), THROTTLE, Surface::Grass, 1.0);
        assert_eq!(state.speed, GRASS_MAX_SPEED);
    }

    #[test]
    fn grass_doesnt_limit_reversing_below_its_top_speed() {
        let state = drive_for(moving(0.0), BRAKE, Surface::Grass, 1.0);
        assert_eq!(state.speed, -MAX_REVERSE_SPEED.min(GRASS_MAX_SPEED));
    }

    /// The tick races start on in these tests.
    const START: u32 = 100;

    /// A world simulating tick `tick` of a race that starts on [`START`], for running
    /// `drive_cars`.
    fn race_world(tick: u32) -> World {
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_secs_f32(DT));
        world.insert_resource(time);
        let mut timeline = LocalTimeline::default();
        timeline.apply_delta(tick as i32);
        world.insert_resource(timeline);
        world.spawn((StartTick(Tick(START)), TrackId::default()));
        world
    }

    /// Whether one run of `drive_cars` on tick `tick` marks a car in `state`, driven with
    /// `input`, as changed.
    fn drive_cars_marks_changed(tick: u32, state: CarState, input: RacingInput) -> bool {
        let mut world = race_world(tick);
        let car = world.spawn((state, ActionState(input))).id();
        world.clear_trackers();
        world.run_system_once(drive_cars).unwrap();
        world
            .entity(car)
            .get_ref::<CarState>()
            .unwrap()
            .is_changed()
    }

    #[test]
    fn a_car_with_no_input_yet_coasts() {
        let mut world = race_world(START);
        let car = world.spawn(moving(100.0)).id();
        world.run_system_once(drive_cars).unwrap();
        let speed = world.get::<CarState>(car).unwrap().speed;
        assert!(speed > 0.0 && speed < 100.0, "{speed}");
    }

    #[test]
    fn spectators_cars_are_left_to_interpolation() {
        let mut world = race_world(START);
        let car = world
            .spawn((moving(100.0), ActionState(THROTTLE), Interpolated))
            .id();
        world.run_system_once(drive_cars).unwrap();
        assert_eq!(*world.get::<CarState>(car).unwrap(), moving(100.0));
    }

    #[test]
    fn a_parked_car_is_not_marked_as_changed() {
        assert!(!drive_cars_marks_changed(START, moving(0.0), COAST));
    }

    #[test]
    fn a_car_that_moves_is_marked_as_changed() {
        assert!(drive_cars_marks_changed(START, moving(0.0), THROTTLE));
    }

    #[test]
    fn cars_cant_move_before_the_start_tick() {
        assert!(!drive_cars_marks_changed(START - 1, moving(0.0), THROTTLE));
    }

    #[test]
    fn a_rollback_replaying_a_tick_before_the_start_keeps_the_car_still() {
        // The host's status already says the race is on, but this tick is from before the start.
        let mut world = race_world(START - 5);
        world.spawn(RaceStatus::Racing);
        let car = world.spawn((moving(0.0), ActionState(THROTTLE))).id();
        world.run_system_once(drive_cars).unwrap();
        assert_eq!(*world.get::<CarState>(car).unwrap(), moving(0.0));
    }

    #[test]
    fn cars_cant_move_before_the_start_has_arrived() {
        let mut world = race_world(START + 10);
        let start = world
            .query_filtered::<Entity, With<StartTick>>()
            .single(&world)
            .unwrap();
        world.despawn(start);
        let car = world.spawn((moving(0.0), ActionState(THROTTLE))).id();
        world.run_system_once(drive_cars).unwrap();
        assert_eq!(*world.get::<CarState>(car).unwrap(), moving(0.0));
    }

    #[test]
    fn cars_cant_move_before_the_track_is_known() {
        let mut world = race_world(START + 10);
        let race = world
            .query_filtered::<Entity, With<StartTick>>()
            .single(&world)
            .unwrap();
        world.entity_mut(race).remove::<TrackId>();
        let car = world.spawn((moving(0.0), ActionState(THROTTLE))).id();
        world.run_system_once(drive_cars).unwrap();
        assert_eq!(*world.get::<CarState>(car).unwrap(), moving(0.0));
    }

    #[test]
    fn cars_drive_on_the_races_own_track() {
        // On the oval's left bend, which is grass on the figure of eight.
        let spot = CarState {
            position: Vec2::new(-270.0, 0.0),
            ..moving(150.0)
        };
        let speed_on = |track: TrackId| {
            let mut world = race_world(START);
            let race = world
                .query_filtered::<Entity, With<StartTick>>()
                .single(&world)
                .unwrap();
            world.entity_mut(race).insert(track);
            let car = world.spawn((spot, ActionState(THROTTLE))).id();
            world.run_system_once(drive_cars).unwrap();
            world.get::<CarState>(car).unwrap().speed
        };
        assert!(speed_on(TrackId::Oval) > 150.0, "speeds up on the road");
        assert!(
            speed_on(TrackId::FigureEight) < 150.0,
            "slows down on grass"
        );
    }
}
