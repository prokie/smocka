//! A game of Pong Royale, as the host runs it: setting it up, the ball, lives and walls, and the
//! end. Every peer sees it through the replicated [`Paddle`]s, [`Lives`], [`Wall`]s and ball.

use bevy::prelude::*;
use lightyear::prelude::*;
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};

use super::arena::{Side, Sides, seats, serve_velocity, step_ball};
use super::paddles::{Lives, Paddle, PaddlePosition, spawn_paddle};
use super::{LIVES_SETTING, PHASE, RoyaleRules, RoyaleSystems};
use crate::minigames::countdown::StartTick;
use crate::minigames::outcome::{MinigameEnded, MinigameResult, Participants, Placement};
use crate::minigames::settings::MinigameSettings;
use crate::network::is_host;
use crate::players::{Player, local_peer_id};

/// Marks the ball.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Ball;

/// Where the ball is. Interpolated on joiners.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BallPosition(pub Vec2);

impl Ease for BallPosition {
    fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
        FunctionCurve::new(Interval::UNIT, move |t| {
            BallPosition(start.0.lerp(end.0, t))
        })
    }
}

/// The ball's velocity. Host only: joiners only need [`BallPosition`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BallVelocity(pub Vec2);

/// A wall across a side's goal, that the ball bounces off: a side nobody sat at, or whose player is
/// out of lives or left.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wall(pub Side);

/// A player who went out, as they were when they did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Out {
    pub owner: PeerId,
    pub name: String,
    pub color: u8,
}

impl From<&Paddle> for Out {
    fn from(paddle: &Paddle) -> Self {
        Self {
            owner: paddle.owner,
            name: paddle.name.clone(),
            color: paddle.color,
        }
    }
}

/// Host only: who's out so far, first out first. Exists while playing.
#[derive(Resource, Debug, Default)]
pub struct RoyaleDirector {
    out: Vec<Out>,
    /// Set once the game has handed control back to the session, so it only does that once.
    over: bool,
}

/// Registers the ball and walls, and runs the game on the host.
pub struct PlayPlugin;

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        app.component::<Ball>().replicate();
        app.component::<BallPosition>()
            .replicate()
            .add_linear_interpolation();
        app.component::<Wall>().replicate();

        app.add_systems(
            OnEnter(PHASE),
            (start_game, count_down_to_the_start).run_if(is_host),
        );
        app.add_systems(OnExit(PHASE), |mut commands: Commands| {
            commands.remove_resource::<RoyaleDirector>();
        });
        app.add_systems(
            FixedUpdate,
            (
                move_ball.in_set(RoyaleSystems::MoveBall),
                (wall_off_empty_sides, end_game)
                    .chain()
                    .in_set(RoyaleSystems::Finish),
            )
                .run_if(is_host)
                .run_if(resource_exists::<RoyaleDirector>),
        );
    }
}

/// Host only: seat the [`Participants`] still here, a side each (see [`seats`]), wall off the
/// other sides, and serve the ball to one of them.
fn start_game(
    settings: Res<MinigameSettings>,
    participants: Single<&Participants>,
    players: Query<(&Player, &ControlledBy)>,
    host: Query<&LocalId, (With<Client>, With<Connected>)>,
    mut commands: Commands,
) {
    let host = local_peer_id(&host);
    let lives = settings.get(LIVES_SETTING);
    let here: Vec<_> = participants
        .0
        .iter()
        .filter_map(|id| players.iter().find(|(player, _)| player.id == *id))
        .collect();
    let sides = seats(here.len());
    for ((player, link), &side) in here.iter().zip(sides) {
        spawn_paddle(&mut commands, player, link.owner, side, lives, host);
    }
    for side in Side::ALL {
        if !sides.contains(&side) {
            commands.spawn((
                Wall(side),
                DespawnOnExit(PHASE),
                Replicate::to_clients(NetworkTarget::All),
            ));
        }
    }

    let mut rng = rand::rng();
    let first = sides.choose(&mut rng).copied().unwrap_or(Side::Left);
    let velocity = serve_velocity(first, &mut rng);
    commands.spawn((
        Ball,
        BallPosition(Vec2::ZERO),
        BallVelocity(velocity),
        DespawnOnExit(PHASE),
        Replicate::to_clients(NetworkTarget::All),
        InterpolationTarget::to_clients(NetworkTarget::from_exclude(host)),
    ));

    commands.insert_resource(RoyaleDirector::default());

    let seated: Vec<String> = here
        .iter()
        .zip(sides)
        .map(|((player, _), side)| format!("{} ({side:?})", player.name()))
        .collect();
    info!(
        "Pong Royale with {lives} lives each: {}. First serve to {first:?}",
        seated.join(", "),
    );
}

