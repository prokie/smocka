//! Persistent paint splatter decals left on the arena floor when paint balls hit walls or gunners,
//! or when a gunner is knocked out.

use std::collections::VecDeque;
use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::RngExt;

use super::SPLAT_Z;
use crate::minigames::shootout::PHASE;
use crate::minigames::shootout::gunner::GunnerKnockedOut;
use crate::minigames::shootout::shots::{PaintBallGunnerHit, PaintBallWallHit};
use crate::players::player_color;

/// A paint splat decal drawn white, tinted with the shooter's colour.
const PAINT_SPLAT_IMAGE: &str = "minigames/shootout/paint_splat.png";
const WALL_SPLAT_SIZE: f32 = 14.0;
const HIT_SPLAT_SIZE: f32 = 20.0;
const KNOCKOUT_BURST_COUNT: usize = 6;
const SPLAT_ALPHA: f32 = 0.85;
const MAX_SPLATS: usize = 160;

/// Live paint splat entities in oldest-first order, capped at [`MAX_SPLATS`].
#[derive(Resource, Default)]
struct PaintSplats {
    image: Handle<Image>,
    live: VecDeque<Entity>,
}

/// Leaves capped paint splatter decals during a shootout.
pub(super) struct PaintSplatsPlugin;

impl Plugin for PaintSplatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaintSplats>();
        app.add_systems(OnEnter(PHASE), setup_splats);
        app.add_systems(OnExit(PHASE), clear_splats);
        app.add_observer(splat_on_wall_hit);
        app.add_observer(splat_on_gunner_hit);
        app.add_observer(splat_on_knockout);
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
    trigger: On<GunnerKnockedOut>,
    mut splats: ResMut<PaintSplats>,
    mut commands: Commands,
) {
    let knockout = trigger.event();
    let mut rng = rand::rng();
    for index in 0..KNOCKOUT_BURST_COUNT {
        let angle =
            index as f32 * (TAU / KNOCKOUT_BURST_COUNT as f32) + rng.random_range(-0.3..0.3);
        let radius = rng.random_range(2.0..12.0);
        let offset = Vec2::from_angle(angle) * radius;
        spawn_splat(
            &mut commands,
            &mut splats,
            knockout.position + offset,
            knockout.color,
            HIT_SPLAT_SIZE * 0.9,
        );
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::{
        KNOCKOUT_BURST_COUNT, MAX_SPLATS, PaintSplats, splat_on_knockout, splat_on_wall_hit,
    };
    use crate::minigames::shootout::gunner::GunnerKnockedOut;
    use crate::minigames::shootout::shots::PaintBallWallHit;

    #[test]
    fn paint_splats_cap_at_max_and_burst_on_knockout() {
        let mut world = World::new();
        world.init_resource::<PaintSplats>();
        world.add_observer(splat_on_knockout);
        world.add_observer(splat_on_wall_hit);

        world.trigger(GunnerKnockedOut {
            position: Vec2::ZERO,
            color: 0,
        });
        assert_eq!(
            world.resource::<PaintSplats>().live.len(),
            KNOCKOUT_BURST_COUNT
        );

        for index in 0..(MAX_SPLATS + 5) {
            world.trigger(PaintBallWallHit {
                position: Vec2::new(index as f32, 0.0),
                color: 0,
            });
        }
        assert_eq!(world.resource::<PaintSplats>().live.len(), MAX_SPLATS);
    }
}
