//! Non-rotating health pips and player name label following each gunner.

use bevy::prelude::*;

use super::{HEALTH_BAR_Z, SyncGunnerTransform};
use crate::config::display::ART_SCALE;
use crate::minigames::shootout::PHASE;
use crate::minigames::shootout::config::{ARENA_HEIGHT, GUNNER_RADIUS};
use crate::minigames::shootout::gunner::{Gunner, GunnerState, Health};

const PIP_SIZE: f32 = 3.0;
const PIP_GAP: f32 = 1.5;
const HEALTH_BAR_OFFSET_Y: f32 = GUNNER_RADIUS + 5.5;
const NAME_FONT_SIZE: f32 = 6.5;
const NAME_OFFSET_Y: f32 = (PIP_SIZE + 2.0) / 2.0 + NAME_FONT_SIZE / 2.0 + 1.5;
/// Above this y, the name would reach past the top of the arena, so flip the bar and name below.
const LABEL_FLIP_Y: f32 =
    ARENA_HEIGHT / 2.0 - (HEALTH_BAR_OFFSET_Y + NAME_OFFSET_Y + NAME_FONT_SIZE / 2.0);
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

/// Spawns and updates each gunner's floating health bar and name label.
pub(super) struct HealthBarsPlugin;

impl Plugin for HealthBarsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(spawn_health_bar);
        app.add_systems(
            PostUpdate,
            sync_health_bars
                .after(SyncGunnerTransform)
                .before(TransformSystems::Propagate)
                .run_if(in_state(PHASE)),
        );
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

type HealthBarParts = (
    Entity,
    &'static HealthBar,
    &'static mut Transform,
    &'static Children,
);

/// Keep each gunner's health bar and name above it (or below near the top edge) and lit for its
/// remaining hits; rebuild the bar if a Shield pickup grows `health.max`, and despawn bars whose
/// gunner was knocked out or left.
fn sync_health_bars(
    gunners: Query<(&Gunner, &Transform, &Health)>,
    mut bars: Query<HealthBarParts, (Without<Gunner>, Without<GunnerName>)>,
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

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use lightyear::prelude::PeerId;

    use super::{
        GunnerName, HealthPip, LABEL_FLIP_Y, label_side, spawn_health_bar, sync_health_bars,
    };
    use crate::minigames::shootout::gunner::{Gunner, GunnerState, Health};

    #[test]
    fn gunner_name_spawns_with_health_bar_flips_near_top_and_rebuilds_on_shield() {
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
}
