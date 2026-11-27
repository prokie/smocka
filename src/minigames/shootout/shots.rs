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
    PAINT_BALL_BOUNCES, PAINT_BALL_RADIUS, PAINT_BALL_SPEED, RAPID_FIRE_COOLDOWN_TICKS,
    SPREAD_ANGLE,
};
use super::gunner::{Gunner, GunnerState, Health, NextShot, normalize_angle};
use super::pickups::{BuffKind, GunnerBuff};
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
    pub bounces_left: u8,
    pub authoritative: bool,
}

/// Local only: triggered when a paint ball hits the border wall or a cover block, so
/// [`sounds`](super::sounds) can play a splat and [`ShootoutVisualsPlugin`](super::ShootoutVisualsPlugin)
/// can leave a decal.
#[derive(Event, Debug, Clone, Copy)]
pub struct PaintBallWallHit {
    pub position: Vec2,
    pub color: u8,
}

/// Local only: triggered when a paint ball hits a gunner, so
/// [`ShootoutVisualsPlugin`](super::ShootoutVisualsPlugin) can leave a paint splat on the floor.
#[derive(Event, Debug, Clone, Copy)]
pub struct PaintBallGunnerHit {
    pub position: Vec2,
    pub color: u8,
}

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

/// Fire cooldown in fixed ticks given a gunner's active weapon buff.
pub fn fire_cooldown_ticks(buff: Option<BuffKind>) -> u32 {
    match buff {
        Some(BuffKind::RapidFire) => RAPID_FIRE_COOLDOWN_TICKS,
        _ => FIRE_COOLDOWN_TICKS,
    }
}

/// Angle offsets (in radians from `state.aim`) fired per trigger pull given a gunner's active buff.
pub fn shot_angle_offsets(buff: Option<BuffKind>) -> &'static [f32] {
    match buff {
        Some(BuffKind::Spread) => &[-SPREAD_ANGLE, 0.0, SPREAD_ANGLE],
        _ => &[0.0],
    }
}

