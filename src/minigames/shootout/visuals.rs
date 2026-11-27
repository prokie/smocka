//! Local-only sprites, HUD and cursor for the shootout: the arena, paint splatters, centre
//! pickups, a sprite, buff halo, name and health pips per gunner, paint balls, the sudden-death
//! danger zone overlay, and a crosshair cursor while this player controls a gunner.

use std::collections::VecDeque;
use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy::window::{CursorIcon, CursorSystems, PrimaryWindow, SystemCursorIcon};
use lightyear::prelude::LocalTimeline;
use lightyear::prelude::input::native::InputMarker;
use rand::RngExt;

use super::PHASE;
use super::ShootoutInput;
use super::config::{
    ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH, GUNNER_RADIUS, PAINT_BALL_RADIUS, PICKUP_RADIUS,
};
use super::gunner::{Gunner, GunnerState, Health};
use super::pickups::{BuffKind, GunnerBuff, Pickup, PickupKind};
use super::shots::{PaintBall, PaintBallGunnerHit, PaintBallPosition, PaintBallWallHit};
use super::sounds::ShootoutSoundsPlugin;
use super::zone::SafeZone;
use crate::camera::gameplay_camera_2d;
use crate::config::display::{ART_SCALE, VIEW_HEIGHT, VIEW_WIDTH};
use crate::minigames::smoothing::{Smoothed, remember_previous_tick, smoothed};
use crate::players::{Player, player_color};
use crate::session::SessionPhase;

/// Covers the whole screen: the floor, the border wall and the cover, drawn to match
/// [`config`](super::config).
const ARENA_IMAGE: &str = "minigames/shootout/arena.png";
/// A gunner seen from above, aiming right, centred on its body. Drawn in white and grey, and
/// tinted with its player's colour.
const GUNNER_IMAGE: &str = "minigames/shootout/gunner.png";
/// A round paint ball drawn white with a dark outline, tinted with its shooter's colour.
const PAINT_BALL_IMAGE: &str = "minigames/shootout/paint_ball.png";
/// A paint splat decal drawn white, tinted with the shooter's colour.
const PAINT_SPLAT_IMAGE: &str = "minigames/shootout/paint_splat.png";
/// Centre pickup sprites for each [`PickupKind`].
const PICKUP_SPREAD_IMAGE: &str = "minigames/shootout/pickup_spread.png";
const PICKUP_RAPID_IMAGE: &str = "minigames/shootout/pickup_rapid.png";
const PICKUP_SHIELD_IMAGE: &str = "minigames/shootout/pickup_shield.png";
/// White halo drawn behind a buffed gunner and tinted per [`BuffKind`].
const BUFF_RING_IMAGE: &str = "minigames/shootout/buff_ring.png";
/// The gunner image's size, in world units: the body, and the barrel sticking out to the right.
/// Twice [`GUNNER_RADIUS`] wide for the body, plus the barrel on each side so it stays centred.
const GUNNER_IMAGE_SIZE: f32 = 2.0 * GUNNER_RADIUS + 12.0;
const BUFF_RING_SIZE: f32 = 2.0 * GUNNER_RADIUS + 10.0;

/// Draw order: higher values are drawn on top.
const ARENA_Z: f32 = 0.0;
const SPLAT_Z: f32 = 0.2;
const PICKUP_Z: f32 = 0.6;
const GUNNER_Z: f32 = 1.0;
const PAINT_BALL_Z: f32 = 2.0;
const ZONE_Z: f32 = 2.5;
const ZONE_BORDER_Z: f32 = 2.55;
const HEALTH_BAR_Z: f32 = 3.0;

/// Paint splatter sizing and cap per round.
const WALL_SPLAT_SIZE: f32 = 14.0;
const HIT_SPLAT_SIZE: f32 = 20.0;
const KNOCKOUT_BURST_COUNT: usize = 6;
const SPLAT_ALPHA: f32 = 0.85;
const MAX_SPLATS: usize = 160;
/// How long a gunner flashes white after taking a hit, in seconds.
const HIT_FLASH_SECONDS: f32 = 0.12;

/// Pickup animation and buff halo colours.
const PICKUP_BOB_HEIGHT: f32 = 1.5;
const PICKUP_BOB_SPEED: f32 = 3.5;
const SPREAD_BUFF_COLOR: Color = Color::srgba(0.98, 0.68, 0.2, 0.9);
const RAPID_BUFF_COLOR: Color = Color::srgba(0.25, 0.85, 1.0, 0.9);

