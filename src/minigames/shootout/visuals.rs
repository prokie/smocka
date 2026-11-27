//! Local-only visuals for the shootout: camera, arena, gunner and paint-ball sprites, white hit
//! flash, crosshair cursor, and the focused sub-plugins for paint splatters, health bars, centre
//! pickups and the sudden-death border.

mod health_bars;
mod pickups;
mod splats;
mod zone;

use bevy::prelude::*;
use bevy::window::{CursorIcon, CursorSystems, PrimaryWindow, SystemCursorIcon};
use lightyear::prelude::input::native::InputMarker;

use super::PHASE;
use super::ShootoutInput;
use super::config::{GUNNER_RADIUS, PAINT_BALL_RADIUS};
use super::gunner::{Gunner, GunnerState, Health};
use super::shots::{PaintBall, PaintBallPosition};
use super::sounds::ShootoutSoundsPlugin;
use crate::camera::gameplay_camera_2d;
use crate::config::display::{VIEW_HEIGHT, VIEW_WIDTH};
use crate::minigames::smoothing::{Smoothed, remember_previous_tick, smoothed};
use crate::players::player_color;

/// Covers the whole screen: the floor, the border wall and the cover, drawn to match
/// [`config`](super::config).
const ARENA_IMAGE: &str = "minigames/shootout/arena.png";
/// A gunner seen from above, aiming right, centred on its body. Drawn in white and grey, and
/// tinted with its player's colour.
const GUNNER_IMAGE: &str = "minigames/shootout/gunner.png";
/// A round paint ball drawn white with a dark outline, tinted with its shooter's colour.
const PAINT_BALL_IMAGE: &str = "minigames/shootout/paint_ball.png";
/// The gunner image's size, in world units: the body, and the barrel sticking out to the right.
/// Twice [`GUNNER_RADIUS`] wide for the body, plus the barrel on each side so it stays centred.
const GUNNER_IMAGE_SIZE: f32 = 2.0 * GUNNER_RADIUS + 12.0;

/// Draw order shared across shootout visual sub-plugins: higher values are drawn on top.
const ARENA_Z: f32 = 0.0;
pub(super) const SPLAT_Z: f32 = 0.2;
pub(super) const PICKUP_Z: f32 = 0.6;
const GUNNER_Z: f32 = 1.0;
const PAINT_BALL_Z: f32 = 2.0;
pub(super) const ZONE_Z: f32 = 2.5;
pub(super) const ZONE_BORDER_Z: f32 = 2.55;
pub(super) const HEALTH_BAR_Z: f32 = 3.0;

/// How long a gunner flashes white after taking a hit, in seconds.
const HIT_FLASH_SECONDS: f32 = 0.12;

/// System set for syncing smoothed gunner transforms in `PostUpdate`, so floating overlays such as
/// [`health_bars`] can follow the updated positions in the same frame.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct SyncGunnerTransform;

/// Tracks a gunner's last seen health and white hit-flash timer.
#[derive(Component)]
struct HitFlash {
    last_health: u32,
    timer: Timer,
}

/// Local-only visuals and sounds for the shootout.
pub struct ShootoutVisualsPlugin;

impl Plugin for ShootoutVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ShootoutSoundsPlugin,
            splats::PaintSplatsPlugin,
            health_bars::HealthBarsPlugin,
            pickups::PickupVisualsPlugin,
            zone::ZoneVisualsPlugin,
        ));
        app.add_systems(OnEnter(PHASE), (spawn_camera, spawn_arena));
        app.add_systems(OnExit(PHASE), reset_cursor);
        app.add_observer(add_gunner_sprite);
        app.add_observer(add_paint_ball_sprite);
        app.add_systems(
            FixedFirst,
            (
                remember_previous_tick::<GunnerState>,
                remember_previous_tick::<PaintBallPosition>,
            )
                .run_if(in_state(PHASE)),
        );
        app.add_systems(Update, update_hit_flash.run_if(in_state(PHASE)));
        app.add_systems(
            PostUpdate,
            (
                sync_gunner_transform
                    .in_set(SyncGunnerTransform)
                    .before(TransformSystems::Propagate),
                sync_paint_ball_transform.before(TransformSystems::Propagate),
                update_crosshair_cursor.before(CursorSystems::Update),
            )
                .run_if(in_state(PHASE)),
        );
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
    commands.entity(trigger.entity).insert((
        Sprite {
            color: player_color(gunner.color),
            custom_size: Some(Vec2::splat(GUNNER_IMAGE_SIZE)),
            ..Sprite::from_image(assets.load(GUNNER_IMAGE))
        },
        Transform::from_xyz(position.x, position.y, GUNNER_Z)
            .with_rotation(Quat::from_rotation_z(aim)),
    ));
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

    use super::{HIT_FLASH_SECONDS, reset_cursor, update_crosshair_cursor, update_hit_flash};
    use crate::minigames::shootout::ShootoutInput;
    use crate::minigames::shootout::gunner::{Gunner, Health};
    use crate::players::player_color;

    #[test]
    fn hit_flash_turns_white_when_health_drops_and_restores_color() {
        let mut world = World::new();
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
