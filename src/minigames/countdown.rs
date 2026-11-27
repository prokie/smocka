//! The countdown before a minigame starts: "3", "2", "1", then "GO!", with a beep for each.
//!
//! As a minigame is set up, the host picks the tick play starts on, a countdown ahead
//! ([`StartTick::after`]), and puts it on a replicated entity that lasts while the minigame does.
//! Every peer then holds play back until the tick it simulates reaches it ([`has_started`]). That's
//! judged by the tick, not by when a message arrives, so a rollback replaying ticks from before the
//! start holds still too, and a joiner (whose timeline runs ahead of the host's) starts its own
//! player on the same tick as the host does rather than a round trip late. Each peer shows and
//! beeps the countdown from the same start and its own timeline, so nothing else is sent.
//!
//! A minigame opts in by spawning a [`StartTick`] and not acting on input, or running its clocks,
//! until [`has_started`].

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};

use crate::sound::play_sound;

/// How long a minigame counts down before play starts, unless its rules say otherwise.
pub const START_COUNTDOWN: Duration = Duration::from_secs(3);
/// How long "GO!" stays up after the start.
const GO_SHOWN_FOR: Duration = Duration::from_secs(1);

const COUNTDOWN_SOUND: &str = "minigames/countdown.wav";
const GO_SOUND: &str = "minigames/go.wav";
const VOLUME: f32 = 0.5;
/// The countdown's text size, in screen pixels.
const TEXT_SIZE: f32 = 72.0;

/// The tick play starts on: input counts, and the minigame's clocks run, from it on. One per
/// minigame being played, spawned by the host and replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartTick(pub Tick);

impl StartTick {
    /// The start `countdown` after tick `now`, with ticks `timestep` apart: the tick a timer of
    /// `countdown` started on `now` finishes on.
    pub fn after(now: Tick, countdown: Duration, timestep: Duration) -> Self {
        Self(now + countdown_ticks(countdown, timestep) as i32)
    }

    /// Whether play is on during tick `tick`.
    pub fn has_started(self, tick: Tick) -> bool {
        tick >= self.0
    }
}

/// How many fixed ticks of `timestep` a countdown of `countdown` lasts: the tick its timer
/// finishes on, counting the first tick after it starts as 1. (A countdown of 0 is over at once.)
pub fn countdown_ticks(countdown: Duration, timestep: Duration) -> u32 {
    countdown.as_nanos().div_ceil(timestep.as_nanos()) as u32
}

/// Whether the minigame being played has started on the tick this peer is simulating. False until
/// its [`StartTick`] has arrived. Works as a run condition.
pub fn has_started(timeline: Res<LocalTimeline>, start: Option<Single<&StartTick>>) -> bool {
    start.is_some_and(|start| start.has_started(timeline.tick()))
}

/// What the countdown shows on tick `now`, for play starting on tick `start` with ticks `timestep`
/// apart: the whole seconds left, rounded up ("3", "2", "1"), then "GO!" for a moment, then
/// nothing.
pub fn countdown_label(start: Tick, now: Tick, timestep: Duration) -> String {
    let ticks_left = start - now;
    if ticks_left > 0 {
        let seconds = (timestep.as_secs_f64() * f64::from(ticks_left)).ceil() as u32;
        return seconds.to_string();
    }
    let since_start = timestep * ticks_left.unsigned_abs();
    if since_start < GO_SHOWN_FOR {
        "GO!".into()
    } else {
        String::new()
    }
}

/// A sound for the countdown moving on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountdownSound {
    /// A beep for each number.
    Beep,
    /// Play has started.
    Go,
}

/// The sound for the countdown going from showing `before` to showing `now` (see
/// [`countdown_label`]): a beep for each new number, and "go" for "GO!".
pub fn countdown_sound(before: &str, now: &str) -> Option<CountdownSound> {
    match now {
        _ if now == before => None,
        "" => None,
        "GO!" => Some(CountdownSound::Go),
        _ => Some(CountdownSound::Beep),
    }
}

/// Registers [`StartTick`] for replication. Replicated types must be registered in the same order
/// on every peer: see [`MinigamesPlugin`](super::MinigamesPlugin).
pub struct CountdownPlugin;

impl Plugin for CountdownPlugin {
    fn build(&self, app: &mut App) {
        app.component::<StartTick>().replicate();
    }
}

/// Shows and beeps the countdown on every peer. Added by
/// [`MinigameVisualsPlugin`](super::MinigameVisualsPlugin).
pub struct CountdownVisualsPlugin;

