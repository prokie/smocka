//! Slipstreams: a car driving close behind another, lined up with it, is pulled along up to
//! [`SLIPSTREAM_MAX_SPEED`](super::config::SLIPSTREAM_MAX_SPEED), fast enough to catch it, and
//! keeps that speed for [`SLIPSTREAM_LINGER`](super::config::SLIPSTREAM_LINGER) after pulling out
//! to pass (see [`CarState::draft`]). It lets the cars at the back catch up, and
//! catching up is where bumping happens.
//!
//! Like bumps, it's worked out from where every car is, which every driver predicts (see
//! [`bumps`](super::bumps)), so each driver's machine works out the same slipstreams the host
//! does: nothing extra is sent.

use bevy::prelude::*;

use super::car::CarState;
use super::config::{
    SLIPSTREAM_HALF_WIDTH, SLIPSTREAM_LENGTH, SLIPSTREAM_MAX_ANGLE, SLIPSTREAM_MIN_SPEED,
};

/// Whether car `behind` is in car `ahead`'s slipstream: no further than [`SLIPSTREAM_LENGTH`]
/// behind it, within [`SLIPSTREAM_HALF_WIDTH`] of the line it's driving along, and pointing
/// within [`SLIPSTREAM_MAX_ANGLE`] of the same way. A car slower than [`SLIPSTREAM_MIN_SPEED`]
/// leaves no slipstream.
pub fn in_slipstream(behind: CarState, ahead: CarState) -> bool {
    if ahead.speed < SLIPSTREAM_MIN_SPEED {
        return false;
    }
    let forward = Vec2::from_angle(ahead.heading);
    let gap = ahead.position - behind.position;
    let back = gap.dot(forward);
    let aside = gap.dot(forward.perp()).abs();
    let lined_up = forward.dot(Vec2::from_angle(behind.heading)) >= SLIPSTREAM_MAX_ANGLE.cos();
    back > 0.0 && back <= SLIPSTREAM_LENGTH && aside <= SLIPSTREAM_HALF_WIDTH && lined_up
}

/// Whether `car` is in the slipstream of any of `others`.
pub fn in_any_slipstream(car: CarState, mut others: impl Iterator<Item = CarState>) -> bool {
    others.any(|other| in_slipstream(car, other))
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use bevy::math::Vec2;

    use super::{in_any_slipstream, in_slipstream};
    use crate::minigames::racing::car::CarState;
    use crate::minigames::racing::config::{
        MAX_SPEED, SLIPSTREAM_HALF_WIDTH, SLIPSTREAM_LENGTH, SLIPSTREAM_MAX_ANGLE,
        SLIPSTREAM_MIN_SPEED,
    };

    /// A car at (`x`, `y`) facing `heading`, going `speed` along it.
    fn car(x: f32, y: f32, heading: f32, speed: f32) -> CarState {
        CarState {
            speed,
            ..CarState::parked(Vec2::new(x, y), heading)
        }
    }

    /// A car racing right along y = 0 at top speed, with its centre at x = 0.
    const AHEAD: CarState = CarState {
        position: Vec2::ZERO,
        heading: 0.0,
        speed: MAX_SPEED,
        slide: Vec2::ZERO,
        boost: 0.0,
        draft: 0.0,
    };

    #[test]
    fn a_car_close_behind_and_lined_up_is_in_the_slipstream() {
        assert!(in_slipstream(car(-20.0, 0.0, 0.0, MAX_SPEED), AHEAD));
        assert!(in_slipstream(
            car(-SLIPSTREAM_LENGTH + 1.0, 0.0, 0.0, MAX_SPEED),
            AHEAD
        ));
        // A little off the line, and pointing a little across it, still counts.
        assert!(in_slipstream(
            car(-40.0, SLIPSTREAM_HALF_WIDTH - 1.0, 0.3, 100.0),
            AHEAD
        ));
    }

    #[test]
    fn a_car_too_far_back_or_to_the_side_isnt() {
        assert!(!in_slipstream(
            car(-SLIPSTREAM_LENGTH - 1.0, 0.0, 0.0, MAX_SPEED),
            AHEAD
        ));
        assert!(!in_slipstream(
            car(-20.0, SLIPSTREAM_HALF_WIDTH + 1.0, 0.0, MAX_SPEED),
            AHEAD
        ));
        assert!(!in_slipstream(
            car(-20.0, -SLIPSTREAM_HALF_WIDTH - 1.0, 0.0, MAX_SPEED),
            AHEAD
        ));
    }

    #[test]
    fn the_car_in_front_isnt_in_the_slipstream_of_the_car_behind() {
        let behind = car(-20.0, 0.0, 0.0, MAX_SPEED);
        assert!(!in_slipstream(AHEAD, behind));
        // Nor is a car level with it.
        assert!(!in_slipstream(car(0.0, 5.0, 0.0, MAX_SPEED), AHEAD));
    }

    #[test]
    fn a_car_crossing_behind_isnt() {
        // Coming up from below, side-on, as at the figure of eight's crossing.
        assert!(!in_slipstream(car(-20.0, 0.0, FRAC_PI_2, MAX_SPEED), AHEAD));
        assert!(!in_slipstream(
            car(-20.0, 0.0, SLIPSTREAM_MAX_ANGLE + 0.05, MAX_SPEED),
            AHEAD
        ));
    }

    #[test]
    fn the_slipstream_follows_the_way_the_car_ahead_points() {
        let ahead = car(0.0, 0.0, FRAC_PI_2, MAX_SPEED);
        assert!(in_slipstream(car(0.0, -30.0, FRAC_PI_2, MAX_SPEED), ahead));
        assert!(!in_slipstream(car(-30.0, 0.0, FRAC_PI_2, MAX_SPEED), ahead));
    }

    #[test]
    fn a_slow_or_parked_car_leaves_no_slipstream() {
        let behind = car(-20.0, 0.0, 0.0, MAX_SPEED);
        assert!(!in_slipstream(
            behind,
            car(0.0, 0.0, 0.0, SLIPSTREAM_MIN_SPEED - 1.0)
        ));
        assert!(!in_slipstream(behind, car(0.0, 0.0, 0.0, 0.0)));
        // Nor does one reversing.
        assert!(!in_slipstream(behind, car(0.0, 0.0, 0.0, -MAX_SPEED)));
    }

    #[test]
    fn any_car_ahead_will_do() {
        let behind = car(-20.0, 0.0, 0.0, MAX_SPEED);
        let elsewhere = car(100.0, 100.0, 0.0, MAX_SPEED);
        assert!(in_any_slipstream(behind, [elsewhere, AHEAD].into_iter()));
        assert!(!in_any_slipstream(behind, [elsewhere].into_iter()));
        assert!(!in_any_slipstream(behind, std::iter::empty()));
    }
}