/// Sudden-death danger overlay colours and banner duration.
const ZONE_DANGER_COLOR: Color = Color::srgba(0.85, 0.12, 0.16, 0.34);
const ZONE_BORDER_COLOR: Color = Color::srgba(1.0, 0.3, 0.25, 0.92);
const ZONE_BORDER_THICKNESS: f32 = 2.0;
const SUDDEN_DEATH_BANNER_SECONDS: f32 = 2.5;
const SUDDEN_DEATH_TEXT_SIZE: f32 = 34.0;
const SUDDEN_DEATH_TOP_MARGIN: f32 = 20.0;

/// Health pips and player name above each gunner, in world units.
const PIP_SIZE: f32 = 3.0;
const PIP_GAP: f32 = 1.5;
const HEALTH_BAR_OFFSET_Y: f32 = GUNNER_RADIUS + 5.5;
const NAME_FONT_SIZE: f32 = 6.5;
const NAME_OFFSET_Y: f32 = (PIP_SIZE + 2.0) / 2.0 + NAME_FONT_SIZE / 2.0 + 1.5;
/// Above this y, the name would reach past the top of the screen, so flip the bar and name below.
const LABEL_FLIP_Y: f32 =
    VIEW_HEIGHT / 2.0 - (HEALTH_BAR_OFFSET_Y + NAME_OFFSET_Y + NAME_FONT_SIZE / 2.0);
const HEALTH_BACK_COLOR: Color = Color::srgba(0.1, 0.1, 0.14, 0.85);
const HEALTH_FULL_COLOR: Color = Color::WHITE;
const HEALTH_LOST_COLOR: Color = Color::srgb(0.28, 0.28, 0.34);

/// Follows a gunner's position without rotating, showing its remaining hit points and name.
#[derive(Component)]
struct HealthBar {
    gunner: Entity,
    pips_built: u32,
}

/// One hit-point pip inside a [`HealthBar`], numbered from 0.
#[derive(Component)]
struct HealthPip(u32);

/// The player's name label inside a [`HealthBar`].
#[derive(Component)]
struct GunnerName;

/// Pulsing halo drawn behind a gunner while it holds an active weapon buff.
#[derive(Component)]
struct BuffHalo;

/// One of four translucent crimson strips covering the floor outside [`SafeZone`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum ZoneStrip {
    Top,
    Bottom,
    Left,
    Right,
}

/// One of four bright edge lines framing [`SafeZone`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum ZoneBorder {
    Top,
    Bottom,
    Left,
    Right,
}

/// Brief top-centre `"SUDDEN DEATH!"` banner shown once the safe zone begins closing.
#[derive(Component)]
struct SuddenDeathBanner {
    announced: bool,
    timer: Timer,
}

/// Tracks a gunner's last seen health and white hit-flash timer.
#[derive(Component)]
struct HitFlash {
    last_health: u32,
    timer: Timer,
}

/// Live paint splat entities in oldest-first order, capped at [`MAX_SPLATS`].
#[derive(Resource, Default)]
struct PaintSplats {
    image: Handle<Image>,
    live: VecDeque<Entity>,
}

/// Local-only visuals for the shootout.
pub struct ShootoutVisualsPlugin;

impl Plugin for ShootoutVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ShootoutSoundsPlugin);
        app.init_resource::<PaintSplats>();
        app.add_systems(
            OnEnter(PHASE),
            (
                spawn_camera,
                spawn_arena,
                setup_splats,
                spawn_zone_overlay,
                spawn_sudden_death_banner,
            ),
        );
        app.add_systems(OnExit(PHASE), (reset_cursor, clear_splats));
        app.add_observer(add_gunner_sprite);
        app.add_observer(spawn_health_bar);
        app.add_observer(add_paint_ball_sprite);
        app.add_observer(add_pickup_sprite);
        app.add_observer(splat_on_wall_hit);
        app.add_observer(splat_on_gunner_hit);
        app.add_observer(splat_on_knockout);
        app.add_systems(
            FixedFirst,
            (
                remember_previous_tick::<GunnerState>,
                remember_previous_tick::<PaintBallPosition>,
            )
                .run_if(in_state(PHASE)),
        );
        app.add_systems(
            Update,
            (
                update_hit_flash,
                animate_pickups,
                sync_buff_halos,
                update_sudden_death_banner,
            )
                .run_if(in_state(PHASE)),
        );
        app.add_systems(
            PostUpdate,
            (
                sync_gunner_transform.before(TransformSystems::Propagate),
                sync_health_bars
                    .after(sync_gunner_transform)
                    .before(TransformSystems::Propagate),
                sync_paint_ball_transform.before(TransformSystems::Propagate),
                sync_safe_zone_visuals.before(TransformSystems::Propagate),
                update_crosshair_cursor.before(CursorSystems::Update),
            )
                .run_if(in_state(PHASE)),
        );
    }
}

fn setup_splats(assets: Res<AssetServer>, mut splats: ResMut<PaintSplats>) {
    splats.image = assets.load(PAINT_SPLAT_IMAGE);
    splats.live.clear();
}