impl Plugin for CountdownVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sounds);
        app.add_systems(Update, show_countdown);
    }
}

/// The countdown in the middle of the screen. Only there while it has something to show, so it
/// never sits over a menu.
#[derive(Component)]
pub struct CountdownText;

#[derive(Resource)]
struct CountdownSounds {
    countdown: Handle<AudioSource>,
    go: Handle<AudioSource>,
}

fn load_sounds(assets: Res<AssetServer>, mut commands: Commands) {
    commands.insert_resource(CountdownSounds {
        countdown: assets.load(COUNTDOWN_SOUND),
        go: assets.load(GO_SOUND),
    });
}

/// Show what the countdown is at, beeping as it changes, and take it away once "GO!" has been up
/// for a moment.
fn show_countdown(
    timeline: Res<LocalTimeline>,
    fixed: Res<Time<Fixed>>,
    start: Option<Single<&StartTick>>,
    mut texts: Query<(&mut Text, &ChildOf), With<CountdownText>>,
    sounds: Res<CountdownSounds>,
    mut commands: Commands,
) {
    let shown = start
        .map(|start| countdown_label(start.0, timeline.tick(), fixed.timestep()))
        .unwrap_or_default();
    let before = match texts.single_mut() {
        Ok((mut text, overlay)) => {
            let before = text.0.clone();
            if shown.is_empty() {
                commands.entity(overlay.parent()).despawn();
            } else if text.0 != shown {
                text.0.clone_from(&shown);
            }
            before
        }
        Err(_) => {
            if !shown.is_empty() {
                spawn_overlay(&shown, &mut commands);
            }
            String::new()
        }
    };
    match countdown_sound(&before, &shown) {
        Some(CountdownSound::Beep) => play_sound(&mut commands, &sounds.countdown, VOLUME),
        Some(CountdownSound::Go) => {
            debug!("Countdown over: GO!");
            play_sound(&mut commands, &sounds.go, VOLUME);
        }
        None => {}
    }
}

fn spawn_overlay(shown: &str, commands: &mut Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        // Never in the way of a click.
        Pickable::IGNORE,
        GlobalZIndex(2),
        children![(
            CountdownText,
            Text::new(shown),
            TextFont {
                font_size: FontSize::Px(TEXT_SIZE),
                ..default()
            },
            TextColor(Color::WHITE),
            Pickable::IGNORE,
        )],
    ));
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lightyear::prelude::Tick;

    use super::{CountdownSound, StartTick, countdown_label, countdown_sound, countdown_ticks};

    const TIMESTEP: Duration = Duration::from_micros(7_812);

    #[test]
    fn a_countdown_lasts_whole_ticks_rounded_up() {
        assert_eq!(countdown_ticks(Duration::ZERO, TIMESTEP), 0);
        assert_eq!(countdown_ticks(TIMESTEP, TIMESTEP), 1);
        assert_eq!(
            countdown_ticks(TIMESTEP + Duration::from_nanos(1), TIMESTEP),
            2
        );
    }

    #[test]
    fn play_starts_on_the_start_tick() {
        let start = StartTick::after(Tick(100), TIMESTEP * 10, TIMESTEP);
        assert_eq!(start, StartTick(Tick(110)));
        assert!(!start.has_started(Tick(109)));
        assert!(start.has_started(Tick(110)));
        assert!(start.has_started(Tick(111)));
        // No countdown: play is on at once.
        assert!(StartTick::after(Tick(100), Duration::ZERO, TIMESTEP).has_started(Tick(100)));
    }

    #[test]
    fn the_label_counts_down_whole_seconds_then_says_go() {
        let step = Duration::from_millis(10);
        let start = Tick(1000);
        let at = |ticks_before: i32| countdown_label(start, start + -ticks_before, step);
        assert_eq!(at(300), "3");
        assert_eq!(at(201), "3");
        assert_eq!(at(200), "2");
        assert_eq!(at(1), "1");
        assert_eq!(at(0), "GO!");
        assert_eq!(at(-99), "GO!");
        assert_eq!(at(-100), "");
    }

    #[test]
    fn beeps_each_number_then_go() {
        let shown = ["", "3", "3", "2", "1", "1", "GO!", "GO!", ""];
        let heard: Vec<Option<CountdownSound>> = shown
            .windows(2)
            .map(|pair| countdown_sound(pair[0], pair[1]))
            .collect();
        use CountdownSound::{Beep, Go};
        assert_eq!(
            heard,
            [
                Some(Beep),
                None,
                Some(Beep),
                Some(Beep),
                None,
                Some(Go),
                None,
                None
            ]
        );
    }
}
