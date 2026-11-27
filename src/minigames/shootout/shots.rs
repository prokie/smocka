//! Paint balls: firing, swept hit detection along each tick's stretch, and broadcasting shots as
//! where, which way and on which tick rather than as replicated entities.
//!
//! Each peer simulates paint balls along their straight lines locally. The shooting peer spawns
//! its own cosmetic ball immediately on the tick it fires (preventing duplicates across rollback),
//! while the host spawns an authoritative ball, broadcasts [`ShotFired`] to everyone else, and
//! decides hits, hit points and knockouts.

use bevy::prelude::*;
use lightyear::prelude::client::RemoteTimeline;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::{server::ClientOf, *};
use serde::{Deserialize, Serialize};

use super::config::{
    ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH, COVER, FIRE_COOLDOWN_TICKS, GUNNER_RADIUS, MUZZLE_OFFSET,
    PAINT_BALL_RADIUS, PAINT_BALL_SPEED,
};
use super::gunner::{Gunner, GunnerState, Health, NextShot};
use super::round::ShootoutDirector;
use super::shootout_input::ShootoutInput;
use super::{PHASE, ShootoutSystems};
use crate::minigames::smoothing::PreviousTick;
use crate::network::is_host;

/// The channel shootout shots travel on: reliable and unordered, since each shot carries its own
/// starting tick and flies independently.
pub struct ShootoutChannel;

/// Host to other peers: a gunner fired a paint ball. Sent to everyone except the shooter (who
/// already drew their own) and the host's own client (which already holds the authoritative ball).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ShotFired {
    /// Who fired it (also who it cannot hit).
    pub shooter: PeerId,
    /// The shooter's [`Player::color`](crate::players::Player::color), for tinting the ball even
    /// if the shooter leaves.
    pub color: u8,
    /// Where the ball starts, in world units (the barrel tip).
    pub origin: Vec2,
    /// Direction of travel, in radians anticlockwise from facing right (+x).
    pub angle: f32,
    /// The tick on which the shot was fired.
    pub tick: Tick,
}

/// A paint ball in flight. Never replicated: every peer spawns and steps its own local entities,
/// and only the host's `authoritative` balls deal damage or knock gunners out.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PaintBall {
    pub shooter: PeerId,
    pub color: u8,
    pub velocity: Vec2,
    pub authoritative: bool,
}

/// Local only: triggered when a paint ball hits the border wall or a cover block, so
/// [`sounds`](super::sounds) can play a splat.
#[derive(Event, Debug, Clone, Copy)]
pub struct PaintBallWallHit;

/// Where a [`PaintBall`] is, in world units. Smoothed between fixed ticks when drawn.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PaintBallPosition(pub Vec2);

impl Ease for PaintBallPosition {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            PaintBallPosition(start.0.lerp(end.0, t))
        })
    }
}

/// Local only (on joiners): the latest tick for which this client already spawned a cosmetic
/// paint ball for its own gunner, so a Lightyear rollback replaying that tick doesn't spawn a
/// duplicate. Reset each shootout.
#[derive(Resource, Default, Debug)]
struct LastLocalShot(Option<Tick>);

/// Registers [`ShotFired`] and runs firing and paint ball movement on every peer.
pub struct ShotsPlugin;