fn clear_splats(mut splats: ResMut<PaintSplats>) {
    splats.live.clear();
}

fn spawn_splat(
    commands: &mut Commands,
    splats: &mut PaintSplats,
    position: Vec2,
    color: u8,
    base_size: f32,
) {
    if splats.live.len() >= MAX_SPLATS
        && let Some(oldest) = splats.live.pop_front()
    {
        commands.entity(oldest).despawn();
    }
    let mut rng = rand::rng();
    let angle = rng.random_range(0.0..TAU);
    let size = base_size * rng.random_range(0.8..=1.2);
    // Tiny z-offset from live count prevents z-fighting between overlapping splats.
    let z = SPLAT_Z + (splats.live.len() as f32) * 1e-4;
    let entity = commands
        .spawn((
            DespawnOnExit(PHASE),
            Sprite {
                color: player_color(color).with_alpha(SPLAT_ALPHA),
                custom_size: Some(Vec2::splat(size)),
                ..Sprite::from_image(splats.image.clone())
            },
            Transform::from_xyz(position.x, position.y, z)
                .with_rotation(Quat::from_rotation_z(angle)),
        ))
        .id();
    splats.live.push_back(entity);
}

fn splat_on_wall_hit(
    trigger: On<PaintBallWallHit>,
    mut splats: ResMut<PaintSplats>,
    mut commands: Commands,
) {
    let hit = trigger.event();
    spawn_splat(
        &mut commands,
        &mut splats,
        hit.position,
        hit.color,
        WALL_SPLAT_SIZE,
    );
}

fn splat_on_gunner_hit(
    trigger: On<PaintBallGunnerHit>,
    mut splats: ResMut<PaintSplats>,
    mut commands: Commands,
) {
    let hit = trigger.event();
    spawn_splat(
        &mut commands,
        &mut splats,
        hit.position,
        hit.color,
        HIT_SPLAT_SIZE,
    );
}

fn splat_on_knockout(
    trigger: On<Remove<Gunner>>,
    phase: Option<Res<State<SessionPhase>>>,
    gunners: Query<(&Gunner, &GunnerState, Option<&Health>)>,
    players: Query<&Player>,
    mut splats: ResMut<PaintSplats>,
    mut commands: Commands,
) {
    if !phase.is_some_and(|phase| *phase.get() == PHASE) {
        return;
    }
    let Ok((gunner, state, health)) = gunners.get(trigger.entity) else {
        return;
    };
    let still_here = players.iter().any(|player| player.id == gunner.owner);
    if !still_here || health.is_some_and(|health| health.remaining > 1) {
        return;
    }
    let mut rng = rand::rng();
    for index in 0..KNOCKOUT_BURST_COUNT {
        let angle =
            index as f32 * (TAU / KNOCKOUT_BURST_COUNT as f32) + rng.random_range(-0.3..0.3);
        let radius = rng.random_range(2.0..12.0);
        let offset = Vec2::from_angle(angle) * radius;
        spawn_splat(
            &mut commands,
            &mut splats,
            state.position + offset,
            gunner.color,
            HIT_SPLAT_SIZE * 0.9,
        );
    }
}

/// Briefly flash a gunner white whenever its [`Health`] drops, then restore its player colour.
fn update_hit_flash(
    time: Res<Time>,
    mut gunners: Query<(Entity, &Gunner, &Health, &mut Sprite, Option<&mut HitFlash>)>,
    mut commands: Commands,
) {
    for (entity, gunner, health, mut sprite, flash) in &mut gunners {
        let Some(mut flash) = flash else {
            let mut timer = Timer::from_seconds(HIT_FLASH_SECONDS, TimerMode::Once);
            timer.tick(timer.duration());
            commands.entity(entity).insert(HitFlash {
                last_health: health.remaining,
                timer,
            });
            continue;
        };
        if health.remaining < flash.last_health {
            flash.timer.reset();
        }
        flash.last_health = health.remaining;
        flash.timer.tick(time.delta());
        let expected = if flash.timer.is_finished() {
            player_color(gunner.color)
        } else {
            Color::WHITE
        };
        if sprite.color != expected {
            sprite.color = expected;
        }
    }
}

/// The whole arena fits on screen.
fn spawn_camera(mut commands: Commands) {
    debug!("Spawning the shootout camera");
    commands.spawn(gameplay_camera_2d(PHASE, VIEW_WIDTH, VIEW_HEIGHT));
}

fn spawn_arena(assets: Res<AssetServer>, mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(PHASE),
        Sprite {
            custom_size: Some(Vec2::new(VIEW_WIDTH, VIEW_HEIGHT)),
            ..Sprite::from_image(assets.load(ARENA_IMAGE))
        },
        Transform::from_xyz(0.0, 0.0, ARENA_Z),
    ));
}

