use bevy::prelude::*;

use super::PHASE;
use super::ball::{Ball, BallPosition};
use super::config::{BALL_MAX_SPEED, BALL_SIZE, SCORE_FONT_SIZE, WALL_THICKNESS};
use super::paddles::{Paddle, PaddlePosition, Side};
use super::score::PongScore;
use super::sounds::PongSoundsPlugin;
use crate::camera::gameplay_camera_2d;
use crate::config::display::{ART_SCALE, VIEW_HEIGHT, VIEW_WIDTH};
use crate::config::network::TICK_RATE;
use crate::minigames::pong_paddle::{PADDLE_SPRITE_HEIGHT, PADDLE_SPRITE_WIDTH};
use crate::minigames::smoothing::{PreviousTick, Smoothed, remember_previous_tick, smoothed};
use crate::players::player_color;

const ARENA_IMAGE: &str = "minigames/two_player_pong/arena.png";
const BALL_IMAGE: &str = "minigames/two_player_pong/ball.png";

/// Draw order: higher values are drawn on top.
const ARENA_Z: f32 = 0.0;
const PADDLE_Z: f32 = 1.0;
const BALL_Z: f32 = 2.0;
const SCORE_Z: f32 = 3.0;

/// A ball further than this from where the tick before left it was put back in the centre after a
/// goal. Twice as far as it can move in a tick.
const BALL_RESET_STEP: f32 = 2.0 * BALL_MAX_SPEED / TICK_RATE as f32;

const SCORE_X_OFFSET: f32 = 40.0;
const SCORE_Y: f32 = VIEW_HEIGHT / 2.0 - WALL_THICKNESS - SCORE_FONT_SIZE / 2.0 - 4.0;

#[derive(Component)]
pub struct Arena;

/// Local text showing one side's score.
#[derive(Component)]
pub struct ScoreText(pub Side);

/// Local-only sprites, score text and sounds for Pong.
pub struct PongVisualsPlugin;

impl Plugin for PongVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PongSoundsPlugin);
        app.add_systems(
            OnEnter(PHASE),
            (spawn_camera, spawn_arena, spawn_score_text),
        );
        app.add_observer(add_paddle_sprite);
        app.add_observer(add_ball_sprite);
        app.add_systems(
            FixedFirst,
            (
                remember_previous_tick::<PaddlePosition>,
                remember_previous_tick::<BallPosition>,
            )
                .run_if(in_state(PHASE)),
        );
        app.add_systems(
            PostUpdate,
            (sync_paddle_transform, sync_ball_transform)
                .before(TransformSystems::Propagate)
                .run_if(in_state(PHASE)),
        );
        app.add_systems(Update, update_score_text.run_if(in_state(PHASE)));
    }
}

/// The arena fills the screen.
fn spawn_camera(mut commands: Commands) {
    debug!("Spawning the Pong camera");
    commands.spawn(gameplay_camera_2d(PHASE, VIEW_WIDTH, VIEW_HEIGHT));
}

fn spawn_arena(assets: Res<AssetServer>, mut commands: Commands) {
    commands.spawn((
        Arena,
        DespawnOnExit(PHASE),
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
            custom_size: Some(Vec2::new(PADDLE_SPRITE_WIDTH, PADDLE_SPRITE_HEIGHT)),
            flip_x: paddle.side == Side::Right,
            ..Sprite::from_image(assets.load(paddle.shape.image()))
        },
        Transform::from_xyz(paddle.side.x_pos(), 0.0, PADDLE_Z),
    ));
}

/// Draw each paddle between its last two ticks (see [`smoothing`](crate::minigames::smoothing)).
fn sync_paddle_transform(
    time: Res<Time<Fixed>>,
    mut paddles: Query<Smoothed<PaddlePosition>, With<Paddle>>,
) {
    for (position, previous, mut transform) in &mut paddles {
        let shown = smoothed(position, previous, time.overstep_fraction());
        transform.translation.x = shown.0.x;
        transform.translation.y = shown.0.y;
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

fn sync_ball_transform(
    time: Res<Time<Fixed>>,
    mut balls: Query<Smoothed<BallPosition>, With<Ball>>,
) {
    for (position, previous, mut transform) in &mut balls {
        let shown = shown_ball_position(*position, previous, time.overstep_fraction());
        transform.translation.x = shown.x;
        transform.translation.y = shown.y;
    }
}

/// Where to draw the ball: between its last two ticks (see
/// [`smoothing`](crate::minigames::smoothing)), except right after a goal put it back in the
/// centre, so it doesn't slide back across the arena.
fn shown_ball_position(
    position: BallPosition,
    previous: Option<&PreviousTick<BallPosition>>,
    overstep: f32,
) -> Vec2 {
    let previous = previous
        .filter(|PreviousTick(previous)| previous.0.distance(position.0) <= BALL_RESET_STEP);
    smoothed(&position, previous, overstep).0
}

fn spawn_score_text(mut commands: Commands) {
    for side in [Side::Left, Side::Right] {
        commands.spawn((
            ScoreText(side),
            DespawnOnExit(PHASE),
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

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::{BallPosition, shown_ball_position};
    use crate::config::network::TICK_RATE;
    use crate::minigames::smoothing::PreviousTick;
    use crate::minigames::two_player_pong::config::{BALL_MAX_SPEED, GOAL_X};

    #[test]
    fn the_ball_is_drawn_between_its_last_two_ticks() {
        let step = BALL_MAX_SPEED / TICK_RATE as f32;
        let previous = PreviousTick(BallPosition(Vec2::new(100.0, 0.0)));
        let shown = shown_ball_position(
            BallPosition(Vec2::new(100.0 + step, 0.0)),
            Some(&previous),
            0.5,
        );
        assert_eq!(shown, Vec2::new(100.0 + step / 2.0, 0.0));
    }

    #[test]
    fn a_ball_put_back_after_a_goal_is_drawn_in_the_centre() {
        let previous = PreviousTick(BallPosition(Vec2::new(GOAL_X, 40.0)));
        let shown = shown_ball_position(BallPosition(Vec2::ZERO), Some(&previous), 0.5);
        assert_eq!(shown, Vec2::ZERO);
    }
}
