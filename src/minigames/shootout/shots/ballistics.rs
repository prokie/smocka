//! Pure swept-collision and ricochet geometry for shootout paint balls.
//!
//! Kept free of Bevy ECS and networking so shot hit-detection, wall reflections and lag catch-up
//! live in one small, deterministic module.

use bevy::math::{Rect, Vec2};
use lightyear::prelude::Tick;

use crate::minigames::shootout::config::{
    ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH, COVER, GUNNER_RADIUS, PAINT_BALL_BOUNCES,
    PAINT_BALL_RADIUS,
};

/// Where and at what surface normal a paint ball hit the border wall or a cover block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallImpact {
    /// Fraction `0.0..=1.0` along the tick's segment.
    pub t: f32,
    /// Unit normal pointing back into the open floor (away from the wall or cover face).
    pub normal: Vec2,
}

/// What a paint ball hit first along a tick's stretch, if anything.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BallHit<T> {
    /// Hit the border wall or a cover block.
    Wall(WallImpact),
    /// Hit `target` at fraction `0.0..=1.0` of the stretch.
    Gunner { t: f32, target: T },
}

/// Reflect `velocity` across unit surface `normal`.
pub fn reflect(velocity: Vec2, normal: Vec2) -> Vec2 {
    velocity - 2.0 * velocity.dot(normal) * normal
}

/// How far along its flight (including up to [`PAINT_BALL_BOUNCES`] wall ricochets) a shot fired
/// on `shot_tick` already is when the host is at `host_tick` (with fixed timestep `dt`). Returns
/// `None` if it already splatted on a wall while in transit.
pub fn catch_up_shot(
    origin: Vec2,
    velocity: Vec2,
    shot_tick: Tick,
    host_tick: Tick,
    dt: f32,
) -> Option<(Vec2, Vec2, u8)> {
    let mut remaining = (host_tick - shot_tick).max(0) as f32 * dt;
    let mut position = origin;
    let mut velocity = velocity;
    let mut bounces_left = PAINT_BALL_BOUNCES;
    while remaining > 0.0 {
        let target = position + velocity * remaining;
        match wall_hit(position, target) {
            Some(impact) if impact.t > 0.0 && bounces_left > 0 => {
                bounces_left -= 1;
                position = position.lerp(target, impact.t) + impact.normal * 0.05;
                velocity = reflect(velocity, impact.normal);
                remaining *= (1.0 - impact.t).max(0.0);
            }
            Some(_) => return None,
            None => return Some((target, velocity, bounces_left)),
        }
    }
    Some((position, velocity, bounces_left))
}

/// The earliest thing a paint ball moving from `start` to `end` hits: the border wall, any
/// [`COVER`] block, or one of `gunners` (each given as `(id, position)`). Comparing the hit
/// fractions guarantees cover blocks shots aimed at a gunner behind them, and that the front
/// gunner takes the hit if two line up.
pub fn check_ball_step<T: Copy>(
    start: Vec2,
    end: Vec2,
    gunners: impl IntoIterator<Item = (T, Vec2)>,
) -> Option<BallHit<T>> {
    let wall = wall_hit(start, end);
    let hit_radius = GUNNER_RADIUS + PAINT_BALL_RADIUS;
    let gunner = gunners
        .into_iter()
        .filter_map(|(target, centre)| {
            segment_hits_circle(start, end, centre, hit_radius).map(|t| (t, target))
        })
        .min_by(|(a, _), (b, _)| a.total_cmp(b));

    match (gunner, wall) {
        (Some((t_gunner, target)), Some(impact)) if t_gunner <= impact.t => Some(BallHit::Gunner {
            t: t_gunner,
            target,
        }),
        (Some((t_gunner, target)), None) => Some(BallHit::Gunner {
            t: t_gunner,
            target,
        }),
        (_, Some(impact)) => Some(BallHit::Wall(impact)),
        (None, None) => None,
    }
}

/// Earliest [`WallImpact`] where a paint ball moving from `start` to `end` touches the border
/// wall or any [`COVER`] block.
pub fn wall_hit(start: Vec2, end: Vec2) -> Option<WallImpact> {
    let playable = Rect::from_center_half_size(
        Vec2::ZERO,
        Vec2::new(
            ARENA_HALF_WIDTH - PAINT_BALL_RADIUS,
            ARENA_HALF_HEIGHT - PAINT_BALL_RADIUS,
        ),
    );
    let border = segment_exits_rect(start, end, playable);
    let cover = COVER
        .into_iter()
        .filter_map(|block| segment_hits_rect(start, end, block.inflate(PAINT_BALL_RADIUS)));
    border
        .into_iter()
        .chain(cover)
        .min_by(|a, b| a.t.total_cmp(&b.t))
}