fn add_gunner_sprite(
    trigger: On<Add<Gunner>>,
    gunners: Query<(&Gunner, Option<&GunnerState>)>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Ok((gunner, state)) = gunners.get(trigger.entity) else {
        return;
    };
    let (position, aim) = state.map_or((Vec2::ZERO, 0.0), |state| (state.position, state.aim));
    commands
        .entity(trigger.entity)
        .insert((
            Sprite {
                color: player_color(gunner.color),
                custom_size: Some(Vec2::splat(GUNNER_IMAGE_SIZE)),
                ..Sprite::from_image(assets.load(GUNNER_IMAGE))
            },
            Transform::from_xyz(position.x, position.y, GUNNER_Z)
                .with_rotation(Quat::from_rotation_z(aim)),
        ))
        .with_children(|parent| {
            parent.spawn((
                BuffHalo,
                Visibility::Hidden,
                Sprite {
                    color: SPREAD_BUFF_COLOR,
                    custom_size: Some(Vec2::splat(BUFF_RING_SIZE)),
                    ..Sprite::from_image(assets.load(BUFF_RING_IMAGE))
                },
                Transform::from_xyz(0.0, 0.0, -0.1),
            ));
        });
}

fn add_pickup_sprite(
    trigger: On<Add<Pickup>>,
    pickups: Query<&Pickup>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Ok(pickup) = pickups.get(trigger.entity) else {
        return;
    };
    let image = match pickup.kind {
        PickupKind::Spread => PICKUP_SPREAD_IMAGE,
        PickupKind::RapidFire => PICKUP_RAPID_IMAGE,
        PickupKind::Shield => PICKUP_SHIELD_IMAGE,
    };
    commands.entity(trigger.entity).insert((
        Sprite {
            custom_size: Some(Vec2::splat(2.0 * PICKUP_RADIUS)),
            ..Sprite::from_image(assets.load(image))
        },
        Transform::from_xyz(pickup.position.x, pickup.position.y, PICKUP_Z),
    ));
}

/// Gently bob and pulse centre pickups so they catch the eye.
fn animate_pickups(time: Res<Time>, mut pickups: Query<(&Pickup, &mut Transform)>) {
    let elapsed = time.elapsed_secs();
    for (pickup, mut transform) in &mut pickups {
        let phase = (elapsed * PICKUP_BOB_SPEED).sin();
        transform.translation.x = pickup.position.x;
        transform.translation.y = pickup.position.y + phase * PICKUP_BOB_HEIGHT;
        transform.scale = Vec3::splat(1.0 + 0.07 * phase);
    }
}

type BuffHaloQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Sprite,
        &'static mut Visibility,
        &'static mut Transform,
    ),
    With<BuffHalo>,
>;

/// Show and pulse each gunner's [`BuffHalo`] while it holds an active weapon buff.
fn sync_buff_halos(
    time: Res<Time>,
    timeline: Option<Res<LocalTimeline>>,
    gunners: Query<(&GunnerBuff, &Children), With<Gunner>>,
    mut halos: BuffHaloQuery,
) {
    let Some(timeline) = timeline else {
        return;
    };
    let tick = timeline.tick();
    let pulse = 1.0 + 0.08 * (time.elapsed_secs() * 7.0).sin();
    for (buff, children) in &gunners {
        let active = buff.active(tick);
        for child in children.iter() {
            let Ok((mut sprite, mut visibility, mut transform)) = halos.get_mut(child) else {
                continue;
            };
            match active {
                Some(BuffKind::Spread) => {
                    *visibility = Visibility::Inherited;
                    sprite.color = SPREAD_BUFF_COLOR;
                    transform.scale = Vec3::splat(pulse);
                }
                Some(BuffKind::RapidFire) => {
                    *visibility = Visibility::Inherited;
                    sprite.color = RAPID_BUFF_COLOR;
                    transform.scale = Vec3::splat(pulse);
                }
                None => {
                    *visibility = Visibility::Hidden;
                }
            }
        }
    }
}

fn spawn_zone_overlay(mut commands: Commands) {
    for strip in [
        ZoneStrip::Top,
        ZoneStrip::Bottom,
        ZoneStrip::Left,
        ZoneStrip::Right,
    ] {
        commands.spawn((
            strip,
            DespawnOnExit(PHASE),
            Visibility::Hidden,
            Sprite::from_color(ZONE_DANGER_COLOR, Vec2::ZERO),
            Transform::from_xyz(0.0, 0.0, ZONE_Z),
        ));
    }
    for border in [
        ZoneBorder::Top,
        ZoneBorder::Bottom,
        ZoneBorder::Left,
        ZoneBorder::Right,
    ] {
        commands.spawn((
            border,
            DespawnOnExit(PHASE),
            Visibility::Hidden,
            Sprite::from_color(ZONE_BORDER_COLOR, Vec2::ZERO),
            Transform::from_xyz(0.0, 0.0, ZONE_BORDER_Z),
        ));
    }
}

