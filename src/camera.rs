//! Cameras. One [`UiCamera`] lives for the whole app and draws every screen's UI on top, at the
//! window's own resolution. It draws no world: each minigame spawns its own gameplay camera (see
//! [`gameplay_camera_2d`]) when it starts, which goes away when the minigame ends. A minigame can
//! pick its own zoom, or a 3D camera and lights, without affecting the menus or other minigames.

use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

/// The UI camera draws after every gameplay camera (which keep the default order, 0).
const UI_CAMERA_ORDER: isize = 1;

/// Spawns the [`UiCamera`] and keeps it clearing the window only when no minigame is drawing.
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ui_camera);
        app.add_systems(PostUpdate, clear_only_without_gameplay);
    }
}

/// The camera all UI is drawn with: menus, lobby, results, and minigame HUDs.
#[derive(Component)]
pub struct UiCamera;

/// A minigame's camera for its world (sprites, gizmos, `Text2d`). Drawn under the [`UiCamera`].
#[derive(Component)]
pub struct GameplayCamera;

fn spawn_ui_camera(mut commands: Commands) {
    commands.spawn((
        UiCamera,
        Camera2d,
        Camera {
            order: UI_CAMERA_ORDER,
            ..default()
        },
        IsDefaultUiCamera,
        // UI ignores render layers, so this camera still draws all of it, but no world.
        RenderLayers::none(),
    ));
}

/// A 2D gameplay camera for the minigame played in `phase`, despawned when it ends. It always
/// shows at least `min_width` × `min_height` world units; other aspect ratios get extra space at
/// the edges instead of stretched art.
pub fn gameplay_camera_2d<S: States>(phase: S, min_width: f32, min_height: f32) -> impl Bundle {
    (
        GameplayCamera,
        DespawnOnExit(phase),
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width,
                min_height,
            },
            ..OrthographicProjection::default_2d()
        }),
    )
}

/// With a minigame on screen the UI is drawn over it, so the [`UiCamera`] mustn't clear the
/// window. Without one, it's the only camera, and has to.
fn clear_only_without_gameplay(
    mut ui: Single<&mut Camera, With<UiCamera>>,
    gameplay: Query<(), With<GameplayCamera>>,
) {
    let alone = gameplay.is_empty();
    if alone != matches!(ui.clear_color, ClearColorConfig::Default) {
        ui.clear_color = if alone {
            ClearColorConfig::Default
        } else {
            ClearColorConfig::None
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{GameplayCamera, UiCamera, clear_only_without_gameplay};
    use bevy::prelude::*;

    fn ui_clears(world: &mut World) -> bool {
        world
            .run_system_cached(clear_only_without_gameplay)
            .unwrap();
        let mut ui = world.query_filtered::<&Camera, With<UiCamera>>();
        matches!(
            ui.single(world).unwrap().clear_color,
            ClearColorConfig::Default
        )
    }

    #[test]
    fn the_ui_camera_clears_only_when_no_minigame_is_drawing() {
        let mut world = World::new();
        world.spawn((UiCamera, Camera::default()));
        assert!(ui_clears(&mut world), "menus only");

        let gameplay = world.spawn(GameplayCamera).id();
        assert!(!ui_clears(&mut world), "drawn over a minigame");

        world.despawn(gameplay);
        assert!(ui_clears(&mut world), "back in the menus");
    }
}