/// Earliest [`WallImpact`] where the segment `start..=end` leaves `rect`, or `t = 0.0` if `start`
/// is already outside it.
pub fn segment_exits_rect(start: Vec2, end: Vec2, rect: Rect) -> Option<WallImpact> {
    if !rect.contains(start) {
        let mut normal = Vec2::ZERO;
        if start.x > rect.max.x {
            normal.x = -1.0;
        } else if start.x < rect.min.x {
            normal.x = 1.0;
        }
        if start.y > rect.max.y {
            normal.y = -1.0;
        } else if start.y < rect.min.y {
            normal.y = 1.0;
        }
        return Some(WallImpact {
            t: 0.0,
            normal: normal.normalize_or(Vec2::X),
        });
    }
    let delta = end - start;
    let mut earliest: Option<WallImpact> = None;
    let mut consider = |t: f32, normal: Vec2| {
        let t = t.clamp(0.0, 1.0);
        if earliest.is_none_or(|best| t < best.t) {
            earliest = Some(WallImpact { t, normal });
        }
    };
    if delta.x > 0.0 && end.x > rect.max.x {
        consider((rect.max.x - start.x) / delta.x, Vec2::NEG_X);
    } else if delta.x < 0.0 && end.x < rect.min.x {
        consider((rect.min.x - start.x) / delta.x, Vec2::X);
    }
    if delta.y > 0.0 && end.y > rect.max.y {
        consider((rect.max.y - start.y) / delta.y, Vec2::NEG_Y);
    } else if delta.y < 0.0 && end.y < rect.min.y {
        consider((rect.min.y - start.y) / delta.y, Vec2::Y);
    }
    earliest
}

/// Earliest [`WallImpact`] where the segment `start..=end` enters (or starts inside) `rect`.
pub fn segment_hits_rect(start: Vec2, end: Vec2, rect: Rect) -> Option<WallImpact> {
    let delta = end - start;
    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;
    let mut normal = (-delta).normalize_or(Vec2::X);
    for (origin, dir, min, max, neg_normal, pos_normal) in [
        (
            start.x,
            delta.x,
            rect.min.x,
            rect.max.x,
            Vec2::NEG_X,
            Vec2::X,
        ),
        (
            start.y,
            delta.y,
            rect.min.y,
            rect.max.y,
            Vec2::NEG_Y,
            Vec2::Y,
        ),
    ] {
        if dir.abs() < 1e-8 {
            if origin < min || origin > max {
                return None;
            }
        } else {
            let inv = 1.0 / dir;
            let (t1, t2, face_normal) = if dir > 0.0 {
                ((min - origin) * inv, (max - origin) * inv, neg_normal)
            } else {
                ((max - origin) * inv, (min - origin) * inv, pos_normal)
            };
            if t1 > t_min {
                t_min = t1;
                normal = face_normal;
            }
            t_max = t_max.min(t2);
            if t_min > t_max {
                return None;
            }
        }
    }
    Some(WallImpact { t: t_min, normal })
}

/// Earliest `t` in `0.0..=1.0` where the segment `start..=end` is within `radius` of `centre`.
pub fn segment_hits_circle(start: Vec2, end: Vec2, centre: Vec2, radius: f32) -> Option<f32> {
    let offset = start - centre;
    let r_sq = radius * radius;
    if offset.length_squared() <= r_sq {
        return Some(0.0);
    }
    let delta = end - start;
    let a = delta.length_squared();
    if a < 1e-8 {
        return None;
    }
    let b = 2.0 * offset.dot(delta);
    let c = offset.length_squared() - r_sq;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let t = (-b - discriminant.sqrt()) / (2.0 * a);
    (0.0..=1.0).contains(&t).then_some(t)
}

#[cfg(test)]
mod tests {
    use bevy::math::{Rect, Vec2};
    use lightyear::prelude::Tick;

    use super::{
        BallHit, catch_up_shot, check_ball_step, reflect, segment_exits_rect, segment_hits_circle,
        segment_hits_rect,
    };
    use crate::minigames::shootout::config::{
        ARENA_HALF_WIDTH, COVER, GUNNER_RADIUS, PAINT_BALL_BOUNCES, PAINT_BALL_RADIUS,
    };

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn fast_ball_cannot_skip_through_a_gunner() {
        // A huge single-tick step right across a gunner at (0, 120).
        let hit = check_ball_step(
            Vec2::new(-100.0, 120.0),
            Vec2::new(100.0, 120.0),
            [(1u32, Vec2::new(0.0, 120.0))],
        );
        let expected_t = (100.0 - (GUNNER_RADIUS + PAINT_BALL_RADIUS)) / 200.0;
        match hit {
            Some(BallHit::Gunner { t, target: 1 }) => assert!(close(t, expected_t), "{t}"),
            other => panic!("expected gunner 1 hit, got {other:?}"),
        }
    }

    #[test]
    fn cover_stops_a_shot_before_it_reaches_a_gunner_behind_it() {
        // Middle cover block is centred on (0, 0), 48×48; gunner hides behind it at (60, 0).
        let hit = check_ball_step(
            Vec2::new(-60.0, 0.0),
            Vec2::new(100.0, 0.0),
            [(1u32, Vec2::new(60.0, 0.0))],
        );
        assert!(matches!(hit, Some(BallHit::Wall(_))), "{hit:?}");
    }

