//! Shared paddle shapes for Two-Player Pong and Pong Royale: visual silhouettes, contour-aware
//! hit detection and bounce angles, plus each player's replicated pick from the pre-game setup
//! screen.

use bevy::prelude::*;
use lightyear::prelude::{server::ClientOf, *};
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::network::is_host;
use crate::players::{Player, ProfileChannel};

/// Width and height of every paddle shape sprite in world units (`tools/make_art.py`).
///
/// The sprite's centre `(0, 0)` matches the paddle's centre, with the flat back at `-4` and the
/// front face pointing right (`+x`), protruding up to `+8` for contoured shapes.
pub const PADDLE_SPRITE_WIDTH: f32 = 16.0;
pub const PADDLE_SPRITE_HEIGHT: f32 = 56.0;

/// Extra reach added to each end of [`PaddleShape::Split`] compared to the base paddle length.
const SPLIT_EXTRA_HALF_LENGTH: f32 = 4.0;
/// Half the open centre gap of [`PaddleShape::Split`], wide enough for an 8-unit ball to slip
/// through cleanly.
const SPLIT_GAP_HALF: f32 = 10.0;
/// How far the front centre of [`PaddleShape::Bumper`] bulges out past the flat face.
const BUMPER_BULGE: f32 = 3.0;
/// How far the tip of [`PaddleShape::Wedge`] points out past the flat face.
const WEDGE_TIP: f32 = 4.0;
/// Hard ceiling on bounce angle off straight back across the arena, so curved/wedge returns stay
/// playable and never crawl nearly vertically.
const MAX_SHAPED_BOUNCE_ANGLE: f32 = 55.0_f32.to_radians();

/// Which paddle silhouette and bounce profile a player uses in Pong and Pong Royale.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PaddleShape {
    /// Standard flat rectangle with balanced, predictable bounce angles.
    #[default]
    Classic,
    /// Convex rounded front that sends off-centre hits off at wider angles.
    Bumper,
    /// V-shaped nose pointing into the arena that deflects even centre hits sharply at an angle.
    Wedge,
    /// Two shorter twin pads with extra reach at both ends, but an open gap in the middle.
    Split,
}

impl PaddleShape {
    /// Every paddle shape, in the order the pre-game setup screen offers them.
    pub const ALL: [Self; 4] = [Self::Classic, Self::Bumper, Self::Wedge, Self::Split];

    /// Pick a shape uniformly at random (used during a [`Party`](crate::party::Party)).
    pub fn random(rng: &mut impl RngExt) -> Self {
        Self::ALL[rng.random_range(0..Self::ALL.len())]
    }

    /// Short display name for the setup screen and player list.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Bumper => "Bumper",
            Self::Wedge => "Wedge",
            Self::Split => "Split",
        }
    }

    /// Brief gameplay hint shown under the shape name on the setup screen.
    pub const fn summary(self) -> &'static str {
        match self {
            Self::Classic => "Flat & balanced",
            Self::Bumper => "Curved, wide angles",
            Self::Wedge => "Sharp V-nose",
            Self::Split => "Wide + centre gap",
        }
    }

    /// Asset path of the upright, right-facing white sprite (tinted with the player's colour).
    pub const fn image(self) -> &'static str {
        match self {
            Self::Classic => "minigames/two_player_pong/paddle_classic.png",
            Self::Bumper => "minigames/two_player_pong/paddle_bumper.png",
            Self::Wedge => "minigames/two_player_pong/paddle_wedge.png",
            Self::Split => "minigames/two_player_pong/paddle_split.png",
        }
    }

    /// Half the total span along the paddle's axis, given the minigame's `base_length`.
    pub fn half_length(self, base_length: f32) -> f32 {
        match self {
            Self::Split => base_length / 2.0 + SPLIT_EXTRA_HALF_LENGTH,
            Self::Classic | Self::Bumper | Self::Wedge => base_length / 2.0,
        }
    }

    /// How far *inward* (towards the arena centre, relative to the paddle's centre) the front face
    /// sits at `along` units from the paddle's midpoint.
    pub fn front_face_inward(self, along: f32, base_length: f32, thickness: f32) -> f32 {
        let base_face = thickness / 2.0;
        let half = self.half_length(base_length);
        let u = (along / half).clamp(-1.0, 1.0);
        match self {
            Self::Classic | Self::Split => base_face,
            Self::Bumper => base_face + BUMPER_BULGE * (1.0 - u * u),
            Self::Wedge => base_face + WEDGE_TIP * (1.0 - u.abs()),
        }
    }

    /// Outgoing bounce angle in radians relative to `inward` (positive towards `+tangent`).
    pub fn bounce_angle(
        self,
        along: f32,
        incoming_along: f32,
        base_length: f32,
        max_angle: f32,
    ) -> f32 {
        let half = self.half_length(base_length);
        let u = (along / half).clamp(-1.0, 1.0);
        match self {
            Self::Classic | Self::Split => u * max_angle,
            Self::Bumper => {
                (u * max_angle * 1.35).clamp(-MAX_SHAPED_BOUNCE_ANGLE, MAX_SHAPED_BOUNCE_ANGLE)
            }
            Self::Wedge => {
                let side = if along.abs() < 0.5 && incoming_along.abs() > 1e-3 {
                    incoming_along.signum()
                } else if along >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                let min_wedge = max_angle * 0.55;
                let max_wedge = (max_angle * 1.2).min(MAX_SHAPED_BOUNCE_ANGLE);
                side * min_wedge.lerp(max_wedge, u.abs())
            }
        }
    }
}

