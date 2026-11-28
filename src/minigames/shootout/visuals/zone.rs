//! Sudden-death danger zone visuals: four non-overlapping crimson strips outside [`SafeZone`], a
//! bright border frame, and a brief `"SUDDEN DEATH!"` banner when shrinking begins.

use bevy::prelude::*;

use super::{ZONE_BORDER_Z, ZONE_Z};
use crate::minigames::shootout::PHASE;
use crate::minigames::shootout::config::{ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH};
use crate::minigames::shootout::zone::SafeZone;

const ZONE_DANGER_COLOR: Color = Color::srgba(0.85, 0.12, 0.16, 0.34);
const ZONE_BORDER_COLOR: Color = Color::srgba(1.0, 0.3, 0.25, 0.92);
const ZONE_BORDER_THICKNESS: f32 = 2.0;
const SUDDEN_DEATH_BANNER_SECONDS: f32 = 2.5;
const SUDDEN_DEATH_TEXT_SIZE: f32 = 34.0;
const SUDDEN_DEATH_TOP_MARGIN: f32 = 20.0;

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

/// Renders the sudden-death danger overlay and banner.
pub(super) struct ZoneVisualsPlugin;

impl Plugin for ZoneVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(PHASE),
            (spawn_zone_overlay, spawn_sudden_death_banner),
        );
        app.add_systems(Update, update_sudden_death_banner.run_if(in_state(PHASE)));
        app.add_systems(
            PostUpdate,
            sync_safe_zone_visuals
                .before(TransformSystems::Propagate)
                .run_if(in_state(PHASE)),
        );
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

type ZoneStripParts = (
    &'static ZoneStrip,
    &'static mut Sprite,
    &'static mut Transform,
    &'static mut Visibility,
);

type ZoneBorderParts = (
    &'static ZoneBorder,
    &'static mut Sprite,
    &'static mut Transform,
    &'static mut Visibility,
);

fn sync_safe_zone_visuals(
    zones: Query<&SafeZone>,
    mut strips: Query<ZoneStripParts, Without<ZoneBorder>>,
    mut borders: Query<ZoneBorderParts, Without<ZoneStrip>>,
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

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;

    use super::{ZoneStrip, spawn_zone_overlay, sync_safe_zone_visuals, zone_strip_rect};
    use crate::minigames::shootout::config::{ARENA_HALF_HEIGHT, ARENA_HALF_WIDTH};
    use crate::minigames::shootout::zone::SafeZone;

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
}