impl Plugin for ShotsPlugin {
    fn build(&self, app: &mut App) {
        app.add_channel::<ShootoutChannel>(ChannelSettings {
            mode: ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<ShotFired>()
            .add_direction(NetworkDirection::ServerToClient);

        app.init_resource::<LastLocalShot>();
        app.add_systems(OnEnter(PHASE), reset_last_local_shot);
        app.add_systems(
            FixedUpdate,
            (
                fire_gunners_host
                    .in_set(ShootoutSystems::Fire)
                    .run_if(is_host),
                fire_gunner_local
                    .in_set(ShootoutSystems::Fire)
                    .run_if(not(is_host)),
                step_paint_balls.in_set(ShootoutSystems::MoveBalls),
            )
                .run_if(in_state(PHASE)),
        );
        // Runs in `Update` so shots aren't missed on high-refresh frames with zero fixed ticks.
        app.add_systems(
            Update,
            receive_shots.run_if(in_state(PHASE).and_then(not(is_host))),
        );
    }
}

fn reset_last_local_shot(mut last: ResMut<LastLocalShot>) {
    last.0 = None;
}

/// Where a shot leaves `state`'s barrel, in world units.
pub fn muzzle_position(state: GunnerState) -> Vec2 {
    state.position + Vec2::from_angle(state.aim) * MUZZLE_OFFSET
}

fn spawn_paint_ball(
    commands: &mut Commands,
    shooter: PeerId,
    color: u8,
    origin: Vec2,
    angle: f32,
    authoritative: bool,
) {
    commands.spawn((
        PaintBall {
            shooter,
            color,
            velocity: Vec2::from_angle(angle) * PAINT_BALL_SPEED,
            authoritative,
        },
        PaintBallPosition(origin),
        PreviousTick(PaintBallPosition(origin)),
        DespawnOnExit(PHASE),
    ));
}

/// Remote client links that receive [`ShotFired`] from the host (excludes the host's own client).
type RemoteShotLinks<'w, 's> = Query<
    'w,
    's,
    (&'static RemoteId, &'static mut MessageSender<ShotFired>),
    (With<ClientOf>, Without<Client>),
>;

/// The components read when this peer's own predicted gunner fires locally.
type LocalGunner<'w> = (
    &'w Gunner,
    &'w GunnerState,
    &'w mut NextShot,
    &'w ActionState<ShootoutInput>,
);

/// Host only: fire gunners holding Fire whose cooldown is up, spawn authoritative balls, and
/// broadcast [`ShotFired`] to every remote client other than the shooter.
fn fire_gunners_host(
    timeline: Res<LocalTimeline>,
    mut gunners: Query<(
        &Gunner,
        &GunnerState,
        &mut NextShot,
        Option<&ActionState<ShootoutInput>>,
    )>,
    mut links: RemoteShotLinks,
    mut commands: Commands,
) {
    let tick = timeline.tick();
    for (gunner, state, mut next_shot, input) in &mut gunners {
        let firing = input.is_some_and(|input| input.0.fire);
        if !firing || !next_shot.ready(tick) {
            continue;
        }
        next_shot.0 = Some(tick + FIRE_COOLDOWN_TICKS as i32);
        let origin = muzzle_position(*state);
        let angle = state.aim;
        spawn_paint_ball(
            &mut commands,
            gunner.owner,
            gunner.color,
            origin,
            angle,
            true,
        );
        let shot = ShotFired {
            shooter: gunner.owner,
            color: gunner.color,
            origin,
            angle,
            tick,
        };
        for (remote_id, mut sender) in &mut links {
            if remote_id.0 != gunner.owner {
                sender.send::<ShootoutChannel>(shot);
            }
        }
    }
}

/// Joiners only: update this player's predicted [`NextShot`] and spawn their own cosmetic paint
/// ball the moment they fire, without waiting for a round trip to the host.
fn fire_gunner_local(
    timeline: Res<LocalTimeline>,
    mut last_shot: ResMut<LastLocalShot>,
    mut gunner: Single<LocalGunner, With<InputMarker<ShootoutInput>>>,
    mut commands: Commands,
) {
    let tick = timeline.tick();
    let (gunner, state, ref mut next_shot, input) = *gunner;
    if !input.0.fire || !next_shot.ready(tick) {
        return;
    }
    next_shot.0 = Some(tick + FIRE_COOLDOWN_TICKS as i32);
    // A Lightyear rollback replays earlier ticks; keep `NextShot` in sync, but don't draw a
    // second ball for a tick we already fired on.
    if last_shot.0.is_some_and(|last| tick <= last) {
        return;
    }
    last_shot.0 = Some(tick);
    spawn_paint_ball(
        &mut commands,
        gunner.owner,
        gunner.color,
        muzzle_position(*state),
        state.aim,
        false,
    );
}

/// Remote client receivers for [`ShotFired`] along with the estimated host timeline.
type RemoteShotReceivers<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut MessageReceiver<ShotFired>,
        Option<&'static RemoteTimeline>,
    ),
    (With<Client>, Without<LinkOf>),
>;

