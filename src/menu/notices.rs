//! Short messages that pop up during a session, such as "Player 2 left.", and go away by
//! themselves after [`NOTICE_DURATION`].

use std::time::Duration;

use bevy::prelude::*;

use super::label;
use crate::app_state::AppState;
use crate::lobby::PlayerLeft;

/// How long a notice stays on screen.
pub const NOTICE_DURATION: Duration = Duration::from_secs(4);
/// Background of a notice.
const NOTICE_BACKGROUND: Color = Color::srgba(0.1, 0.1, 0.12, 0.85);

/// The column near the bottom of the screen that notices are stacked in, newest last.
#[derive(Component)]
pub struct NoticeArea;

/// A notice on screen, and how long it has left.
#[derive(Component)]
pub struct Notice(Timer);

/// When a session starts, make room for its notices.
pub(super) fn spawn_notice_area(mut commands: Commands) {
    commands.spawn((
        NoticeArea,
        DespawnOnExit(AppState::InSession),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(40),
            width: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(8),
            ..default()
        },
    ));
}

/// When another player leaves, say so.
pub(super) fn notice_player_left(
    trigger: On<PlayerLeft>,
    area: Query<Entity, With<NoticeArea>>,
    mut commands: Commands,
) {
    let PlayerLeft(player) = trigger.event();
    for area in &area {
        commands
            .entity(area)
            .with_child(notice(format!("{} left.", player.name())));
    }
}

/// A notice saying `text`.
fn notice(text: String) -> impl Bundle {
    (
        Notice(Timer::new(NOTICE_DURATION, TimerMode::Once)),
        Node {
            padding: UiRect::axes(px(16), px(8)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(NOTICE_BACKGROUND),
        children![label(text, 20.0, Color::WHITE)],
    )
}

/// Remove notices that have been up for [`NOTICE_DURATION`].
pub(super) fn expire_notices(
    time: Res<Time>,
    mut notices: Query<(Entity, &mut Notice)>,
    mut commands: Commands,
) {
    for (entity, mut notice) in &mut notices {
        if notice.0.tick(time.delta()).is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
