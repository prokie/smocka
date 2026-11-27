//! Visuals for centre pickups (bobbing sprites) and active weapon buffs (a pulsing halo behind
//! the buffed gunner).

use bevy::prelude::*;
use lightyear::prelude::LocalTimeline;

use super::PICKUP_Z;
use crate::minigames::shootout::PHASE;
use crate::minigames::shootout::config::{GUNNER_RADIUS, PICKUP_RADIUS};
use crate::minigames::shootout::gunner::Gunner;
use crate::minigames::shootout::pickups::{BuffKind, GunnerBuff, Pickup, PickupKind};

const PICKUP_SPREAD_IMAGE: &str = "minigames/shootout/pickup_spread.png";
const PICKUP_RAPID_IMAGE: &str = "minigames/shootout/pickup_rapid.png";
const PICKUP_SHIELD_IMAGE: &str = "minigames/shootout/pickup_shield.png";
const BUFF_RING_IMAGE: &str = "minigames/shootout/buff_ring.png";
const BUFF_RING_SIZE: f32 = 2.0 * GUNNER_RADIUS + 10.0;

const PICKUP_BOB_HEIGHT: f32 = 1.5;
const PICKUP_BOB_SPEED: f32 = 3.5;
const SPREAD_BUFF_COLOR: Color = Color::srgba(0.98, 0.68, 0.2, 0.9);
const RAPID_BUFF_COLOR: Color = Color::srgba(0.25, 0.85, 1.0, 0.9);

/// Pulsing halo drawn behind a gunner while it holds an active weapon buff.
#[derive(Component)]
struct BuffHalo;

/// Renders centre pickups and active gunner buff halos.
pub(super) struct PickupVisualsPlugin;

impl Plugin for PickupVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(add_pickup_sprite);
        app.add_observer(attach_buff_halo);
        app.add_systems(
            Update,
            (animate_pickups, sync_buff_halos).run_if(in_state(PHASE)),
        );
    }
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

fn attach_buff_halo(trigger: On<Add<Gunner>>, assets: Res<AssetServer>, mut commands: Commands) {
    commands.entity(trigger.entity).with_children(|parent| {
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