/// Joiners only: spawn cosmetic paint balls for other players' shots sent by the host, caught up
/// to where the host's timeline currently has them so lagged shots don't appear late.
fn receive_shots(
    fixed: Res<Time<Fixed>>,
    mut receivers: RemoteShotReceivers,
    mut commands: Commands,
) {
    let dt = fixed.timestep().as_secs_f32();
    for (mut receiver, remote_timeline) in &mut receivers {
        let host_tick = remote_timeline.map(|timeline| timeline.tick());
        for shot in receiver.receive() {
            if !shot.origin.is_finite() || !shot.angle.is_finite() {
                continue;
            }
            let velocity = Vec2::from_angle(shot.angle) * PAINT_BALL_SPEED;
            let position = match host_tick {
                Some(now) => match catch_up_shot(shot.origin, velocity, shot.tick, now, dt) {
                    Some(position) => position,
                    None => continue,
                },
                None => shot.origin,
            };
            spawn_paint_ball(
                &mut commands,
                shot.shooter,
                shot.color,
                position,
                shot.angle,
                false,
            );
        }
    }
}

/// How far along its flight a shot fired on `shot_tick` already is when the host is at
/// `host_tick` (with fixed timestep `dt`). Returns `None` if it already hit a wall or cover
/// while in transit, so it isn't spawned past the obstacle.
pub fn catch_up_shot(
    origin: Vec2,
    velocity: Vec2,
    shot_tick: Tick,
    host_tick: Tick,
    dt: f32,
) -> Option<Vec2> {
    let elapsed = (host_tick - shot_tick).max(0) as f32 * dt;
    if elapsed <= 0.0 {
        return Some(origin);
    }
    let advanced = origin + velocity * elapsed;
    wall_hit(origin, advanced).is_none().then_some(advanced)
}

/// Move every paint ball along its line and check the stretch it covered against the walls,
/// cover and other gunners. Only the host's authoritative balls reduce [`Health`] and knock
/// gunners out; cosmetic balls simply disappear on contact.
fn step_paint_balls(
    time: Res<Time>,
    timeline: Res<LocalTimeline>,
    mut director: Option<ResMut<ShootoutDirector>>,
    mut balls: Query<(Entity, &PaintBall, &mut PaintBallPosition)>,
    mut gunners: Query<(Entity, &Gunner, &GunnerState, &mut Health)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    let tick = timeline.tick();
    for (ball_entity, ball, mut position) in &mut balls {
        let start = position.0;
        let end = start + ball.velocity * dt;
        let hit = check_ball_step(
            start,
            end,
            gunners
                .iter()
                .filter(|(_, gunner, _, health)| {
                    gunner.owner != ball.shooter && health.remaining > 0
                })
                .map(|(entity, _, state, _)| (entity, state.position)),
        );
        match hit {
            Some(BallHit::Wall(_)) => {
                commands.entity(ball_entity).despawn();
                commands.trigger(PaintBallWallHit);
            }
            Some(BallHit::Gunner { target, .. }) => {
                commands.entity(ball_entity).despawn();
                if ball.authoritative
                    && let Ok((_, gunner, _, mut health)) = gunners.get_mut(target)
                {
                    health.remaining = health.remaining.saturating_sub(1);
                    debug!(
                        "{} ({:?}) hit by {:?} ({}/{} hp left)",
                        gunner.name, gunner.owner, ball.shooter, health.remaining, health.max
                    );
                    if health.remaining == 0 {
                        info!(
                            "{} ({:?}) knocked out by {:?} on tick {}",
                            gunner.name, gunner.owner, ball.shooter, tick.0
                        );
                        if let Some(director) = director.as_deref_mut() {
                            director.record_knockout(gunner.clone(), tick);
                        }
                        commands.entity(target).despawn();
                    }
                }
            }
            None => {
                position.0 = end;
            }
        }
    }
}

/// What a paint ball hit first along a tick's stretch, if anything.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BallHit<T> {
    /// Hit the border wall or a cover block at fraction `0.0..=1.0` of the stretch.
    Wall(f32),
    /// Hit `target` at fraction `0.0..=1.0` of the stretch.
    Gunner { t: f32, target: T },
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
        (Some((t_gunner, target)), Some(t_wall)) if t_gunner <= t_wall => Some(BallHit::Gunner {
            t: t_gunner,
            target,
        }),
        (Some((t_gunner, target)), None) => Some(BallHit::Gunner {
            t: t_gunner,
            target,
        }),
        (_, Some(t_wall)) => Some(BallHit::Wall(t_wall)),
        (None, None) => None,
    }
}

