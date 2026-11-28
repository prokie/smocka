//! Local-only visuals for Pong Royale: the camera, the arena, paddles, ball and walls, and a label
//! for each player saying which side is theirs and how many lives they have left.

use bevy::prelude::*;
use lightyear::prelude::input::native::InputMarker;

use super::PHASE;
use super::RoyaleInput;
use super::arena::Side;
use super::config::{ARENA_HALF, BALL_MAX_SPEED, BALL_SIZE, GOAL_HALF_WIDTH, WALL_THICKNESS};
use super::paddles::{Lives, Paddle, PaddlePosition};
use super::play::{Ball, BallPosition, Wall};
use super::sounds::RoyaleSoundsPlugin;
use crate::camera::gameplay_camera_2d;
use crate::config::display::{ART_SCALE, VIEW_HEIGHT, VIEW_WIDTH};
use crate::config::network::TICK_RATE;
use crate::minigames::pong_paddle::{PADDLE_SPRITE_HEIGHT, PADDLE_SPRITE_WIDTH};
use crate::minigames::smoothing::{PreviousTick, Smoothed, remember_previous_tick, smoothed};
use crate::players::player_color;

const ARENA_IMAGE: &str = "minigames/pong_royale/arena.png";
const BALL_IMAGE: &str = "minigames/two_player_pong/ball.png";

/// The colour of a wall across a goal. The same as the arena's corner blocks.
const WALL_COLOR: Color = Color::srgb(0.376, 0.392, 0.486);
/// A label for a player who's out.
const OUT_COLOR: Color = Color::srgb(0.45, 0.45, 0.5);

/// Draw order: higher values are drawn on top.
const ARENA_Z: f32 = 0.0;
const WALL_Z: f32 = 0.5;
const PADDLE_Z: f32 = 1.0;
const BALL_Z: f32 = 2.0;
const LABEL_Z: f32 = 3.0;

/// A ball further than this from where the tick before left it was served again from the
/// centre: draw it there rather than sliding it back. Twice as far as it can move in a tick.
const BALL_RESET_STEP: f32 = 2.0 * BALL_MAX_SPEED / TICK_RATE as f32;

/// Labels sit in the space either side of the arena, in world units.
const LABEL_SIZE: f32 = 14.0;
const LABEL_X: f32 = (ARENA_HALF + VIEW_WIDTH / 2.0) / 2.0;
const LABEL_END_Y: f32 = ARENA_HALF - 30.0;

#[derive(Component)]
pub struct Arena;

/// The label for whoever plays `side`, made when their paddle first arrives and kept after they
/// go out. Holds their name, since their paddle goes when they do.
#[derive(Component, Debug)]
pub struct SideLabel {
    pub side: Side,
    name: String,
    color: u8,
}

/// Local-only sprites, labels and sounds for Pong Royale.
pub struct RoyaleVisualsPlugin;

impl Plugin for RoyaleVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RoyaleSoundsPlugin);
        app.add_systems(OnEnter(PHASE), (spawn_camera, spawn_arena));
        app.add_observer(add_paddle_sprite);
        app.add_observer(add_ball_sprite);
        app.add_observer(add_wall_sprite);
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
        app.add_systems(Update, update_labels.run_if(in_state(PHASE)));
    }
}