fn spawn_sudden_death_banner(mut commands: Commands) {
    commands.spawn((
        SuddenDeathBanner {
            announced: false,
            timer: Timer::from_seconds(SUDDEN_DEATH_BANNER_SECONDS, TimerMode::Once),
        },
        DespawnOnExit(PHASE),
        Visibility::Hidden,
        Text::new("SUDDEN DEATH!"),
        TextFont {
            font_size: FontSize::Px(SUDDEN_DEATH_TEXT_SIZE),
            ..default()
        },
        TextColor(ZONE_BORDER_COLOR),
        Node {
            position_type: PositionType::Absolute,
            top: px(SUDDEN_DEATH_TOP_MARGIN),
            width: percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        TextLayout::justify(Justify::Center),
    ));
}

fn update_sudden_death_banner(
    time: Res<Time>,
    zones: Query<&SafeZone>,
    mut banners: Query<(&mut SuddenDeathBanner, &mut Visibility)>,
) {
    let active = zones.iter().any(|zone| zone.active);
    for (mut banner, mut visibility) in &mut banners {
        if !banner.announced && active {
            banner.announced = true;
            banner.timer.reset();
            *visibility = Visibility::Inherited;
        }
        if banner.announced {
            banner.timer.tick(time.delta());
            if banner.timer.is_finished() {
                *visibility = Visibility::Hidden;
            }
        }
    }
}

/// Centre and size of one non-overlapping crimson danger strip between the outer arena floor and
/// the current safe zone (`half_w` × `half_h`).
fn zone_strip_rect(strip: ZoneStrip, half_w: f32, half_h: f32) -> (Vec2, Vec2) {
    let top_h = (ARENA_HALF_HEIGHT - half_h).max(0.0);
    let side_w = (ARENA_HALF_WIDTH - half_w).max(0.0);
    match strip {
        ZoneStrip::Top => (
            Vec2::new(0.0, half_h + top_h / 2.0),
            Vec2::new(2.0 * ARENA_HALF_WIDTH, top_h),
        ),
        ZoneStrip::Bottom => (
            Vec2::new(0.0, -(half_h + top_h / 2.0)),
            Vec2::new(2.0 * ARENA_HALF_WIDTH, top_h),
        ),
        ZoneStrip::Left => (
            Vec2::new(-(half_w + side_w / 2.0), 0.0),
            Vec2::new(side_w, 2.0 * half_h),
        ),
        ZoneStrip::Right => (
            Vec2::new(half_w + side_w / 2.0, 0.0),
            Vec2::new(side_w, 2.0 * half_h),
        ),
    }
}

fn zone_border_rect(border: ZoneBorder, half_w: f32, half_h: f32) -> (Vec2, Vec2) {
    let t = ZONE_BORDER_THICKNESS;
    match border {
        ZoneBorder::Top => (Vec2::new(0.0, half_h), Vec2::new(2.0 * half_w + t, t)),
        ZoneBorder::Bottom => (Vec2::new(0.0, -half_h), Vec2::new(2.0 * half_w + t, t)),
        ZoneBorder::Left => (Vec2::new(-half_w, 0.0), Vec2::new(t, 2.0 * half_h)),
        ZoneBorder::Right => (Vec2::new(half_w, 0.0), Vec2::new(t, 2.0 * half_h)),
    }
}

type ZoneStripQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static ZoneStrip,
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    Without<ZoneBorder>,
>;

type ZoneBorderQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static ZoneBorder,
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    Without<ZoneStrip>,
>;

fn sync_safe_zone_visuals(
    zones: Query<&SafeZone>,
    mut strips: ZoneStripQuery,
    mut borders: ZoneBorderQuery,
) {
    let active_zone = zones.iter().find(|zone| zone.active).copied();
    for (strip, mut sprite, mut transform, mut visibility) in &mut strips {
        let Some(zone) = active_zone else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let (centre, size) = zone_strip_rect(*strip, zone.half_width, zone.half_height);
        *visibility = Visibility::Inherited;
        sprite.custom_size = Some(size);
        transform.translation.x = centre.x;
        transform.translation.y = centre.y;
    }
    for (border, mut sprite, mut transform, mut visibility) in &mut borders {
        let Some(zone) = active_zone else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let (centre, size) = zone_border_rect(*border, zone.half_width, zone.half_height);
        *visibility = Visibility::Inherited;
        sprite.custom_size = Some(size);
        transform.translation.x = centre.x;
        transform.translation.y = centre.y;
    }
}

fn spawn_health_bar(
    trigger: On<Add<Health>>,
    gunners: Query<(&Gunner, &Health, Option<&GunnerState>)>,
    mut commands: Commands,
) {
    let Ok((gunner, health, state)) = gunners.get(trigger.entity) else {
        return;
    };
    let position = state.map_or(Vec2::ZERO, |state| state.position);
    build_health_bar(&mut commands, trigger.entity, gunner, *health, position);
}

fn build_health_bar(
    commands: &mut Commands,
    gunner_entity: Entity,
    gunner: &Gunner,
    health: Health,
    position: Vec2,
) {
    if health.max == 0 {
        return;
    }
    let side = label_side(position.y);
    let total_width = health.max as f32 * PIP_SIZE + health.max.saturating_sub(1) as f32 * PIP_GAP;
    let first_x = -total_width / 2.0 + PIP_SIZE / 2.0;
    commands
        .spawn((
            HealthBar {
                gunner: gunner_entity,
                pips_built: health.max,
            },
            DespawnOnExit(PHASE),
            Sprite::from_color(
                HEALTH_BACK_COLOR,
                Vec2::new(total_width + 2.0, PIP_SIZE + 2.0),
            ),
            Transform::from_xyz(
                position.x,
                position.y + side * HEALTH_BAR_OFFSET_Y,
                HEALTH_BAR_Z,
            ),
        ))
        .with_children(|bar| {
            for index in 0..health.max {
                let x = first_x + index as f32 * (PIP_SIZE + PIP_GAP);
                let color = if index < health.remaining {
                    HEALTH_FULL_COLOR
                } else {
                    HEALTH_LOST_COLOR
                };
                bar.spawn((
                    HealthPip(index),
                    Sprite::from_color(color, Vec2::splat(PIP_SIZE)),
                    Transform::from_xyz(x, 0.0, 0.1),
                ));
            }
            bar.spawn((
                GunnerName,
                Text2d::new(gunner.name.clone()),
                // Draw at ART_SCALE and shrink back down so text stays crisp when the camera zooms.
                TextFont {
                    font_size: FontSize::Px(NAME_FONT_SIZE * ART_SCALE),
                    ..default()
                },
                TextColor(Color::WHITE),
                Transform::from_xyz(0.0, side * NAME_OFFSET_Y, 0.1)
                    .with_scale(Vec3::splat(1.0 / ART_SCALE)),
            ));
        });
}

/// `+1.0` to draw the health bar and name above a gunner at `y`, or `-1.0` to flip them below so
/// they don't clip off the top of the arena.
fn label_side(gunner_y: f32) -> f32 {
    if gunner_y > LABEL_FLIP_Y { -1.0 } else { 1.0 }
}

fn add_paint_ball_sprite(
    trigger: On<Add<PaintBall>>,
    balls: Query<(&PaintBall, &PaintBallPosition)>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Ok((ball, position)) = balls.get(trigger.entity) else {
        return;
    };
    commands.entity(trigger.entity).insert((
        Sprite {
            color: player_color(ball.color),
            custom_size: Some(Vec2::splat(2.0 * PAINT_BALL_RADIUS)),
            ..Sprite::from_image(assets.load(PAINT_BALL_IMAGE))
        },
        Transform::from_xyz(position.0.x, position.0.y, PAINT_BALL_Z),
    ));
}

/// Draw each gunner between its last two ticks (see [`smoothing`](crate::minigames::smoothing)).
fn sync_gunner_transform(
    time: Res<Time<Fixed>>,
    mut gunners: Query<Smoothed<GunnerState>, With<Gunner>>,
) {
    for (state, previous, mut transform) in &mut gunners {
        let shown = smoothed(state, previous, time.overstep_fraction());
        transform.translation.x = shown.position.x;
        transform.translation.y = shown.position.y;
        transform.rotation = Quat::from_rotation_z(shown.aim);
    }
}

type HealthBarQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static HealthBar,
        &'static mut Transform,
        &'static Children,
    ),
    (Without<Gunner>, Without<GunnerName>),