/// Geometry of one paddle for [`hits_paddle_shape`] and [`bounce_off_paddle_shape`].
#[derive(Clone, Copy, Debug)]
pub struct PaddleGeometry {
    pub centre: Vec2,
    /// Unit vector pointing from the paddle into the arena.
    pub inward: Vec2,
    /// Unit vector pointing along the paddle's length.
    pub tangent: Vec2,
    pub shape: PaddleShape,
    pub base_length: f32,
    pub thickness: f32,
}

/// Whether a ball of width `ball_size` moving from `previous` to `ball` at `velocity` hits the
/// front face of `paddle`.
pub fn hits_paddle_shape(
    paddle: PaddleGeometry,
    previous: Vec2,
    ball: Vec2,
    velocity: Vec2,
    ball_size: f32,
) -> bool {
    let heading_out = velocity.dot(paddle.inward) < 0.0;
    if !heading_out {
        return false;
    }
    let half_ball = ball_size / 2.0;
    let half_len = paddle.shape.half_length(paddle.base_length);
    let along = (ball - paddle.centre).dot(paddle.tangent);
    if along.abs() > half_len + half_ball {
        return false;
    }
    if paddle.shape == PaddleShape::Split && along.abs() < SPLIT_GAP_HALF - half_ball {
        return false;
    }
    let prev_along = (previous - paddle.centre).dot(paddle.tangent);
    let prev_face =
        paddle
            .shape
            .front_face_inward(prev_along, paddle.base_length, paddle.thickness);
    let leading_before = (previous - paddle.centre).dot(paddle.inward) - half_ball;
    let was_in_front = leading_before >= prev_face;

    let face = paddle
        .shape
        .front_face_inward(along, paddle.base_length, paddle.thickness);
    let ball_inward = (ball - paddle.centre).dot(paddle.inward);
    let overlaps_depth =
        ball_inward <= face + half_ball && ball_inward >= -(paddle.thickness / 2.0 + half_ball);
    was_in_front && overlaps_depth
}

/// Outgoing velocity of a ball hitting `paddle` at `ball` with incoming `velocity`, keeping the
/// same speed and aiming according to `paddle.shape` and `max_angle`.
pub fn bounce_off_paddle_shape(
    paddle: PaddleGeometry,
    ball: Vec2,
    velocity: Vec2,
    max_angle: f32,
) -> Vec2 {
    let along = (ball - paddle.centre).dot(paddle.tangent);
    let incoming_along = velocity.dot(paddle.tangent);
    let angle = paddle
        .shape
        .bounce_angle(along, incoming_along, paddle.base_length, max_angle);
    (paddle.inward * angle.cos() + paddle.tangent * angle.sin()) * velocity.length()
}

/// Replicated on every [`Player`]: the [`PaddleShape`] this player picked for Pong and Pong Royale.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerPaddleShape(pub PaddleShape);

/// Client to host: "I'd like to use this [`PaddleShape`] in Pong / Pong Royale".
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaddleShapeRequest(pub PaddleShape);

/// Command: this player picks `shape` for Pong and Pong Royale. The host applies its own pick
/// straight away; a joiner sends a [`PaddleShapeRequest`] over [`ProfileChannel`].
#[derive(Debug, Clone, Copy)]
pub struct PickPaddleShape(pub PaddleShape);

impl Command for PickPaddleShape {
    type Out = ();

    fn apply(self, world: &mut World) {
        let PickPaddleShape(shape) = self;
        if world.run_system_cached(is_host).unwrap_or(false) {
            let mut players = world.query::<(&Player, &mut PlayerPaddleShape)>();
            if let Some((player, mut chosen)) =
                players.iter_mut(world).find(|(player, _)| player.is_host())
                && chosen.0 != shape
            {
                info!(
                    "{} ({:?}) picked paddle shape {:?}",
                    player.name, player.id, shape
                );
                chosen.0 = shape;
            }
            return;
        }
        let mut senders = world.query_filtered::<&mut MessageSender<PaddleShapeRequest>, (
            With<Client>,
            With<Connected>,
        )>();
        match senders.iter_mut(world).next() {
            Some(mut sender) => {
                debug!("Asking the host for paddle shape {shape:?}");
                sender.send::<ProfileChannel>(PaddleShapeRequest(shape));
            }
            None => warn!("Can't pick a paddle shape: not connected"),
        }
    }
}