/// Host only: count down to the start (see [`countdown`](crate::minigames::countdown)).
fn count_down_to_the_start(
    rules: Res<RoyaleRules>,
    timeline: Res<LocalTimeline>,
    fixed: Res<Time<Fixed>>,
    mut commands: Commands,
) {
    let start = StartTick::after(timeline.tick(), rules.countdown, fixed.timestep());
    info!(
        "Pong Royale starts in {} s, on tick {}",
        rules.countdown.as_secs_f32().ceil() as u32,
        start.0.0
    );
    commands.spawn((
        start,
        DespawnOnExit(PHASE),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

/// The paddles and walls the ball can meet this tick.
fn sides(paddles: &Query<PaddleState>, walls: &Query<&Wall>) -> Sides {
    let with_paddles = paddles
        .iter()
        .fold(Sides::default(), |sides, (_, paddle, position, _)| {
            sides.with_paddle(paddle.side, position.0)
        });
    walls
        .iter()
        .fold(with_paddles, |sides, wall| sides.with_wall(wall.0))
}

type PaddleState = (
    Entity,
    &'static Paddle,
    &'static PaddlePosition,
    &'static mut Lives,
);

/// Host only: move the ball. When it goes into a goal, that side's player loses a life, or at
/// their last, goes out and a wall goes up in their place. Then the ball is served again from the
/// centre, to them if they're still in, or else to someone who is.
fn move_ball(
    time: Res<Time>,
    mut balls: Query<(&mut BallPosition, &mut BallVelocity), With<Ball>>,
    mut paddles: Query<PaddleState>,
    walls: Query<&Wall>,
    mut director: ResMut<RoyaleDirector>,
    mut commands: Commands,
) {
    if director.over {
        return;
    }
    let sides = sides(&paddles, &walls);
    for (mut position, mut velocity) in &mut balls {
        let step = step_ball(position.0, velocity.0, &sides, time.delta_secs());
        position.0 = step.position;
        velocity.0 = step.velocity;
        let Some(side) = step.goal else {
            continue;
        };

        let mut still_in = None;
        if let Some((entity, paddle, _, mut lives)) = paddles
            .iter_mut()
            .find(|(_, paddle, _, _)| paddle.side == side)
        {
            lives.0 = lives.0.saturating_sub(1);
            if lives.0 > 0 {
                debug!(
                    "{} ({:?}) let the ball in on the {side:?}: {} lives left",
                    paddle.name, paddle.owner, lives.0
                );
                still_in = Some(side);
            } else {
                info!(
                    "{} ({:?}) is out: the {side:?} side is walled off",
                    paddle.name, paddle.owner
                );
                director.out.push(Out::from(paddle));
                commands.entity(entity).despawn();
                commands.spawn((
                    Wall(side),
                    DespawnOnExit(PHASE),
                    Replicate::to_clients(NetworkTarget::All),
                ));
            }
        }

        let mut rng = rand::rng();
        let remaining: Vec<Side> = paddles
            .iter()
            .filter(|(_, _, _, lives)| lives.0 > 0)
            .map(|(_, paddle, _, _)| paddle.side)
            .collect();
        position.0 = Vec2::ZERO;
        velocity.0 = match still_in.or_else(|| remaining.choose(&mut rng).copied()) {
            Some(to) => serve_velocity(to, &mut rng),
            None => Vec2::ZERO,
        };
    }
}

/// Host only: wall off any side whose paddle has gone without its player going out: they left.
fn wall_off_empty_sides(paddles: Query<&Paddle>, walls: Query<&Wall>, mut commands: Commands) {
    for side in Side::ALL {
        let defended = paddles.iter().any(|paddle| paddle.side == side);
        let walled = walls.iter().any(|wall| wall.0 == side);
        if !defended && !walled {
            info!("The {side:?} side's player left: walling it off");
            commands.spawn((
                Wall(side),
                DespawnOnExit(PHASE),
                Replicate::to_clients(NetworkTarget::All),
            ));
        }
    }
}

/// Host only: once at most one paddle is left, the game is over. Its player wins, and the others
/// place in the order they went out, the last out second. If nobody went out (everyone else left),
/// it's abandoned.
fn end_game(paddles: Query<&Paddle>, mut director: ResMut<RoyaleDirector>, mut commands: Commands) {
    if director.over || paddles.iter().len() >= 2 {
        return;
    }
    director.over = true;
    let survivor = paddles.iter().next().map(Out::from);
    if director.out.is_empty() {
        match &survivor {
            Some(survivor) => info!(
                "Abandoning Pong Royale: every other player left, leaving {} ({:?}) alone",
                survivor.name, survivor.owner
            ),
            None => info!("Abandoning Pong Royale: every player has left"),
        }
        commands.trigger(MinigameEnded::Abandoned);
        return;
    }
    let placements = royale_placements(survivor.as_ref(), &director.out);
    info!(
        "Pong Royale over: {}",
        placements
            .iter()
            .map(Placement::describe)
            .collect::<Vec<_>>()
            .join(", ")
    );
    commands.trigger(MinigameEnded::Won(MinigameResult {
        placements,
        summary: "Last one standing".into(),
    }));
}

/// The result: the survivor (if they're still here) first, then everyone who went out, the last
/// out first.
pub fn royale_placements(survivor: Option<&Out>, out: &[Out]) -> Vec<Placement> {
    survivor
        .into_iter()
        .chain(out.iter().rev())
        .zip(1..)
        .map(|(player, place)| Placement {
            player: player.owner,
            name: player.name.clone(),
            color: player.color,
            place,
            note: String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use lightyear::prelude::PeerId;

    use super::{Out, royale_placements};

    fn out(number: u64) -> Out {
        Out {
            owner: PeerId::Netcode(number),
            name: format!("Player {number}"),
            color: number as u8,
        }
    }

    #[test]
    fn the_survivor_wins_and_the_rest_place_by_who_lasted_longest() {
        let gone = [out(3), out(1), out(4)];
        let placements = royale_placements(Some(&out(2)), &gone);
        let shown: Vec<String> = placements.iter().map(|p| p.describe()).collect();
        assert_eq!(
            shown,
            [
                "1st Player 2",
                "2nd Player 4",
                "3rd Player 1",
                "4th Player 3"
            ]
        );
    }

    #[test]
    fn without_a_survivor_the_last_out_wins() {
        let placements = royale_placements(None, &[out(1), out(2)]);
        assert_eq!(placements[0].player, PeerId::Netcode(2));
        assert_eq!(placements[0].place, 1);
    }
}