>;

/// Keep each gunner's health bar and name above it (or below near the top edge) and lit for its
/// remaining hits; rebuild the bar if a Shield pickup grows `health.max`, and despawn bars whose
/// gunner was knocked out or left.
fn sync_health_bars(
    gunners: Query<(&Gunner, &Transform, &Health)>,
    mut bars: HealthBarQuery,
    mut pips: Query<(&HealthPip, &mut Sprite)>,
    mut names: Query<&mut Transform, (With<GunnerName>, Without<Gunner>)>,
    mut commands: Commands,
) {
    for (bar_entity, bar, mut bar_transform, children) in &mut bars {
        let Ok((gunner, gunner_transform, health)) = gunners.get(bar.gunner) else {
            commands.entity(bar_entity).despawn();
            continue;
        };
        if bar.pips_built != health.max {
            commands.entity(bar_entity).despawn();
            build_health_bar(
                &mut commands,
                bar.gunner,
                gunner,
                *health,
                gunner_transform.translation.truncate(),
            );
            continue;
        }
        let side = label_side(gunner_transform.translation.y);
        bar_transform.translation.x = gunner_transform.translation.x;
        bar_transform.translation.y = gunner_transform.translation.y + side * HEALTH_BAR_OFFSET_Y;
        for child in children.iter() {
            if let Ok((pip, mut sprite)) = pips.get_mut(child) {
                sprite.color = if pip.0 < health.remaining {
                    HEALTH_FULL_COLOR
                } else {
                    HEALTH_LOST_COLOR
                };
            } else if let Ok(mut name_transform) = names.get_mut(child) {
                name_transform.translation.y = side * NAME_OFFSET_Y;
            }
        }
    }
}