/// The whole arena, and the labels either side of it, fit on screen.
fn spawn_camera(mut commands: Commands) {
    debug!("Spawning the Pong Royale camera");
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

/// Which way a side's paddle and wall are turned: those on the bottom and top lie on their side.
fn side_rotation(side: Side) -> Quat {
    match side {
        Side::Left | Side::Right => Quat::IDENTITY,
        Side::Bottom | Side::Top => Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    }
}

/// Where `side`'s label goes: the left and right players' beside their own side, the bottom
/// player's low on the right and the top player's high on the left.
fn label_position(side: Side) -> Vec2 {
    match side {
        Side::Left => Vec2::new(-LABEL_X, 0.0),
        Side::Right => Vec2::new(LABEL_X, 0.0),
        Side::Bottom => Vec2::new(LABEL_X, -LABEL_END_Y),
        Side::Top => Vec2::new(-LABEL_X, LABEL_END_Y),
    }
}

/// What `side`'s label says: an arrow to their side and their name (and "you" for this player),
/// then a heart for each life left, or that they're out.
pub fn label_text(side: Side, name: &str, you: bool, lives: Option<u32>) -> String {
    let you = if you { " (you)" } else { "" };
    let lives = match lives {
        Some(lives) => "♥".repeat(lives as usize),
        None => "out".into(),
    };
    format!("{} {name}{you}\n{lives}", side.arrow())
}

fn add_paddle_sprite(
    trigger: On<Add<Paddle>>,
    paddles: Query<&Paddle>,
    labels: Query<&SideLabel>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Ok(paddle) = paddles.get(trigger.entity) else {
        return;
    };
    // Paddle sprites face +x (right); rotate so the shaped front points inward into the arena.
    let paddle_rotation = Quat::from_rotation_z((-paddle.side.normal()).to_angle());
    commands.entity(trigger.entity).insert((
        Sprite {
            color: player_color(paddle.color),
            custom_size: Some(Vec2::new(PADDLE_SPRITE_WIDTH, PADDLE_SPRITE_HEIGHT)),
            ..Sprite::from_image(assets.load(paddle.shape.image()))
        },
        Transform::from_translation(paddle.side.paddle_centre(0.0).extend(PADDLE_Z))
            .with_rotation(paddle_rotation),
    ));
    if labels.iter().any(|label| label.side == paddle.side) {
        return;
    }
    commands.spawn((
        SideLabel {
            side: paddle.side,
            name: paddle.name.clone(),
            color: paddle.color,
        },
        DespawnOnExit(PHASE),
        Text2d::new(""),
        // Text2d is drawn at the window's scale factor, not the camera's zoom, so draw it
        // ART_SCALE times bigger and shrink it back down, or it blurs when the camera zooms in.
        TextFont {
            font_size: FontSize::Px(LABEL_SIZE * ART_SCALE),
            ..default()
        },
        TextLayout::justify(Justify::Center),
        TextColor(player_color(paddle.color)),
        Transform::from_translation(label_position(paddle.side).extend(LABEL_Z))
            .with_scale(Vec3::splat(1.0 / ART_SCALE)),
    ));
}

/// Draw each paddle between its last two ticks (see [`smoothing`](crate::minigames::smoothing)).
fn sync_paddle_transform(
    time: Res<Time<Fixed>>,
    mut paddles: Query<(&Paddle, Smoothed<PaddlePosition>)>,
) {
    for (paddle, (position, previous, mut transform)) in &mut paddles {
        let shown = smoothed(position, previous, time.overstep_fraction());
        let centre = paddle.side.paddle_centre(shown.0);
        transform.translation.x = centre.x;
        transform.translation.y = centre.y;
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
        // Right after a serve, draw it in the centre, not sliding back from the goal.
        let previous = previous
            .filter(|PreviousTick(previous)| previous.0.distance(position.0) <= BALL_RESET_STEP);
        let shown = smoothed(position, previous, time.overstep_fraction()).0;
        transform.translation.x = shown.x;
        transform.translation.y = shown.y;
    }
}

fn add_wall_sprite(trigger: On<Add<Wall>>, walls: Query<&Wall>, mut commands: Commands) {
    let Ok(Wall(side)) = walls.get(trigger.entity) else {
        return;
    };
    let centre = side.normal() * (ARENA_HALF - WALL_THICKNESS / 2.0);
    commands.entity(trigger.entity).insert((
        Sprite::from_color(WALL_COLOR, Vec2::new(WALL_THICKNESS, 2.0 * GOAL_HALF_WIDTH)),
        Transform::from_translation(centre.extend(WALL_Z)).with_rotation(side_rotation(*side)),
    ));
}

/// Keep each player's label up to date: their lives, or that they're out once their paddle has
/// gone.
fn update_labels(
    paddles: Query<(&Paddle, &Lives, Has<InputMarker<RoyaleInput>>)>,
    mut labels: Query<(&SideLabel, &mut Text2d, &mut TextColor)>,
) {
    for (label, mut text, mut color) in &mut labels {
        let playing = paddles
            .iter()
            .find(|(paddle, _, _)| paddle.side == label.side);
        let you = playing.is_some_and(|(_, _, you)| you);
        let lives = playing.map(|(_, lives, _)| lives.0);
        let shown = label_text(label.side, &label.name, you, lives);
        if text.0 != shown {
            text.0 = shown;
        }
        let shade = if lives.is_some() {
            player_color(label.color)
        } else {
            OUT_COLOR
        };
        color.set_if_neq(TextColor(shade));
    }
}

#[cfg(test)]
mod tests {
    use super::label_text;
    use crate::minigames::pong_royale::arena::Side;

    #[test]
    fn a_label_points_to_its_side_and_shows_lives_or_out() {
        assert_eq!(label_text(Side::Top, "Ada", false, Some(3)), "↑ Ada\n♥♥♥");
        assert_eq!(label_text(Side::Left, "Bo", true, Some(1)), "← Bo (you)\n♥");
        assert_eq!(label_text(Side::Right, "Cy", false, None), "→ Cy\nout");
    }
}
