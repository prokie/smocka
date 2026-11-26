use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use lightyear::prelude::input::native::ActionState;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::RacingSystems;
use super::progress::RaceProgress;
use super::race::RaceStatus;
use super::racing_input::RacingInput;
use super::track::{START_HEADING, Surface, grid_position, surface_at};
use crate::config::minigames::racing::{
    ACCELERATION, BRAKE_DECELERATION, CAR_MAX_X, CAR_MAX_Y, COAST_DECELERATION, EDGE_MAX_SPEED,
    FULL_STEER_SPEED, GRASS_DECELERATION, MAX_REVERSE_SPEED, MAX_SPEED, REVERSE_ACCELERATION,
    TURN_RATE,
};
use crate::network::is_host;
use crate::players::Player;
use crate::session::{Participants, SessionPhase};

/// Who drives this car. Set once at spawn, copied from the driver's [`Player`] so the car keeps
/// them even if the player leaves mid-race.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Car {
    pub owner: PeerId,
    pub name: String,
    /// The driver's [`Player::color`], an index for [`player_color`](crate::players::player_color).
    pub color: u8,
}

/// Predicted for the driver and interpolated for everyone else. Everything [`drive`] reads or
/// writes must live here, or rollback would re-run it from the wrong state.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CarState {
    /// Centre of the car, in world units.
    pub position: Vec2,
    /// Which way the car points, in radians anticlockwise from facing right (+x). Always in
    /// `-π..π`.
    pub heading: f32,
    /// Speed along [`heading`](Self::heading), in world units per second. Negative in reverse.
    pub speed: f32,
}

impl CarState {
    pub fn parked(position: Vec2, heading: f32) -> Self {
        Self {
            position,
            heading,
            speed: 0.0,
        }
    }
}

impl Ease for CarState {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| CarState {
            position: start.position.lerp(end.position, t),
            heading: lerp_angle(start.heading, end.heading, t),
            speed: start.speed.lerp(end.speed, t),
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

        app.add_systems(OnEnter(SessionPhase::Racing), spawn_cars.run_if(is_host));
        app.add_systems(
            FixedUpdate,
            drive_cars
                .in_set(RacingSystems::Drive)
                .run_if(in_state(SessionPhase::Racing)),
        );
    }
}

/// One car per [`Participants`] player still here, on the grid in their order.
fn spawn_cars(
    mut commands: Commands,
    participants: Single<&Participants>,
    players: Query<(&Player, &ControlledBy)>,
) {
    let players: Vec<_> = participants
        .0
        .iter()
        .filter_map(|id| players.iter().find(|(player, _)| player.id == *id))
        .collect();
    let count = players.len();
    debug!(
        "Spawning {count} cars: {:?}",
        players
            .iter()
            .map(|(player, _)| player.name())
            .collect::<Vec<_>>()
    );
    for (slot, (player, link)) in players.into_iter().enumerate() {
        commands.spawn((
            Car {
                owner: player.id,
                name: player.name.clone(),
                color: player.color,
            },
            CarState::parked(grid_position(slot), START_HEADING),
            RaceProgress::on_grid(),
            DespawnOnExit(SessionPhase::Racing),
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::Single(player.id)),
            InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(player.id)),
            ControlledBy {
                owner: link.owner,
                lifetime: Lifetime::default(),
            },
        ));
    }
}

/// Runs on the host for every car and on each client for its own predicted one. Until the
/// countdown is over (or the race's status hasn't arrived yet), input is ignored.
fn drive_cars(
    time: Res<Time>,
    status: Option<Single<&RaceStatus>>,
    mut cars: Query<(&mut CarState, &ActionState<RacingInput>)>,
) {
    let may_drive = status.is_some_and(|status| status.cars_may_drive());
    for (mut state, input) in &mut cars {
        let input = if may_drive {
            input.0
        } else {
            RacingInput::default()
        };
        let next = drive(*state, input, surface_at(state.position), time.delta_secs());
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

    let moved = state.position + Vec2::from_angle(heading) * speed * dt;
    let limit = Vec2::new(CAR_MAX_X, CAR_MAX_Y);
    let position = moved.clamp(-limit, limit);
    if position != moved {
        speed = speed.clamp(-EDGE_MAX_SPEED, EDGE_MAX_SPEED);
    }

    CarState {
        position,
        heading,
        speed,
    }
}

fn approach_zero(speed: f32, amount: f32) -> f32 {
    if speed > 0.0 {
        (speed - amount).max(0.0)
    } else {
        (speed + amount).min(0.0)
    }
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
    use lightyear::prelude::input::native::ActionState;

    use super::{CarState, RaceStatus, RacingInput, drive, drive_cars, lerp_angle, wrap_angle};
    use crate::config::minigames::racing::{
        CAR_MAX_X, EDGE_MAX_SPEED, GRASS_MAX_SPEED, MAX_REVERSE_SPEED, MAX_SPEED,
    };
    use crate::minigames::racing::track::Surface;

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

    /// Whether one run of `drive_cars` during `status` marks a car in `state`, driven with
    /// `input`, as changed.
    fn drive_cars_marks_changed(status: RaceStatus, state: CarState, input: RacingInput) -> bool {
        let mut world = World::new();
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_secs_f32(DT));
        world.insert_resource(time);
        world.spawn(status);
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
    fn a_parked_car_is_not_marked_as_changed() {
        assert!(!drive_cars_marks_changed(
            RaceStatus::Racing,
            moving(0.0),
            COAST
        ));
    }

    #[test]
    fn a_car_that_moves_is_marked_as_changed() {
        assert!(drive_cars_marks_changed(
            RaceStatus::Racing,
            moving(0.0),
            THROTTLE
        ));
    }

    #[test]
    fn cars_cant_move_during_the_countdown() {
        assert!(!drive_cars_marks_changed(
            RaceStatus::Countdown(2),
            moving(0.0),
            THROTTLE
        ));
    }
}
