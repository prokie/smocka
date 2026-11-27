//! Local-only sprites and cursor for the shootout: the arena, a sprite per gunner turned the way
//! it aims, and a crosshair cursor while this player controls a gunner.

use bevy::prelude::*;
use bevy::window::{CursorIcon, CursorSystems, PrimaryWindow, SystemCursorIcon};
use lightyear::prelude::input::native::InputMarker;

use super::PHASE;
use super::ShootoutInput;
use super::config::GUNNER_RADIUS;
use super::gunner::{Gunner, GunnerState};
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
/// The gunner image's size, in world units: the body, and the barrel sticking out to the right.
/// Twice [`GUNNER_RADIUS`] wide for the body, plus the barrel on each side so it stays centred.
const GUNNER_IMAGE_SIZE: f32 = 2.0 * GUNNER_RADIUS + 12.0;

/// Draw order: higher values are drawn on top.
const ARENA_Z: f32 = 0.0;
const GUNNER_Z: f32 = 1.0;

/// Local-only visuals for the shootout.
pub struct ShootoutVisualsPlugin;

impl Plugin for ShootoutVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(PHASE), (spawn_camera, spawn_arena));
        app.add_systems(OnExit(PHASE), reset_cursor);
        app.add_observer(add_gunner_sprite);
        app.add_systems(
            FixedFirst,
            remember_previous_tick::<GunnerState>.run_if(in_state(PHASE)),
        );
        app.add_systems(
            PostUpdate,
            (
                sync_gunner_transform.before(TransformSystems::Propagate),
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
    gunners: Query<&Gunner>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Ok(gunner) = gunners.get(trigger.entity) else {
        return;
    };
    commands.entity(trigger.entity).insert((
        Sprite {
            color: player_color(gunner.color),
            custom_size: Some(Vec2::splat(GUNNER_IMAGE_SIZE)),
            ..Sprite::from_image(assets.load(GUNNER_IMAGE))
        },
        Transform::from_xyz(0.0, 0.0, GUNNER_Z),
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

    use super::{reset_cursor, update_crosshair_cursor};
    use crate::minigames::shootout::ShootoutInput;

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
