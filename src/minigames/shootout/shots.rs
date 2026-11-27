//! Paint balls: firing, broadcasting shots as where, which way and on which tick rather than as
//! replicated entities, and stepping balls with swept hit detection (see [`check_ball_step`]).
//!
//! Each peer simulates paint balls along their straight lines locally. The shooting peer spawns
//! its own cosmetic ball immediately on the tick it fires (preventing duplicates across rollback),
//! while the host spawns an authoritative ball, broadcasts [`ShotFired`] to everyone else, and
//! decides hits, hit points and knockouts.

mod ballistics;

pub use ballistics::{
    BallHit, WallImpact, catch_up_shot, check_ball_step, reflect, segment_exits_rect,
    segment_hits_circle, segment_hits_rect, wall_hit,
};

use bevy::prelude::*;
use lightyear::prelude::client::RemoteTimeline;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::{server::ClientOf, *};
use serde::{Deserialize, Serialize};

use super::config::{
    FIRE_COOLDOWN_TICKS, MUZZLE_OFFSET, PAINT_BALL_BOUNCES, PAINT_BALL_SPEED,
    RAPID_FIRE_COOLDOWN_TICKS, SPREAD_ANGLE,
};
use super::gunner::{Gunner, GunnerState, Health, NextShot, normalize_angle};
use super::pickups::{BuffKind, GunnerBuff};
use super::round::{ShootoutDirector, apply_damage};
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
/// [`sounds`](super::sounds) can play a splat and
/// [`ShootoutVisualsPlugin`](super::ShootoutVisualsPlugin) can leave a decal.
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
                    let source = format!("{:?}", ball.shooter);
                    apply_damage(
                        director.as_deref_mut(),
                        &mut commands,
                        target,
                        gunner,
                        &mut health,
                        tick,
                        &source,
                    );
                }
            }
            None => {
                position.0 = end;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fire_cooldown_ticks, shot_angle_offsets};
    use crate::minigames::shootout::config::{
        FIRE_COOLDOWN_TICKS, RAPID_FIRE_COOLDOWN_TICKS, SPREAD_ANGLE,
    };
    use crate::minigames::shootout::pickups::BuffKind;

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