/// Draw each paint ball between its last two ticks (see
/// [`smoothing`](crate::minigames::smoothing)).
fn sync_paint_ball_transform(
    time: Res<Time<Fixed>>,
    mut balls: Query<Smoothed<PaintBallPosition>, With<PaintBall>>,
) {
    for (position, previous, mut transform) in &mut balls {
        let shown = smoothed(position, previous, time.overstep_fraction());
        transform.translation.x = shown.0.x;
        transform.translation.y = shown.0.y;
    }
}

/// Show a crosshair cursor while this player controls a gunner, and the normal cursor while
/// watching.
fn update_crosshair_cursor(
    own_gunner: Query<(), With<InputMarker<ShootoutInput>>>,
    window: Single<(Entity, Option<&CursorIcon>), With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let (window, cursor) = *window;
    let crosshair = CursorIcon::System(SystemCursorIcon::Crosshair);
    if !own_gunner.is_empty() {
        if cursor != Some(&crosshair) {
            commands.entity(window).insert(crosshair);
        }
    } else if cursor.is_some() {
        commands.entity(window).remove::<CursorIcon>();
    }
}

/// Put the normal cursor back when leaving the shootout.
fn reset_cursor(
    window: Single<(Entity, Option<&CursorIcon>), With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let (window, cursor) = *window;
    if cursor.is_some() {
        commands.entity(window).remove::<CursorIcon>();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};
    use lightyear::prelude::PeerId;
    use lightyear::prelude::input::native::InputMarker;

    use super::{
        GunnerName, HIT_FLASH_SECONDS, HealthPip, LABEL_FLIP_Y, MAX_SPLATS, PaintSplats, ZoneStrip,
        label_side, reset_cursor, spawn_health_bar, spawn_zone_overlay, splat_on_wall_hit,
        sync_health_bars, sync_safe_zone_visuals, update_crosshair_cursor, update_hit_flash,
        zone_strip_rect,
    };
    use crate::minigames::shootout::ShootoutInput;
    use crate::minigames::shootout::config::{ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH};
    use crate::minigames::shootout::gunner::{Gunner, GunnerState, Health};
    use crate::minigames::shootout::shots::PaintBallWallHit;
    use crate::minigames::shootout::zone::SafeZone;
    use crate::players::player_color;

    #[test]
    fn gunner_name_spawns_with_health_bar_and_flips_below_near_the_top_wall() {
        assert!(label_side(0.0) > 0.0);
        assert!(label_side(140.0) > 0.0);
        assert!(label_side(LABEL_FLIP_Y + 5.0) < 0.0);

        let mut world = World::new();
        world.add_observer(spawn_health_bar);
        let gunner = world
            .spawn((
                Gunner {
                    owner: PeerId::Netcode(1),
                    name: "Ada".into(),
                    color: 0,
                },
                GunnerState::at(Vec2::new(0.0, 50.0)),
                Health::full(3),
                Transform::from_xyz(0.0, 50.0, 1.0),
            ))
            .id();
        let label = world
            .query_filtered::<&Text2d, With<GunnerName>>()
            .single(&world)
            .unwrap();
        assert_eq!(label.0, "Ada");

        // Running `sync_health_bars` verifies the queries are disjoint and follow the gunner.
        world.run_system_once(sync_health_bars).unwrap();
        let above_y = world
            .query_filtered::<&Transform, With<GunnerName>>()
            .single(&world)
            .unwrap()
            .translation
            .y;
        assert!(above_y > 0.0, "{above_y}");

        world.get_mut::<Transform>(gunner).unwrap().translation.y = LABEL_FLIP_Y + 5.0;
        world.run_system_once(sync_health_bars).unwrap();
        let below_y = world
            .query_filtered::<&Transform, With<GunnerName>>()
            .single(&world)
            .unwrap()
            .translation
            .y;
        assert!(below_y < 0.0, "{below_y}");

        // Growing health.max (from a Shield pickup) rebuilds the bar with an extra pip.
        *world.get_mut::<Health>(gunner).unwrap() = Health {
            remaining: 4,
            max: 4,
        };
        world.run_system_once(sync_health_bars).unwrap();
        let pip_count = world.query::<&HealthPip>().iter(&world).count();
        assert_eq!(pip_count, 4);
    }

    #[test]
    fn paint_splats_cap_at_max_and_hit_flash_resets_after_duration() {
        let mut world = World::new();
        world.init_resource::<PaintSplats>();
        world.add_observer(splat_on_wall_hit);
        for index in 0..(MAX_SPLATS + 5) {
            world.trigger(PaintBallWallHit {
                position: Vec2::new(index as f32, 0.0),
                color: 0,
            });
        }
        assert_eq!(world.resource::<PaintSplats>().live.len(), MAX_SPLATS);

        // Hit flash turns the gunner sprite white when health drops, then restores its colour.
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_millis(20));
        world.insert_resource(time);
        let gunner = world
            .spawn((
                Gunner {
                    owner: PeerId::Netcode(1),
                    name: "Ada".into(),
                    color: 1,
                },
                Health::full(3),
                Sprite::from_color(player_color(1), Vec2::ONE),
            ))
            .id();
        world.run_system_once(update_hit_flash).unwrap();
        assert_eq!(world.get::<Sprite>(gunner).unwrap().color, player_color(1));

        world.get_mut::<Health>(gunner).unwrap().remaining = 2;
        world.run_system_once(update_hit_flash).unwrap();
        assert_eq!(world.get::<Sprite>(gunner).unwrap().color, Color::WHITE);

        world
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs_f32(HIT_FLASH_SECONDS + 0.05));
        world.run_system_once(update_hit_flash).unwrap();
        assert_eq!(world.get::<Sprite>(gunner).unwrap().color, player_color(1));
    }

    #[test]
    fn safe_zone_overlay_stays_hidden_until_active_and_covers_danger_without_overlap() {
        let mut world = World::new();
        world.run_system_once(spawn_zone_overlay).unwrap();
        let zone = world.spawn(SafeZone::full()).id();

        world.run_system_once(sync_safe_zone_visuals).unwrap();
        assert!(
            world
                .query_filtered::<&Visibility, With<ZoneStrip>>()
                .iter(&world)
                .all(|v| *v == Visibility::Hidden)
        );

        *world.get_mut::<SafeZone>(zone).unwrap() = SafeZone::at_progress(0.5);
        world.run_system_once(sync_safe_zone_visuals).unwrap();
        assert!(
            world
                .query_filtered::<&Visibility, With<ZoneStrip>>()
                .iter(&world)
                .all(|v| *v == Visibility::Inherited)
        );

        // Strip area + safe area must equal total arena area (no corner overlap).
        let active = *world.get::<SafeZone>(zone).unwrap();
        let strips_area: f32 = [
            ZoneStrip::Top,
            ZoneStrip::Bottom,
            ZoneStrip::Left,
            ZoneStrip::Right,
        ]
        .into_iter()
        .map(|strip| {
            let (_, size) = zone_strip_rect(strip, active.half_width, active.half_height);
            size.x * size.y
        })
        .sum();
        let safe_area = (2.0 * active.half_width) * (2.0 * active.half_height);
        let arena_area = (2.0 * ARENA_HALF_WIDTH) * (2.0 * ARENA_HALF_HEIGHT);
        assert!((strips_area + safe_area - arena_area).abs() < 1e-2);
    }

    #[test]
    fn crosshair_is_shown_only_while_controlling_a_gunner_and_reset_on_exit() {
        let mut world = World::new();
        let window = world.spawn((Window::default(), PrimaryWindow)).id();

        // A spectator (no controlled gunner) keeps the default cursor.
        world.run_system_once(update_crosshair_cursor).unwrap();
        assert_eq!(world.get::<CursorIcon>(window), None);

        // Once this player has a controlled gunner, the cursor becomes a crosshair.
        let gunner = world.spawn(InputMarker::<ShootoutInput>::default()).id();
        world.run_system_once(update_crosshair_cursor).unwrap();
        assert_eq!(
            world.get::<CursorIcon>(window),
            Some(&CursorIcon::System(SystemCursorIcon::Crosshair))
        );

        // Leaving the shootout restores the normal cursor.
        world.run_system_once(reset_cursor).unwrap();
        assert_eq!(world.get::<CursorIcon>(window), None);

        // If the player stops controlling a gunner mid-round, the cursor also resets.
        world.run_system_once(update_crosshair_cursor).unwrap();
        world.despawn(gunner);
        world.run_system_once(update_crosshair_cursor).unwrap();
        assert_eq!(world.get::<CursorIcon>(window), None);
    }
}
