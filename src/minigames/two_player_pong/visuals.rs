use bevy::prelude::*;

use super::ball::{Ball, BallPosition};
use super::paddles::{Paddle, PaddlePosition, Side};
use super::score::PongScore;
use crate::camera::gameplay_camera_2d;
use crate::config::display::{ART_SCALE, VIEW_HEIGHT, VIEW_WIDTH};
use crate::config::minigames::two_player_pong::{
    BALL_SIZE, PADDLE_HEIGHT, PADDLE_WIDTH, SCORE_FONT_SIZE, WALL_THICKNESS,
};
use crate::players::player_color;
use crate::session::SessionPhase;

const ARENA_IMAGE: &str = "minigames/two_player_pong/arena.png";
/// Drawn in white/grey and tinted with its player's colour.
const PADDLE_IMAGE: &str = "minigames/two_player_pong/paddle.png";
const BALL_IMAGE: &str = "minigames/two_player_pong/ball.png";

/// Draw order: higher values are drawn on top.
const ARENA_Z: f32 = 0.0;
const PADDLE_Z: f32 = 1.0;
const BALL_Z: f32 = 2.0;
const SCORE_Z: f32 = 3.0;

const SCORE_X_OFFSET: f32 = 40.0;
const SCORE_Y: f32 = VIEW_HEIGHT / 2.0 - WALL_THICKNESS - SCORE_FONT_SIZE / 2.0 - 4.0;

#[derive(Component)]
pub struct Arena;

/// Local text showing one side's score.
#[derive(Component)]
pub struct ScoreText(pub Side);

/// Local-only sprites and score text for Pong.
pub struct PongVisualsPlugin;

impl Plugin for PongVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(SessionPhase::TwoPlayerPong),
            (spawn_camera, spawn_arena, spawn_score_text),
        );
        app.add_observer(add_paddle_sprite);
        app.add_observer(add_ball_sprite);
        app.add_systems(
            PostUpdate,
            (sync_paddle_transform, sync_ball_transform).before(TransformSystems::Propagate),
        );
        app.add_systems(Update, update_score_text);
    }
}

/// The arena fills the screen.
fn spawn_camera(mut commands: Commands) {
    debug!("Spawning the Pong camera");
    commands.spawn(gameplay_camera_2d(
        SessionPhase::TwoPlayerPong,
        VIEW_WIDTH,
        VIEW_HEIGHT,
    ));
}

fn spawn_arena(assets: Res<AssetServer>, mut commands: Commands) {
    commands.spawn((
        Arena,
        DespawnOnExit(SessionPhase::TwoPlayerPong),
        Sprite {
            custom_size: Some(Vec2::new(VIEW_WIDTH, VIEW_HEIGHT)),
            ..Sprite::from_image(assets.load(ARENA_IMAGE))
        },
        Transform::from_xyz(0.0, 0.0, ARENA_Z),
    ));
}

fn add_paddle_sprite(
    trigger: On<Add<Paddle>>,
    paddles: Query<&Paddle>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Ok(paddle) = paddles.get(trigger.entity) else {
        return;
    };
    commands.entity(trigger.entity).insert((
        Sprite {
            color: player_color(paddle.color),
            custom_size: Some(Vec2::new(PADDLE_WIDTH, PADDLE_HEIGHT)),
            ..Sprite::from_image(assets.load(PADDLE_IMAGE))
        },
        Transform::from_xyz(paddle.side.x_pos(), 0.0, PADDLE_Z),
    ));
}

fn sync_paddle_transform(mut paddles: Query<(&PaddlePosition, &mut Transform), With<Paddle>>) {
    for (position, mut transform) in &mut paddles {
        transform.translation.x = position.0.x;
        transform.translation.y = position.0.y;
    }
}

fn add_ball_sprite(trigger: On<Add<Ball>>, assets: Res<AssetServer>, mut commands: Commands) {
    commands.entity(trigger.entity).insert((
        Sprite {
            custom_size: Some(Vec2::splat(BALL_SIZE)),
            ..Sprite::from_image(assets.load(BALL_IMAGE))
        },
        Transform::from_xyz(0.0, 0.0, BALL_Z),
    ));
}

fn sync_ball_transform(mut balls: Query<(&BallPosition, &mut Transform), With<Ball>>) {
    for (position, mut transform) in &mut balls {
        transform.translation.x = position.0.x;
        transform.translation.y = position.0.y;
    }
}

fn spawn_score_text(mut commands: Commands) {
    for side in [Side::Left, Side::Right] {
        commands.spawn((
            ScoreText(side),
            DespawnOnExit(SessionPhase::TwoPlayerPong),
            Text2d::new("0"),
            // Text2d is drawn at the window's scale factor, not the camera's zoom, so draw it
            // ART_SCALE times bigger and shrink it back down, or it blurs when the camera zooms in.
            TextFont {
                font_size: FontSize::Px(SCORE_FONT_SIZE * ART_SCALE),
                ..default()
            },
            // Takes the colour of this side's player once their paddle has arrived.
            TextColor(Color::WHITE),
            Transform::from_xyz(side.x_pos().signum() * SCORE_X_OFFSET, SCORE_Y, SCORE_Z)
                .with_scale(Vec3::splat(1.0 / ART_SCALE)),
        ));
    }
}

/// Show the latest [`PongScore`], each side in its player's colour.
fn update_score_text(
    score: Single<&PongScore>,
    paddles: Query<&Paddle>,
    mut texts: Query<(&ScoreText, &mut Text2d, &mut TextColor)>,
) {
    for (ScoreText(side), mut text, mut color) in &mut texts {
        let shown = score.of(*side).to_string();
        if text.0 != shown {
            text.0 = shown;
        }
        if let Some(paddle) = paddles.iter().find(|paddle| paddle.side == *side) {
            color.set_if_neq(TextColor(player_color(paddle.color)));
        }
    }
}