/// Registers [`PlayerPaddleShape`] and [`PaddleShapeRequest`], and applies joiners' picks on the
/// host.
pub struct PongPaddlePlugin;

impl Plugin for PongPaddlePlugin {
    fn build(&self, app: &mut App) {
        app.component::<PlayerPaddleShape>().replicate();
        app.register_message::<PaddleShapeRequest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.add_systems(Update, apply_paddle_shape_requests.run_if(is_host));
    }
}

/// Host only: apply paddle shape picks sent by joiners.
fn apply_paddle_shape_requests(
    mut links: Query<(Entity, &mut MessageReceiver<PaddleShapeRequest>), With<ClientOf>>,
    mut players: Query<(&Player, &mut PlayerPaddleShape, &ControlledBy)>,
) {
    for (link, mut receiver) in &mut links {
        for PaddleShapeRequest(shape) in receiver.receive() {
            let Some((player, mut chosen, _)) = players
                .iter_mut()
                .find(|(_, _, controlled_by)| controlled_by.owner == link)
            else {
                continue;
            };
            if chosen.0 != shape {
                info!(
                    "{} ({:?}) picked paddle shape {:?}",
                    player.name, player.id, shape
                );
                chosen.0 = shape;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bevy::math::Vec2;

    use super::{PaddleGeometry, PaddleShape, bounce_off_paddle_shape, hits_paddle_shape};

    fn left_paddle(shape: PaddleShape) -> PaddleGeometry {
        PaddleGeometry {
            centre: Vec2::new(-296.0, 0.0),
            inward: Vec2::X,
            tangent: Vec2::Y,
            shape,
            base_length: 48.0,
            thickness: 8.0,
        }
    }

    #[test]
    fn every_paddle_shape_sprite_exists() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        for shape in PaddleShape::ALL {
            assert!(
                assets.join(shape.image()).is_file(),
                "{} is missing",
                shape.image()
            );
        }
    }

    #[test]
    fn bumper_sends_off_centre_hits_wider_than_classic() {
        let classic = left_paddle(PaddleShape::Classic);
        let bumper = left_paddle(PaddleShape::Bumper);
        let max_angle = 37.0_f32.to_radians();
        let hit = Vec2::new(-288.0, 14.0);
        let incoming = Vec2::new(-200.0, 0.0);

        let v_classic = bounce_off_paddle_shape(classic, hit, incoming, max_angle);
        let v_bumper = bounce_off_paddle_shape(bumper, hit, incoming, max_angle);
        assert!(v_bumper.y > v_classic.y, "{v_bumper} vs {v_classic}");
        assert!((v_bumper.length() - incoming.length()).abs() < 1e-3);
    }

    #[test]
    fn wedge_deflects_even_centre_hits_at_a_sharp_angle() {
        let wedge = left_paddle(PaddleShape::Wedge);
        let max_angle = 37.0_f32.to_radians();
        let incoming = Vec2::new(-200.0, 0.0);

        let up = bounce_off_paddle_shape(wedge, Vec2::new(-284.0, 1.0), incoming, max_angle);
        let down = bounce_off_paddle_shape(wedge, Vec2::new(-284.0, -1.0), incoming, max_angle);
        assert!(up.x > 0.0 && up.y > 40.0, "{up}");
        assert!(down.x > 0.0 && down.y < -40.0, "{down}");
    }

    #[test]
    fn split_has_wider_outer_reach_but_lets_dead_centre_balls_through() {
        let split = left_paddle(PaddleShape::Split);
        let classic = left_paddle(PaddleShape::Classic);
        let velocity = Vec2::new(-200.0, 0.0);

        // Dead-centre ball slips through Split's centre gap, while Classic blocks it.
        let centre_ball = Vec2::new(-290.0, 0.0);
        let centre_prev = centre_ball + Vec2::new(4.0, 0.0);
        assert!(hits_paddle_shape(
            classic,
            centre_prev,
            centre_ball,
            velocity,
            8.0
        ));
        assert!(!hits_paddle_shape(
            split,
            centre_prev,
            centre_ball,
            velocity,
            8.0
        ));

        // Wide ball at y = 30 misses Classic (reach 28) but hits Split (reach 32).
        let wide_ball = Vec2::new(-290.0, 30.0);
        let wide_prev = wide_ball + Vec2::new(4.0, 0.0);
        assert!(!hits_paddle_shape(
            classic, wide_prev, wide_ball, velocity, 8.0
        ));
        assert!(hits_paddle_shape(
            split, wide_prev, wide_ball, velocity, 8.0
        ));
    }
}
