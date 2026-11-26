use std::time::Duration;

use bevy::prelude::*;

use super::widgets::label;
use crate::app_state::AppState;
use crate::lobby::PlayerLeft;

pub const NOTICE_DURATION: Duration = Duration::from_secs(4);
const NOTICE_BACKGROUND: Color = Color::srgba(0.1, 0.1, 0.12, 0.85);

/// Where notices (such as "Player 2 left.") stack up, newest last.
#[derive(Component)]
pub struct NoticeArea;

/// A notice on screen, and how long it has left.
#[derive(Component)]
pub struct Notice(Timer);

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
