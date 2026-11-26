use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use super::PongSystems;
use super::ball::{BallPosition, BallVelocity, serve_velocity};
use super::paddles::{Paddle, Side};
use crate::config::minigames::two_player_pong::{GOAL_X, GOALS_TO_WIN};
use crate::network::is_host;
use crate::players::Player;
use crate::session::{MinigameEnded, MinigameResult, Placement, SessionPhase};

/// Host only: how a game of Pong is won.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PongRules {
    /// Goals a player needs to win. Defaults to [`GOALS_TO_WIN`].
    pub goals_to_win: u32,
}

impl Default for PongRules {
    fn default() -> Self {
        Self {
            goals_to_win: GOALS_TO_WIN,
        }
    }
}

/// Goals scored by each side. One replicated entity per game, spawned by the host.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PongScore {
    /// Goals scored by the left paddle's player.
    pub left: u32,
    /// Goals scored by the right paddle's player.
    pub right: u32,
}

impl PongScore {
    pub fn of(&self, side: Side) -> u32 {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
        }
    }

    pub fn add_goal(&mut self, side: Side) {
        match side {
            Side::Left => self.left += 1,
            Side::Right => self.right += 1,
        }
    }

    /// The side that has reached `goals_to_win`, if either has.
    pub fn winner(&self, goals_to_win: u32) -> Option<Side> {
        [Side::Left, Side::Right]
            .into_iter()
            .find(|&side| self.of(side) >= goals_to_win)
    }
}

pub struct ScorePlugin;

impl Plugin for ScorePlugin {
    fn build(&self, app: &mut App) {
        app.component::<PongScore>().replicate();
        app.init_resource::<PongRules>();

        app.add_systems(
            OnEnter(SessionPhase::TwoPlayerPong),
            spawn_score.run_if(is_host),
        );
        app.add_systems(
            FixedUpdate,
            score_goals
                .in_set(PongSystems::Score)
                .run_if(is_host)
                .run_if(in_state(SessionPhase::TwoPlayerPong)),
        );
        app.add_systems(
            Update,
            abandon_without_both_players
                .run_if(is_host)
                .run_if(in_state(SessionPhase::TwoPlayerPong)),
        );
    }
}

fn spawn_score(mut commands: Commands) {
    commands.spawn((
        PongScore::default(),
        DespawnOnExit(SessionPhase::TwoPlayerPong),
        Replicate::to_clients(NetworkTarget::All),
    ));
}

fn score_goals(
    mut balls: Query<(&mut BallPosition, &mut BallVelocity)>,
    mut score: Single<&mut PongScore>,
    paddles: Query<&Paddle>,
    players: Query<&Player>,
    rules: Res<PongRules>,
    mut commands: Commands,
) {
    // The phase only changes at the start of the next frame, so more ticks can run after the
    // winning goal. The ball is left where it went out, so don't count it again.
    if score.winner(rules.goals_to_win).is_some() {
        return;
    }
    for (mut position, mut velocity) in &mut balls {
        let Some(scorer) = scorer(position.0) else {
            continue;
        };
        score.add_goal(scorer);
        debug!("Goal for {scorer:?}: {} – {}", score.left, score.right);

        if score.winner(rules.goals_to_win).is_none() {
            position.0 = Vec2::ZERO;
            velocity.0 = serve_velocity(scorer.opponent(), &mut rand::rng());
            continue;
        }
        let player_on = |side| {
            let paddle = paddles.iter().find(|paddle| paddle.side == side)?;
            players.iter().find(|player| player.id == paddle.owner)
        };
        commands.trigger(match (player_on(scorer), player_on(scorer.opponent())) {
            (Some(winner), Some(loser)) => MinigameEnded::Won(MinigameResult {
                placements: vec![Placement::new(winner, 1), Placement::new(loser, 2)],
                summary: format!("{} - {}", score.left, score.right),
            }),
            _ => {
                info!("Abandoning Pong: a player left as the winning goal went in");
                MinigameEnded::Abandoned
            }
        });
    }
}

fn abandon_without_both_players(paddles: Query<(), With<Paddle>>, mut commands: Commands) {
    let paddles = paddles.iter().len();
    if paddles < 2 {
        info!("Abandoning Pong: a paddle's player left ({paddles} of 2 paddles remain)");
        commands.trigger(MinigameEnded::Abandoned);
    }
}

/// The side that scores with the ball at `position`, once it is fully past an edge.
pub fn scorer(position: Vec2) -> Option<Side> {
    if position.x < -GOAL_X {
        Some(Side::Right)
    } else if position.x > GOAL_X {
        Some(Side::Left)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{PongScore, scorer};
    use crate::config::minigames::two_player_pong::GOAL_X;
    use crate::minigames::two_player_pong::paddles::Side;
    use bevy::math::Vec2;

    #[test]
    fn off_the_left_edge_scores_for_the_right() {
        assert_eq!(scorer(Vec2::new(-GOAL_X - 1.0, 0.0)), Some(Side::Right));
    }

    #[test]
    fn off_the_right_edge_scores_for_the_left() {
        assert_eq!(scorer(Vec2::new(GOAL_X + 1.0, 50.0)), Some(Side::Left));
    }

    #[test]
    fn a_ball_still_partly_on_screen_does_not_score() {
        assert_eq!(scorer(Vec2::new(GOAL_X - 1.0, 0.0)), None);
        assert_eq!(scorer(Vec2::new(-GOAL_X + 1.0, 0.0)), None);
    }

    #[test]
    fn adds_goals_to_the_right_side() {
        let mut score = PongScore::default();
        score.add_goal(Side::Left);
        score.add_goal(Side::Left);
        score.add_goal(Side::Right);
        assert_eq!((score.of(Side::Left), score.of(Side::Right)), (2, 1));
    }

    #[test]
    fn nobody_wins_below_the_goal_count() {
        let score = PongScore { left: 2, right: 2 };
        assert_eq!(score.winner(3), None);
    }

    #[test]
    fn the_side_that_reaches_the_goal_count_wins() {
        assert_eq!(PongScore { left: 3, right: 1 }.winner(3), Some(Side::Left));
        assert_eq!(PongScore { left: 0, right: 3 }.winner(3), Some(Side::Right));
    }
}
