//! Local-only sprites and cursor for the shootout: the arena, a sprite, name and health pips per
//! gunner, paint balls, and a crosshair cursor while this player controls a gunner.

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
use crate::config::display::{ART_SCALE, VIEW_HEIGHT, VIEW_WIDTH};
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

/// Draw order: higher values are drawn on top.
const ARENA_Z: f32 = 0.0;
const GUNNER_Z: f32 = 1.0;
const PAINT_BALL_Z: f32 = 2.0;
const HEALTH_BAR_Z: f32 = 3.0;

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
}

/// One hit-point pip inside a [`HealthBar`], numbered from 0.
#[derive(Component)]
struct HealthPip(u32);

/// The player's name label inside a [`HealthBar`].
#[derive(Component)]
struct GunnerName;

/// Local-only visuals for the shootout.
pub struct ShootoutVisualsPlugin;

impl Plugin for ShootoutVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ShootoutSoundsPlugin);
        app.add_systems(OnEnter(PHASE), (spawn_camera, spawn_arena));
        app.add_systems(OnExit(PHASE), reset_cursor);
        app.add_observer(add_gunner_sprite);
        app.add_observer(spawn_health_bar);
        app.add_observer(add_paint_ball_sprite);
        app.add_systems(
            FixedFirst,
            (
                remember_previous_tick::<GunnerState>,
                remember_previous_tick::<PaintBallPosition>,
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

fn spawn_health_bar(
    trigger: On<Add<Health>>,
    gunners: Query<(&Gunner, &Health, Option<&GunnerState>)>,
    mut commands: Commands,
) {
    let Ok((gunner, health, state)) = gunners.get(trigger.entity) else {
        return;
    };
    if health.max == 0 {
        return;
    }
    let position = state.map_or(Vec2::ZERO, |state| state.position);
    let side = label_side(position.y);
    let total_width = health.max as f32 * PIP_SIZE + health.max.saturating_sub(1) as f32 * PIP_GAP;
    let first_x = -total_width / 2.0 + PIP_SIZE / 2.0;
    commands
        .spawn((
            HealthBar {
                gunner: trigger.entity,
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
/// remaining hits; despawn bars whose gunner was knocked out or left.
fn sync_health_bars(
    gunners: Query<(&Transform, &Health), With<Gunner>>,
    mut bars: HealthBarQuery,
    mut pips: Query<(&HealthPip, &mut Sprite)>,
    mut names: Query<&mut Transform, (With<GunnerName>, Without<Gunner>)>,
    mut commands: Commands,
) {
    for (bar_entity, bar, mut bar_transform, children) in &mut bars {
        let Ok((gunner_transform, health)) = gunners.get(bar.gunner) else {
            commands.entity(bar_entity).despawn();
            continue;
        };
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
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};
    use lightyear::prelude::input::native::InputMarker;

    use super::{
        GunnerName, LABEL_FLIP_Y, label_side, reset_cursor, spawn_health_bar, sync_health_bars,
        update_crosshair_cursor,
    };
    use crate::minigames::shootout::ShootoutInput;
    use crate::minigames::shootout::gunner::{Gunner, GunnerState, Health};
    use lightyear::prelude::PeerId;

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
