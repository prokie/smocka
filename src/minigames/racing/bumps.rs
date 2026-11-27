//! Car-to-car bumping: cars that touch are pushed apart, and if they hit each other they bounce
//! like two equal weights. A rear-ended car speeds up while the car behind slows down; a car hit
//! from the side slides away (see [`CarState::slide`]).
//!
//! Every driver predicts every car, so each driver's machine works out the same bumps the host
//! does, from the same states, instead of driving through a car it only sees as it was a moment
//! ago. Spectators interpolate the host's results.

use bevy::prelude::*;
use lightyear::prelude::{Interpolated, PeerId, Rollback};

use super::PHASE;
use super::RacingSystems;
use super::car::{Car, CarState, in_bounds};
use super::config::{BOOST_MAX_SPEED, BUMP_RESTITUTION, CAR_LENGTH, CAR_WIDTH, MAX_REVERSE_SPEED};
use super::track::TrackId;
use crate::config::network::TICK_RATE;

// A car's outline is a capsule along its length, so it must be longer than it is wide.
const _: () = assert!(CAR_LENGTH > CAR_WIDTH);
// Two cars driving at each other at their fastest must touch on some tick, not pass through each
// other between two.
const _: () = assert!(2.0 * BOOST_MAX_SPEED as f64 / TICK_RATE < CAR_WIDTH as f64);

/// Half the width of a car's outline: a capsule (a rectangle with round ends) as long and wide as
/// the car.
const RADIUS: f32 = CAR_WIDTH / 2.0;
/// Half the length of the line down the middle of a car's outline, between the centres of its
/// round ends.
const HALF_SPINE: f32 = (CAR_LENGTH - CAR_WIDTH) / 2.0;

/// Where two cars touch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// From the second car towards the first: the way the first car is pushed.
    pub normal: Dir2,
    /// How far the cars overlap, in world units.
    pub depth: f32,
}

/// Two cars after bumping into each other.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bump {
    pub a: CarState,
    pub b: CarState,
    /// How fast the cars were closing on each other, in world units per second. Zero if they
    /// weren't, say because they were already moving apart.
    pub impact: f32,
}

/// Two cars just bumped into each other. Triggered locally (never sent) where bumps are worked
/// out, on the host and on every driver's machine, once when the cars first touch. Rollbacks that
/// replay a bump don't trigger it again.
#[derive(Event, Debug, Clone, Copy, PartialEq)]
pub struct CarsBumped {
    /// How fast the cars were closing on each other, in world units per second (see
    /// [`Bump::impact`]).
    pub impact: f32,
}

/// Bumps cars into each other on the host and on every driver's machine.
pub struct BumpPlugin;

impl Plugin for BumpPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            bump_cars
                .in_set(RacingSystems::Bump)
                .run_if(in_state(PHASE)),
        );
    }
}

/// Where cars `a` and `b` touch, if they do.
pub fn contact(a: CarState, b: CarState) -> Option<Contact> {
    let (on_a, on_b) = closest_points(spine(a), spine(b));
    let gap = on_a - on_b;
    let depth = 2.0 * RADIUS - gap.length();
    if depth <= 0.0 {
        return None;
    }
    // Spines that cross give no direction: push the cars apart along the line between their
    // centres instead, or out to the left of `a` if they're in the same place.
    let normal = Dir2::new(gap)
        .or_else(|_| Dir2::new(a.position - b.position))
        .unwrap_or_else(|_| Dir2::new_unchecked(Vec2::from_angle(a.heading).perp()));
    Some(Contact { normal, depth })
}