    #[test]
    fn closer_of_two_lined_up_gunners_takes_the_hit() {
        let hit = check_ball_step(
            Vec2::new(-100.0, 120.0),
            Vec2::new(100.0, 120.0),
            [
                (2u32, Vec2::new(40.0, 120.0)),
                (1u32, Vec2::new(-20.0, 120.0)),
            ],
        );
        assert!(
            matches!(hit, Some(BallHit::Gunner { target: 1, .. })),
            "{hit:?}"
        );
    }

    #[test]
    fn ball_stops_at_the_border_wall() {
        let edge = ARENA_HALF_WIDTH - PAINT_BALL_RADIUS;
        let hit = check_ball_step(
            Vec2::new(edge - 5.0, 120.0),
            Vec2::new(edge + 15.0, 120.0),
            std::iter::empty::<(u32, Vec2)>(),
        );
        match hit {
            Some(BallHit::Wall(impact)) => {
                assert!(close(impact.t, 0.25), "{impact:?}");
                assert_eq!(impact.normal, Vec2::NEG_X);
            }
            other => panic!("expected border wall hit, got {other:?}"),
        }
    }

    #[test]
    fn shot_starting_inside_cover_hits_immediately() {
        let block = COVER[0];
        let hit = check_ball_step(
            block.center(),
            block.center() + Vec2::new(10.0, 0.0),
            std::iter::empty::<(u32, Vec2)>(),
        );
        assert!(
            matches!(hit, Some(BallHit::Wall(impact)) if close(impact.t, 0.0)),
            "{hit:?}"
        );
    }

    #[test]
    fn segment_circle_misses_when_passing_to_the_side_or_away() {
        assert_eq!(
            segment_hits_circle(
                Vec2::new(-50.0, 20.0),
                Vec2::new(50.0, 20.0),
                Vec2::ZERO,
                10.0
            ),
            None
        );
        assert_eq!(
            segment_hits_circle(Vec2::new(20.0, 0.0), Vec2::new(50.0, 0.0), Vec2::ZERO, 10.0),
            None
        );
        // Already overlapping at start hits at t = 0.
        assert_eq!(
            segment_hits_circle(Vec2::new(5.0, 0.0), Vec2::new(20.0, 0.0), Vec2::ZERO, 10.0),
            Some(0.0)
        );
    }

    #[test]
    fn segment_rect_helpers_handle_inside_and_parallel_cases() {
        let rect = Rect::from_center_half_size(Vec2::ZERO, Vec2::splat(10.0));
        assert_eq!(
            segment_exits_rect(Vec2::new(0.0, 0.0), Vec2::new(5.0, 0.0), rect),
            None
        );
        assert!(close(
            segment_exits_rect(Vec2::new(20.0, 0.0), Vec2::new(25.0, 0.0), rect)
                .unwrap()
                .t,
            0.0
        ));
        assert_eq!(
            segment_hits_rect(Vec2::new(-20.0, 15.0), Vec2::new(20.0, 15.0), rect),
            None
        );
        let impact = segment_hits_rect(Vec2::new(-20.0, 0.0), Vec2::new(20.0, 0.0), rect).unwrap();
        assert!(close(impact.t, 0.25));
        assert_eq!(impact.normal, Vec2::NEG_X);
        assert_eq!(
            reflect(Vec2::new(100.0, 50.0), Vec2::NEG_X),
            Vec2::new(-100.0, 50.0)
        );
    }

    #[test]
    fn catch_up_advances_a_delayed_shot_bounces_once_and_drops_on_second_wall() {
        let dt = 1.0 / 128.0;
        let origin = Vec2::new(-100.0, 140.0);
        let velocity = Vec2::new(600.0, 0.0);

        // No delay or slightly early estimate leaves the ball at its origin.
        assert_eq!(
            catch_up_shot(origin, velocity, Tick(50), Tick(50), dt),
            Some((origin, velocity, PAINT_BALL_BOUNCES))
        );
        assert_eq!(
            catch_up_shot(origin, velocity, Tick(50), Tick(48), dt),
            Some((origin, velocity, PAINT_BALL_BOUNCES))
        );

        // 8 ticks behind advances 8 ticks along its line.
        let (caught_up, caught_vel, bounces) =
            catch_up_shot(origin, velocity, Tick(50), Tick(58), dt).unwrap();
        assert!(
            caught_up.distance(origin + velocity * (8.0 * dt)) < 1e-4,
            "{caught_up}"
        );
        assert_eq!(caught_vel, velocity);
        assert_eq!(bounces, PAINT_BALL_BOUNCES);

        // A shot fired into the centre cover bounces back with 0 bounces left.
        let (pos, vel, bounces) =
            catch_up_shot(Vec2::new(-40.0, 0.0), velocity, Tick(50), Tick(60), dt).unwrap();
        assert!(vel.x < 0.0, "expected reflection: {vel}");
        assert_eq!(bounces, 0);
        assert!(pos.x < -27.0, "{pos}");

        // After enough time to hit a second wall (the left cover block at x = -260), it splats.
        assert_eq!(
            catch_up_shot(Vec2::new(-40.0, 0.0), velocity, Tick(50), Tick(180), dt),
            None
        );
    }
}