fn spawn_paint_ball(
    commands: &mut Commands,
    shooter: PeerId,
    color: u8,
    origin: Vec2,
    velocity: Vec2,
    bounces_left: u8,
    authoritative: bool,
) {
    commands.spawn((
        PaintBall {
            shooter,
            color,
            velocity,
            bounces_left,
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
    Option<&'w GunnerBuff>,
    &'w ActionState<ShootoutInput>,
);

/// The components read when the host fires gunners.
type HostGunner<'w> = (
    &'w Gunner,
    &'w GunnerState,
    &'w mut NextShot,
    Option<&'w GunnerBuff>,
    Option<&'w ActionState<ShootoutInput>>,
);

/// Host only: fire gunners holding Fire whose cooldown is up, spawn authoritative balls, and
/// broadcast [`ShotFired`] to every remote client other than the shooter.
fn fire_gunners_host(
    timeline: Res<LocalTimeline>,
    mut gunners: Query<HostGunner>,
    mut links: RemoteShotLinks,
    mut commands: Commands,
) {
    let tick = timeline.tick();
    for (gunner, state, mut next_shot, buff, input) in &mut gunners {
        let firing = input.is_some_and(|input| input.0.fire);
        if !firing || !next_shot.ready(tick) {
            continue;
        }
        let active_buff = buff.and_then(|buff| buff.active(tick));
        next_shot.0 = Some(tick + fire_cooldown_ticks(active_buff) as i32);
        let origin = muzzle_position(*state);
        for &offset in shot_angle_offsets(active_buff) {
            let angle = normalize_angle(state.aim + offset);
            let velocity = Vec2::from_angle(angle) * PAINT_BALL_SPEED;
            spawn_paint_ball(
                &mut commands,
                gunner.owner,
                gunner.color,
                origin,
                velocity,
                PAINT_BALL_BOUNCES,
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
    let (gunner, state, ref mut next_shot, buff, input) = *gunner;
    if !input.0.fire || !next_shot.ready(tick) {
        return;
    }
    let active_buff = buff.and_then(|buff| buff.active(tick));
    next_shot.0 = Some(tick + fire_cooldown_ticks(active_buff) as i32);
    // A Lightyear rollback replays earlier ticks; keep `NextShot` in sync, but don't draw a
    // second ball for a tick we already fired on.
    if last_shot.0.is_some_and(|last| tick <= last) {
        return;
    }
    last_shot.0 = Some(tick);
    let origin = muzzle_position(*state);
    for &offset in shot_angle_offsets(active_buff) {
        let angle = normalize_angle(state.aim + offset);
        let velocity = Vec2::from_angle(angle) * PAINT_BALL_SPEED;
        spawn_paint_ball(
            &mut commands,
            gunner.owner,
            gunner.color,
            origin,
            velocity,
            PAINT_BALL_BOUNCES,
            false,
        );
    }
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
            let (position, velocity, bounces_left) = match host_tick {
                Some(now) => match catch_up_shot(shot.origin, velocity, shot.tick, now, dt) {
                    Some(state) => state,
                    None => continue,
                },
                None => (shot.origin, velocity, PAINT_BALL_BOUNCES),
            };
            spawn_paint_ball(
                &mut commands,
                shot.shooter,
                shot.color,
                position,
                velocity,
                bounces_left,
                false,
            );
        }
    }
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

/// Reflect `velocity` across unit surface `normal`.
pub fn reflect(velocity: Vec2, normal: Vec2) -> Vec2 {
    velocity - 2.0 * velocity.dot(normal) * normal
}

/// Move every paint ball along its line and check the stretch it covered against the walls,
/// cover and other gunners. A ball ricochets off walls up to [`PAINT_BALL_BOUNCES`] times before
/// splatting. Only the host's authoritative balls reduce [`Health`] and knock gunners out.
fn step_paint_balls(
    time: Res<Time>,
    timeline: Res<LocalTimeline>,
    mut director: Option<ResMut<ShootoutDirector>>,
    mut balls: Query<(Entity, &mut PaintBall, &mut PaintBallPosition)>,
    mut gunners: Query<(Entity, &Gunner, &GunnerState, &mut Health)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    let tick = timeline.tick();
    for (ball_entity, mut ball, mut position) in &mut balls {
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
            Some(BallHit::Wall(impact)) => {
                let hit_pos = start.lerp(end, impact.t);
                commands.trigger(PaintBallWallHit {
                    position: hit_pos,
                    color: ball.color,
                });
                if impact.t > 0.0 && ball.bounces_left > 0 {
                    ball.bounces_left -= 1;
                    ball.velocity = reflect(ball.velocity, impact.normal);
                    let nudged = hit_pos + impact.normal * 0.05;
                    let after_bounce = nudged + ball.velocity * ((1.0 - impact.t) * dt);
                    position.0 = if wall_hit(nudged, after_bounce).is_none() {
                        after_bounce
                    } else {
                        nudged
                    };
                } else {
                    commands.entity(ball_entity).despawn();
                }
            }
            Some(BallHit::Gunner { t, target }) => {
                commands.entity(ball_entity).despawn();
                commands.trigger(PaintBallGunnerHit {
                    position: start.lerp(end, t),
                    color: ball.color,
                });
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
        BallHit, catch_up_shot, check_ball_step, fire_cooldown_ticks, reflect, segment_exits_rect,
        segment_hits_circle, segment_hits_rect, shot_angle_offsets,
    };
    use crate::minigames::shootout::config::{
        ARENA_HALF_WIDTH, COVER, FIRE_COOLDOWN_TICKS, GUNNER_RADIUS, PAINT_BALL_BOUNCES,
        PAINT_BALL_RADIUS, RAPID_FIRE_COOLDOWN_TICKS, SPREAD_ANGLE,
    };
    use crate::minigames::shootout::pickups::BuffKind;

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

    #[test]
    fn weapon_buffs_adjust_fire_cooldown_and_spread_fan() {
        assert_eq!(fire_cooldown_ticks(None), FIRE_COOLDOWN_TICKS);
        assert_eq!(
            fire_cooldown_ticks(Some(BuffKind::RapidFire)),
            RAPID_FIRE_COOLDOWN_TICKS
        );
        assert_eq!(
            fire_cooldown_ticks(Some(BuffKind::Spread)),
            FIRE_COOLDOWN_TICKS
        );

        assert_eq!(shot_angle_offsets(None), &[0.0]);
        assert_eq!(shot_angle_offsets(Some(BuffKind::RapidFire)), &[0.0]);
        assert_eq!(
            shot_angle_offsets(Some(BuffKind::Spread)),
            &[-SPREAD_ANGLE, 0.0, SPREAD_ANGLE]
        );
    }
}
