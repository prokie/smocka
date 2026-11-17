//! Local-only rendering for two-player Pong: sprites for the arena and the paddles.
//!
//! None of this is replicated. Each peer attaches sprites to the replicated game entities it sees.
//! The art lives in `assets/minigames/two_player_pong/`. Overwrite those PNGs to change how the
//! game looks: no code changes are needed as long as the file names stay the same.

use bevy::prelude::*;

use super::paddles::{Paddle, PaddlePosition, Side};

/// Arena background image (640×360).
const ARENA_IMAGE: &str = "minigames/two_player_pong/arena.png";
/// Paddle image (8×48). Drawn in white/grey and tinted per side with [`Side::color`].
const PADDLE_IMAGE: &str = "minigames/two_player_pong/paddle.png";

/// Draw order: higher values are drawn on top.
const ARENA_Z: f32 = 0.0;
const PADDLE_Z: f32 = 1.0;

/// Marker for the local arena background sprite.
#[derive(Component)]
pub struct Arena;

/// Adds sprites to Pong entities. Needs a renderer and an [`AssetServer`], so it is added by the
/// binary (through [`MinigameVisualsPlugin`](crate::minigames::MinigameVisualsPlugin)), not by
/// headless tests.
pub struct PongVisualsPlugin;

impl Plugin for PongVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(add_paddle_sprite);
        app.add_systems(
            PostUpdate,
            sync_paddle_transform.before(TransformSystems::Propagate),
        );
    }
}

impl Side {
    /// Tint applied to this side's paddle sprite.
    pub fn color(self) -> Color {
        match self {
            Side::Left => Color::srgb(0.35, 0.65, 1.0),
            Side::Right => Color::srgb(1.0, 0.4, 0.4),
        }
    }
}

/// When a paddle appears, give it a sprite, and spawn the arena if it isn't there yet.
fn add_paddle_sprite(
    trigger: On<Add<Paddle>>,
    paddles: Query<&Paddle>,
    arenas: Query<(), With<Arena>>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Ok(paddle) = paddles.get(trigger.entity) else {
        return;
    };
    commands.entity(trigger.entity).insert((
        Sprite {
            color: paddle.side.color(),
            ..Sprite::from_image(assets.load(PADDLE_IMAGE))
        },
        Transform::from_xyz(paddle.side.x_pos(), 0.0, PADDLE_Z),
    ));

    if arenas.is_empty() {
        commands.spawn((
            Arena,
            Sprite::from_image(assets.load(ARENA_IMAGE)),
            Transform::from_xyz(0.0, 0.0, ARENA_Z),
        ));
    }
}

/// Move each paddle sprite to its (predicted or interpolated) [`PaddlePosition`].
fn sync_paddle_transform(mut paddles: Query<(&PaddlePosition, &mut Transform), With<Paddle>>) {
    for (position, mut transform) in &mut paddles {
        transform.translation.x = position.0.x;
        transform.translation.y = position.0.y;
    }
}