/// Cars `a` and `b` after bumping into each other, if they touch. They're pushed apart until they
/// only just touch, each going half the way (as far as `bounds`, the track's edges, let it). If
/// they were closing on each other, they bounce apart like equal weights, at [`BUMP_RESTITUTION`]
/// times the speed they came together at.
pub fn bump(a: CarState, b: CarState, bounds: Vec2) -> Option<Bump> {
    let Contact { normal, depth } = contact(a, b)?;
    let push = normal * (depth / 2.0);
    let mut a = CarState {
        position: in_bounds(a.position + push, bounds),
        ..a
    };
    let mut b = CarState {
        position: in_bounds(b.position - push, bounds),
        ..b
    };
    let impact = (b.velocity() - a.velocity()).dot(*normal).max(0.0);
    if impact > 0.0 {
        // Equal weights share the change in their closing speed equally.
        let kick = normal * ((1.0 + BUMP_RESTITUTION) * impact / 2.0);
        a = kicked(a, kick);
        b = kicked(b, -kick);
    }
    Some(Bump { a, b, impact })
}

/// `car` after a bump changes its velocity by `change`. The part along the car's heading goes into
/// its speed, as far as its top speeds allow; the rest (sideways, or past a top speed) into its
/// slide, so the change is kept in full either way. A car already past its top speed (after a
/// boost, say) keeps that speed.
fn kicked(car: CarState, change: Vec2) -> CarState {
    let forward = Vec2::from_angle(car.heading);
    let top = car.top_speed().max(car.speed);
    let speed = (car.speed + change.dot(forward)).clamp(-MAX_REVERSE_SPEED, top);
    CarState {
        speed,
        slide: car.slide + change - forward * (speed - car.speed),
        ..car
    }
}

/// The line down the middle of the car's outline, rear end first.
fn spine(car: CarState) -> (Vec2, Vec2) {
    let half = Vec2::from_angle(car.heading) * HALF_SPINE;
    (car.position - half, car.position + half)
}