/// Earliest `t` in `0.0..=1.0` where a paint ball moving from `start` to `end` touches the border
/// wall or any [`COVER`] block.
pub fn wall_hit(start: Vec2, end: Vec2) -> Option<f32> {
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
    border.into_iter().chain(cover).min_by(f32::total_cmp)
}

/// Earliest `t` in `0.0..=1.0` where the segment `start..=end` leaves `rect`, or `0.0` if `start`
/// is already outside it.
pub fn segment_exits_rect(start: Vec2, end: Vec2, rect: Rect) -> Option<f32> {
    if !rect.contains(start) {
        return Some(0.0);
    }
    let delta = end - start;
    let mut earliest: Option<f32> = None;
    let mut consider = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        earliest = Some(earliest.map_or(t, |best| best.min(t)));
    };
    if delta.x > 0.0 && end.x > rect.max.x {
        consider((rect.max.x - start.x) / delta.x);
    } else if delta.x < 0.0 && end.x < rect.min.x {
        consider((rect.min.x - start.x) / delta.x);
    }
    if delta.y > 0.0 && end.y > rect.max.y {
        consider((rect.max.y - start.y) / delta.y);
    } else if delta.y < 0.0 && end.y < rect.min.y {
        consider((rect.min.y - start.y) / delta.y);
    }
    earliest
}

/// Earliest `t` in `0.0..=1.0` where the segment `start..=end` enters (or starts inside) `rect`.
pub fn segment_hits_rect(start: Vec2, end: Vec2, rect: Rect) -> Option<f32> {
    let delta = end - start;
    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;
    for (origin, dir, min, max) in [
        (start.x, delta.x, rect.min.x, rect.max.x),
        (start.y, delta.y, rect.min.y, rect.max.y),
    ] {
        if dir.abs() < 1e-8 {
            if origin < min || origin > max {
                return None;
            }
        } else {
            let inv = 1.0 / dir;
            let mut t1 = (min - origin) * inv;
            let mut t2 = (max - origin) * inv;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            t_min = t_min.max(t1);
            t_max = t_max.min(t2);
            if t_min > t_max {
                return None;
            }
        }
    }
    Some(t_min)
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
        BallHit, catch_up_shot, check_ball_step, segment_exits_rect, segment_hits_circle,
        segment_hits_rect,
    };
    use crate::minigames::shootout::config::{
        ARENA_HALF_WIDTH, COVER, GUNNER_RADIUS, PAINT_BALL_RADIUS,
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
            Some(BallHit::Wall(t)) => assert!(close(t, 0.25), "{t}"),
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
        assert_eq!(hit, Some(BallHit::Wall(0.0)));
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
        assert_eq!(
            segment_exits_rect(Vec2::new(20.0, 0.0), Vec2::new(25.0, 0.0), rect),
            Some(0.0)
        );
        assert_eq!(
            segment_hits_rect(Vec2::new(-20.0, 15.0), Vec2::new(20.0, 15.0), rect),
            None
        );
        assert!(close(
            segment_hits_rect(Vec2::new(-20.0, 0.0), Vec2::new(20.0, 0.0), rect).unwrap(),
            0.25
        ));
    }

    #[test]
    fn catch_up_advances_a_delayed_shot_and_drops_one_that_hit_a_wall() {
        let dt = 1.0 / 128.0;
        let origin = Vec2::new(-100.0, 140.0);
        let velocity = Vec2::new(600.0, 0.0);

        // No delay or slightly early estimate leaves the ball at its origin.
        assert_eq!(
            catch_up_shot(origin, velocity, Tick(50), Tick(50), dt),
            Some(origin)
        );
        assert_eq!(
            catch_up_shot(origin, velocity, Tick(50), Tick(48), dt),
            Some(origin)
        );

        // 8 ticks behind advances 8 ticks along its line.
        let caught_up = catch_up_shot(origin, velocity, Tick(50), Tick(58), dt).unwrap();
        assert!(
            caught_up.distance(origin + velocity * (8.0 * dt)) < 1e-4,
            "{caught_up}"
        );

        // A shot fired into the centre cover block that already hit it in transit is dropped.
        assert_eq!(
            catch_up_shot(Vec2::new(-40.0, 0.0), velocity, Tick(50), Tick(60), dt),
            None
        );
    }
}