/// The closest points between two line segments, one on each, which must both be longer than 0.
/// From Ericson, *Real-Time Collision Detection*, section 5.1.9.
fn closest_points((p1, q1): (Vec2, Vec2), (p2, q2): (Vec2, Vec2)) -> (Vec2, Vec2) {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.length_squared();
    let e = d2.length_squared();
    let b = d1.dot(d2);
    let c = d1.dot(r);
    let f = d2.dot(r);
    let denominator = a * e - b * b;
    // Parallel segments have a whole stretch of closest points: start from p1's end.
    let s = if denominator > f32::EPSILON * a * e {
        ((b * f - c * e) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let t = (b * s + f) / e;
    let (s, t) = if t < 0.0 {
        ((-c / a).clamp(0.0, 1.0), 0.0)
    } else if t > 1.0 {
        (((b - c) / a).clamp(0.0, 1.0), 1.0)
    } else {
        (s, t)
    };
    (p1 + d1 * s, p2 + d2 * t)
}

/// Bump every pair of touching cars. Pairs go in the order of their drivers' [`PeerId`]s, so the
/// host and every driver work through them the same way. Runs on the host for every car and on
/// each driver's machine for the cars it predicts (all of them); spectators' cars are interpolated,
/// so it leaves them alone.
///
/// `touching` remembers which pairs touched on the last tick, so each bump is logged, and
/// [`CarsBumped`] triggered, once, when it starts. Rollbacks replay ticks a driver's machine has
/// already done that for, so they do neither.
fn bump_cars(
    rollback: Option<Res<Rollback>>,
    track: Option<Single<&TrackId>>,
    mut cars: Query<(Entity, &Car, &mut CarState), Without<Interpolated>>,
    mut touching: Local<Vec<(Entity, Entity)>>,
    mut commands: Commands,
) {
    let Some(bounds) = track.map(|id| id.track().car_bounds()) else {
        return;
    };
    let mut order: Vec<(PeerId, Entity)> = cars
        .iter()
        .map(|(entity, car, _)| (car.owner, entity))
        .collect();
    order.sort_unstable();
    let replaying = rollback.is_some();
    let mut now_touching = Vec::new();
    for (i, &(_, first)) in order.iter().enumerate() {
        for &(_, second) in &order[i + 1..] {
            let Ok([(_, car_a, mut a), (_, car_b, mut b)]) = cars.get_many_mut([first, second])
            else {
                continue;
            };
            let Some(bump) = bump(*a, *b, bounds) else {
                continue;
            };
            a.set_if_neq(bump.a);
            b.set_if_neq(bump.b);
            if replaying {
                continue;
            }
            if !touching.contains(&(first, second)) {
                debug!(
                    "{} ({:?}) and {} ({:?}) bumped into each other at {:.0} units/s",
                    car_a.name, car_a.owner, car_b.name, car_b.owner, bump.impact
                );
                commands.trigger(CarsBumped {
                    impact: bump.impact,
                });
            }
            now_touching.push((first, second));
        }
    }
    if !replaying {
        *touching = now_touching;
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, PI};

    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use lightyear::prelude::{Interpolated, PeerId};

    use super::{Bump, bump_cars, contact};
    use crate::minigames::racing::car::{Car, CarState};
    use crate::minigames::racing::config::{
        BOOST_MAX_SPEED, BUMP_RESTITUTION, CAR_LENGTH, CAR_WIDTH, MAX_REVERSE_SPEED, MAX_SPEED,
    };
    use crate::minigames::racing::track::TrackId;

    const BOUNDS: Vec2 = Vec2::new(632.0, 352.0);

    fn bump(a: CarState, b: CarState) -> Option<Bump> {
        super::bump(a, b, BOUNDS)
    }

    /// A car at (`x`, `y`) facing `heading`, going `speed` along it.
    fn car(x: f32, y: f32, heading: f32, speed: f32) -> CarState {
        CarState {
            speed,
            ..CarState::parked(Vec2::new(x, y), heading)
        }
    }

    /// The total momentum of two equal cars.
    fn momentum(bump: &Bump) -> Vec2 {
        bump.a.velocity() + bump.b.velocity()
    }

    fn assert_close(actual: Vec2, expected: Vec2) {
        assert!(
            actual.abs_diff_eq(expected, 1e-3),
            "{actual} isn't {expected}"
        );
    }

    #[test]
    fn cars_apart_dont_bump() {
        assert_eq!(
            bump(car(0.0, 0.0, 0.0, 100.0), car(30.0, 0.0, 0.0, 0.0)),
            None
        );
    }

    #[test]
    fn the_outline_is_as_long_and_wide_as_the_car() {
        // Nose to tail, then side by side, then corner to corner.
        let behind = |gap: f32| car(-CAR_LENGTH - gap, 0.0, 0.0, 0.0);
        let beside = |gap: f32| car(0.0, -CAR_WIDTH - gap, 0.0, 0.0);
        let parked = car(0.0, 0.0, 0.0, 0.0);
        assert!(contact(parked, behind(-0.1)).is_some());
        assert!(contact(parked, behind(0.1)).is_none());
        assert!(contact(parked, beside(-0.1)).is_some());
        assert!(contact(parked, beside(0.1)).is_none());
        // The ends are round, so cars whose boxes' corners just overlap don't touch.
        let corner = car(-CAR_LENGTH + 0.5, -CAR_WIDTH + 0.5, 0.0, 0.0);
        assert!(contact(parked, corner).is_none());
    }

    #[test]
    fn the_contact_pushes_the_first_car_away_from_the_second() {
        let contact = contact(car(0.0, 0.0, 0.0, 0.0), car(-15.0, 0.0, 0.0, 0.0)).unwrap();
        assert_eq!(contact.normal, Dir2::X);
        assert!((contact.depth - 1.0).abs() < 1e-4, "{contact:?}");
    }

    #[test]
    fn touching_cars_are_pushed_apart_until_they_just_touch() {
        let bumped = bump(car(0.0, 0.0, 0.0, 0.0), car(4.0, 6.0, 0.5, 0.0)).unwrap();
        let left = contact(bumped.a, bumped.b).map_or(0.0, |contact| contact.depth);
        assert!(left < 1e-3, "still overlapping by {left}");
        // Each went half the way.
        let a_moved = bumped.a.position.length();
        let b_moved = bumped.b.position.distance(Vec2::new(4.0, 6.0));
        assert!((a_moved - b_moved).abs() < 1e-3);
    }

    #[test]
    fn rear_ending_slows_the_car_behind_and_speeds_up_the_one_in_front() {
        let behind = car(-15.0, 0.0, 0.0, 200.0);
        let front = car(0.0, 0.0, 0.0, 100.0);
        let bumped = bump(behind, front).unwrap();
        assert_eq!(bumped.impact, 100.0);
        // Equal weights share the change: 70 each at a restitution of 0.4.
        let change = (1.0 + BUMP_RESTITUTION) * 100.0 / 2.0;
        assert!(
            (bumped.a.speed - (200.0 - change)).abs() < 1e-3,
            "{bumped:?}"
        );
        assert!(
            (bumped.b.speed - (100.0 + change)).abs() < 1e-3,
            "{bumped:?}"
        );
        assert_close(bumped.a.slide, Vec2::ZERO);
        assert_close(bumped.b.slide, Vec2::ZERO);
        assert_close(momentum(&bumped), Vec2::new(300.0, 0.0));
    }

    #[test]
    fn a_boosted_car_rear_ending_another_keeps_its_speed_rather_than_sliding() {
        let behind = CarState {
            boost: 0.5,
            ..car(-15.0, 0.0, 0.0, BOOST_MAX_SPEED)
        };
        let front = car(0.0, 0.0, 0.0, BOOST_MAX_SPEED - 60.0);
        let bumped = bump(behind, front).unwrap();
        assert!(bumped.a.speed > MAX_SPEED, "{bumped:?}");
        assert_close(bumped.a.slide, Vec2::ZERO);
        // The car in front isn't boosted, so it's past its own top speed: it keeps that speed, and
        // the rest of the shove is a slide that soon wears off.
        assert_eq!(bumped.b.speed, BOOST_MAX_SPEED - 60.0);
        assert!(bumped.b.slide.x > 0.0, "{bumped:?}");
        assert_close(
            momentum(&bumped),
            Vec2::new(2.0 * BOOST_MAX_SPEED - 60.0, 0.0),
        );
    }

    #[test]
    fn a_car_hit_from_the_side_slides_away() {
        // Driving up into the side of a car parked facing right.
        let hitting = car(0.0, -12.0, FRAC_PI_2, 150.0);
        let parked = car(0.0, 0.0, 0.0, 0.0);
        let bumped = bump(hitting, parked).unwrap();
        assert!(bumped.b.speed.abs() < 1e-3, "a shove sideways isn't speed");
        assert!(bumped.b.slide.y > 0.0 && bumped.b.slide.x.abs() < 1e-3);
        assert!(bumped.a.speed < 150.0);
        assert_close(momentum(&bumped), Vec2::new(0.0, 150.0));
    }

    #[test]
    fn a_head_on_crash_sends_both_back_no_faster_than_they_can_reverse() {
        let bumped = bump(
            car(-15.0, 0.0, 0.0, MAX_SPEED),
            car(0.0, 0.0, PI, MAX_SPEED),
        )
        .unwrap();
        for state in [bumped.a, bumped.b] {
            assert_eq!(state.speed, -MAX_REVERSE_SPEED);
            // The rest of the bounce is a slide backwards.
            let backwards = -Vec2::from_angle(state.heading);
            assert!(state.slide.dot(backwards) > 0.0, "{state:?}");
        }
        assert_close(momentum(&bumped), Vec2::ZERO);
    }

    #[test]
    fn touching_cars_moving_apart_keep_their_speeds() {
        let bumped = bump(car(0.0, 0.0, 0.0, 150.0), car(-15.0, 0.0, 0.0, 100.0)).unwrap();
        assert_eq!(bumped.impact, 0.0);
        assert_eq!((bumped.a.speed, bumped.b.speed), (150.0, 100.0));
    }

    #[test]
    fn it_doesnt_matter_which_car_comes_first() {
        let one = car(-14.0, 2.0, 0.3, 180.0);
        let other = car(0.0, 0.0, -0.2, 60.0);
        let forwards = bump(one, other).unwrap();
        let backwards = bump(other, one).unwrap();
        assert!((forwards.impact - backwards.impact).abs() < 1e-3);
        assert_close(forwards.a.position, backwards.b.position);
        assert_close(forwards.a.velocity(), backwards.b.velocity());
        assert_close(forwards.b.velocity(), backwards.a.velocity());
    }

    #[test]
    fn cars_in_the_same_place_are_still_pushed_apart() {
        let bumped = bump(car(5.0, 5.0, 0.0, 0.0), car(5.0, 5.0, 0.0, 0.0)).unwrap();
        assert!(bumped.a.position.is_finite() && bumped.b.position.is_finite());
        assert!(bumped.a.position.distance(bumped.b.position) > 1.0);
    }

    #[test]
    fn a_bump_never_pushes_a_car_out_of_bounds() {
        let at_edge = car(BOUNDS.x, 0.0, 0.0, 0.0);
        let bumped = bump(at_edge, car(BOUNDS.x - 12.0, 0.0, 0.0, 0.0)).unwrap();
        assert_eq!(bumped.a.position.x, BOUNDS.x);
    }

    /// Run `bump_cars` once on `cars`, each given as its driver's number, its state, and whether
    /// it's interpolated, and return their states after.
    fn run_bump_cars(cars: &[(u64, CarState, bool)]) -> Vec<CarState> {
        let mut world = World::new();
        world.spawn(TrackId::default());
        let entities: Vec<Entity> = cars
            .iter()
            .map(|&(driver, state, interpolated)| {
                let car = Car {
                    owner: PeerId::Netcode(driver),
                    name: format!("Player {driver}"),
                    color: 0,
                };
                let mut entity = world.spawn((car, state));
                if interpolated {
                    entity.insert(Interpolated);
                }
                entity.id()
            })
            .collect();
        world.run_system_once(bump_cars).unwrap();
        entities
            .into_iter()
            .map(|entity| *world.get::<CarState>(entity).unwrap())
            .collect()
    }

    #[test]
    fn bump_cars_bumps_every_touching_pair() {
        let states = run_bump_cars(&[
            (1, car(0.0, 0.0, 0.0, 0.0), false),
            (2, car(15.0, 0.0, 0.0, 0.0), false),
            (3, car(100.0, 0.0, 0.0, 0.0), false),
        ]);
        let left = contact(states[0], states[1]).map_or(0.0, |contact| contact.depth);
        assert!(left < 1e-3, "still overlapping by {left}");
        assert_eq!(states[2], car(100.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn bump_cars_leaves_spectators_cars_to_interpolation() {
        let overlapping = [car(0.0, 0.0, 0.0, 0.0), car(15.0, 0.0, 0.0, 0.0)];
        let states = run_bump_cars(&[(1, overlapping[0], true), (2, overlapping[1], true)]);
        assert_eq!(states, overlapping);
    }

    #[test]
    fn bump_cars_goes_by_driver_whatever_order_the_cars_were_spawned_in() {
        // Three cars in a row, the middle one touching both: the order the pairs are bumped in
        // changes the result, so it must be the same on every peer.
        let row = [
            car(0.0, 0.0, 0.0, 100.0),
            car(15.0, 0.0, 0.0, 0.0),
            car(30.0, 0.0, 0.0, -50.0),
        ];
        let spawned = run_bump_cars(&[(1, row[0], false), (2, row[1], false), (3, row[2], false)]);
        let reversed = run_bump_cars(&[(3, row[2], false), (2, row[1], false), (1, row[0], false)]);
        assert_eq!(spawned, reversed.into_iter().rev().collect::<Vec<_>>());
    }
}
