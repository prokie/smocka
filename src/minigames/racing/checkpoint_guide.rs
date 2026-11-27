//! Shows a driver where to go: a pulsing strip of chevrons in their car's colour lies across the
//! road at the checkpoint their car needs next, pointing the way to drive. If their car goes
//! through a checkpoint out of order (cutting across the grass, or turning the wrong way at the
//! crossing), a message says so and an arrow by the car points to the checkpoint it still needs.
//!
//! Local to each peer, for its own car only: worked out from that car's replicated
//! [`RaceProgress`] and where this peer sees it, so nothing extra is sent. Spectators see none of
//! it.

use std::f32::consts::TAU;

use bevy::prelude::*;
use lightyear::prelude::input::native::InputMarker;

use super::PHASE;
use super::RacingInput;
use super::car::{Car, CarState};
use super::progress::RaceProgress;
use super::track::TrackId;
use super::visuals::sync_car_transform;
use crate::players::player_color;

/// Points right (+x). Drawn white with a dark outline, and tinted with the player's colour.
const CHEVRON_IMAGE: &str = "minigames/racing/chevron.png";
/// `chevron.png`'s size in world units: along the way it points, then across.
const CHEVRON_SIZE: Vec2 = Vec2::new(10.0, 12.0);
/// How many chevrons the marker has, spread across the road.
const MARKER_CHEVRONS: usize = 3;
/// Above the track and the skid marks, below the cars.
const MARKER_Z: f32 = 0.6;
/// How often the marker pulses, per second.
const PULSE_PER_SECOND: f32 = 1.5;
/// How see-through the marker gets, at the faintest and the strongest of a pulse.
const MARKER_ALPHA: (f32, f32) = (0.35, 0.85);

/// How far from the car's centre the arrow sits, towards the checkpoint it needs.
const ARROW_DISTANCE: f32 = 22.0;
/// The arrow is a chevron this many times its size.
const ARROW_SCALE: f32 = 1.3;
/// Above the cars.
const ARROW_Z: f32 = 2.0;

const MESSAGE: &str = "Missed a checkpoint! Follow the arrow";
/// In screen pixels: the message's size, and how far below the top of the screen it is.
const MESSAGE_SIZE: f32 = 24.0;
const MESSAGE_TOP: f32 = 64.0;
const MESSAGE_COLOR: Color = Color::srgb(1.0, 0.82, 0.3);

/// The strip across the road at this player's next checkpoint. Its children are the chevrons.
#[derive(Component)]
pub struct NextCheckpointMarker;

/// One chevron of the [`NextCheckpointMarker`].
#[derive(Component)]
struct MarkerChevron(usize);

/// Points from this player's car to the checkpoint it needs, after it missed one.
#[derive(Component)]
pub struct MissedCheckpointArrow;

/// Says this player's car missed a checkpoint.
#[derive(Component)]
pub struct MissedCheckpointText;

/// What this peer has seen of its own car's way round the track.
#[derive(Resource, Debug, Default, PartialEq, Eq)]
pub struct CheckpointGuide {
    /// The checkpoint the car needs next, as last seen.
    next: Option<usize>,
    /// The checkpoint the car is in, if any, so entering one is noticed once.
    inside: Option<usize>,
    /// Whether the car went through a checkpoint out of order since it last reached the one it
    /// needed.
    pub missed: bool,
}

/// Added by [`RacingVisualsPlugin`](super::RacingVisualsPlugin).
pub struct CheckpointGuidePlugin;

impl Plugin for CheckpointGuidePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CheckpointGuide>();
        app.add_systems(OnEnter(PHASE), spawn_guide);
        app.add_systems(Update, follow_own_car.run_if(in_state(PHASE)));
        app.add_systems(
            PostUpdate,
            point_arrow
                .after(sync_car_transform)
                .before(TransformSystems::Propagate)
                .run_if(in_state(PHASE)),
        );
    }
}

/// Whether a car that needs checkpoint `next` (of `count`) went off course by entering
/// `entered`. The checkpoint it just reached doesn't count: the car is often still in it when
/// the host's word that it counted arrives.
pub fn out_of_order(entered: usize, next: usize, count: usize) -> bool {
    let just_reached = (next + count - 1) % count;
    entered != next && entered != just_reached
}

fn spawn_guide(
    assets: Res<AssetServer>,
    mut guide: ResMut<CheckpointGuide>,
    mut commands: Commands,
) {
    *guide = CheckpointGuide::default();
    let chevron = assets.load(CHEVRON_IMAGE);
    commands
        .spawn((
            NextCheckpointMarker,
            DespawnOnExit(PHASE),
            Transform::from_xyz(0.0, 0.0, MARKER_Z),
            Visibility::Hidden,
        ))
        .with_children(|marker| {
            for index in 0..MARKER_CHEVRONS {
                marker.spawn((
                    MarkerChevron(index),
                    Sprite {
                        custom_size: Some(CHEVRON_SIZE),
                        ..Sprite::from_image(chevron.clone())
                    },
                    Transform::default(),
                ));
            }
        });
    commands.spawn((
        MissedCheckpointArrow,
        DespawnOnExit(PHASE),
        Sprite {
            custom_size: Some(CHEVRON_SIZE * ARROW_SCALE),
            ..Sprite::from_image(chevron)
        },
        Transform::from_xyz(0.0, 0.0, ARROW_Z),
        Visibility::Hidden,
    ));
    commands.spawn((
        DespawnOnExit(PHASE),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: px(MESSAGE_TOP),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            MissedCheckpointText,
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(MESSAGE_SIZE),
                ..default()
            },
            TextColor(MESSAGE_COLOR),
        )],
    ));
}

/// The arrow's parts that [`point_arrow`] sets.
type ArrowParts = (
    &'static mut Transform,
    &'static mut Visibility,
    &'static mut Sprite,
);

/// This player's car: its driver, where it is, and how far round it has got.
type OwnCar = (&'static Car, &'static CarState, &'static RaceProgress);

/// Put the marker on this player's next checkpoint, notice when their car misses one, and show
/// or clear the message. Everything hides when there's no car of theirs still racing, or until
/// the race's track is known.
fn follow_own_car(
    time: Res<Time>,
    track: Option<Single<&TrackId>>,
    own_car: Query<OwnCar, With<InputMarker<RacingInput>>>,
    mut guide: ResMut<CheckpointGuide>,
    marker: Single<(&mut Transform, &mut Visibility), With<NextCheckpointMarker>>,
    mut chevrons: Query<
        (&MarkerChevron, &mut Sprite, &mut Transform),
        Without<NextCheckpointMarker>,
    >,
    mut message: Single<&mut Text, With<MissedCheckpointText>>,
) {
    let (mut marker_transform, mut marker_visibility) = marker.into_inner();
    let racing = track
        .map(|id| id.track())
        .zip(own_car.single().ok())
        .filter(|(_, (_, _, progress))| progress.place.is_none());
    let Some((track, (car, state, progress))) = racing else {
        marker_visibility.set_if_neq(Visibility::Hidden);
        guide.missed = false;
        if !message.0.is_empty() {
            message.0.clear();
        }
        return;
    };

    let count = track.checkpoints().len();
    let next = progress.next_checkpoint;
    if guide.next != Some(next) {
        // Reached the checkpoint it needed (or the race just began): back on course.
        guide.next = Some(next);
        guide.missed = false;
    }
    let inside = track.checkpoint_at(state.position);
    if inside != guide.inside {
        guide.inside = inside;
        if let Some(entered) = inside
            && out_of_order(entered, next, count)
            && !guide.missed
        {
            debug!(
                "{} ({:?}) missed checkpoint {next}: went through checkpoint {entered} instead",
                car.name, car.owner
            );
            guide.missed = true;
        }
    }

    let checkpoint = track.checkpoints()[next];
    marker_transform.translation = checkpoint.center().extend(MARKER_Z);
    marker_transform.rotation = Quat::from_rotation_z(checkpoint.heading());
    marker_visibility.set_if_neq(Visibility::Inherited);
    let pulse = (time.elapsed_secs() * PULSE_PER_SECOND * TAU).sin() * 0.5 + 0.5;
    let alpha = MARKER_ALPHA.0 + (MARKER_ALPHA.1 - MARKER_ALPHA.0) * pulse;
    let color = player_color(car.color).with_alpha(alpha);
    // Spread evenly across the road, a third of its width apart.
    let spacing = 2.0 * track.layout().road_half_width / MARKER_CHEVRONS as f32;
    for (MarkerChevron(index), mut sprite, mut transform) in &mut chevrons {
        let from_middle = *index as f32 - (MARKER_CHEVRONS - 1) as f32 / 2.0;
        transform.translation = Vec3::new(0.0, from_middle * spacing, 0.0);
        sprite.color = color;
    }

    let shown = if guide.missed { MESSAGE } else { "" };
    if message.0 != shown {
        message.0 = shown.into();
    }
}

/// While this player's car has missed a checkpoint, point the arrow from where the car is drawn
/// to the checkpoint it needs. Runs once the car's drawn position is set for this frame, so the
/// arrow keeps up with it.
fn point_arrow(
    guide: Res<CheckpointGuide>,
    track: Option<Single<&TrackId>>,
    own_car: Query<(&Car, &RaceProgress, &Transform), With<InputMarker<RacingInput>>>,
    arrow: Single<ArrowParts, (With<MissedCheckpointArrow>, Without<Car>)>,
) {
    let (mut transform, mut visibility, mut sprite) = arrow.into_inner();
    let shown = track
        .map(|id| id.track())
        .zip(own_car.single().ok())
        .filter(|_| guide.missed);
    let Some((track, (car, progress, car_transform))) = shown else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let from = car_transform.translation.truncate();
    let to = track.checkpoints()[progress.next_checkpoint].center();
    let direction = (to - from).normalize_or(Vec2::X);
    transform.translation = (from + direction * ARROW_DISTANCE).extend(ARROW_Z);
    transform.rotation = Quat::from_rotation_z(direction.to_angle());
    sprite.color = player_color(car.color);
    visibility.set_if_neq(Visibility::Inherited);
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use lightyear::prelude::PeerId;
    use lightyear::prelude::input::native::InputMarker;

    use super::{
        ARROW_DISTANCE, CheckpointGuide, MARKER_CHEVRONS, MESSAGE, MarkerChevron,
        MissedCheckpointArrow, MissedCheckpointText, NextCheckpointMarker, follow_own_car,
        out_of_order, point_arrow,
    };
    use crate::minigames::racing::RacingInput;
    use crate::minigames::racing::car::{Car, CarState};
    use crate::minigames::racing::progress::RaceProgress;
    use crate::minigames::racing::track::{Track, TrackId};

    const TRACK: TrackId = TrackId::FigureEight;

    fn track() -> &'static Track {
        TRACK.track()
    }

    /// A race on [`TRACK`] with this player's car at `position`, with `progress`, and the guide's
    /// marker, chevrons and message as `spawn_guide` makes them.
    fn race(progress: RaceProgress, position: Vec2) -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.init_resource::<CheckpointGuide>();
        world.spawn(TRACK);
        let car = world
            .spawn((
                Car {
                    owner: PeerId::Netcode(1),
                    name: "Ada".into(),
                    color: 0,
                },
                CarState::parked(position, 0.0),
                progress,
                InputMarker::<RacingInput>::default(),
                Transform::from_translation(position.extend(1.0)),
            ))
            .id();
        world.spawn((
            NextCheckpointMarker,
            Transform::default(),
            Visibility::Hidden,
        ));
        for index in 0..MARKER_CHEVRONS {
            world.spawn((
                MarkerChevron(index),
                Sprite::default(),
                Transform::default(),
            ));
        }
        world.spawn((MissedCheckpointText, Text::new("")));
        world.spawn((
            MissedCheckpointArrow,
            Sprite::default(),
            Transform::default(),
            Visibility::Hidden,
        ));
        (world, car)
    }

    fn follow(world: &mut World) {
        world.run_system_once(follow_own_car).unwrap();
    }

    fn marker(world: &mut World) -> (Transform, Visibility) {
        let (transform, visibility) = world
            .query_filtered::<(&Transform, &Visibility), With<NextCheckpointMarker>>()
            .single(world)
            .unwrap();
        (*transform, *visibility)
    }

    fn message(world: &mut World) -> String {
        world
            .query_filtered::<&Text, With<MissedCheckpointText>>()
            .single(world)
            .unwrap()
            .0
            .clone()
    }

    fn move_car(world: &mut World, car: Entity, position: Vec2) {
        *world.get_mut::<CarState>(car).unwrap() = CarState::parked(position, 0.0);
        world.get_mut::<Transform>(car).unwrap().translation = position.extend(1.0);
    }

    #[test]
    fn the_marker_lies_across_the_road_at_the_next_checkpoint() {
        let (mut world, _) = race(RaceProgress::on_grid(), track().grid_position(0));
        follow(&mut world);

        let next = track().checkpoints()[1];
        let (transform, visibility) = marker(&mut world);
        assert_eq!(visibility, Visibility::Inherited);
        assert!(transform.translation.truncate().distance(next.center()) < 1e-3);
        // The chevrons point right (+x) before turning.
        let pointing = (transform.rotation * Vec3::X).truncate();
        assert!(pointing.angle_to(Vec2::from_angle(next.heading())).abs() < 1e-3);
        // Spread across the road, the middle one in the middle.
        let mut across: Vec<f32> = world
            .query_filtered::<&Transform, With<MarkerChevron>>()
            .iter(&world)
            .map(|transform| transform.translation.y)
            .collect();
        across.sort_by(f32::total_cmp);
        assert_eq!(across, [-16.0, 0.0, 16.0]);
        assert_eq!(message(&mut world), "");
    }

    #[test]
    fn crossing_the_line_off_the_grid_isnt_a_miss() {
        let (mut world, car) = race(RaceProgress::on_grid(), track().grid_position(0));
        follow(&mut world);
        move_car(&mut world, car, track().checkpoints()[0].center());
        follow(&mut world);
        assert!(!world.resource::<CheckpointGuide>().missed);
        assert_eq!(message(&mut world), "");
    }

    #[test]
    fn a_missed_checkpoint_is_shown_until_the_car_reaches_it() {
        let (mut world, car) = race(RaceProgress::on_grid(), track().grid_position(0));
        follow(&mut world);

        // Needs checkpoint 1, but went through 3.
        move_car(&mut world, car, track().checkpoints()[3].center());
        follow(&mut world);
        assert!(world.resource::<CheckpointGuide>().missed);
        assert_eq!(message(&mut world), MESSAGE);

        // Still missed after leaving it.
        move_car(&mut world, car, Vec2::new(150.0, 0.0));
        follow(&mut world);
        assert_eq!(message(&mut world), MESSAGE);

        // The host counts checkpoint 1: back on course.
        world.get_mut::<RaceProgress>(car).unwrap().next_checkpoint = 2;
        follow(&mut world);
        assert!(!world.resource::<CheckpointGuide>().missed);
        assert_eq!(message(&mut world), "");
    }

    #[test]
    fn nothing_shows_once_the_car_has_finished() {
        let finished = RaceProgress {
            place: Some(1),
            ..RaceProgress::on_grid()
        };
        let (mut world, _) = race(finished, track().checkpoints()[3].center());
        follow(&mut world);
        assert_eq!(marker(&mut world).1, Visibility::Hidden);
        assert_eq!(message(&mut world), "");
    }

    #[test]
    fn nothing_shows_for_a_spectator() {
        let (mut world, car) = race(RaceProgress::on_grid(), track().grid_position(0));
        world.entity_mut(car).remove::<InputMarker<RacingInput>>();
        follow(&mut world);
        assert_eq!(marker(&mut world).1, Visibility::Hidden);
    }

    #[test]
    fn the_arrow_points_from_the_car_to_the_checkpoint_it_needs_only_after_a_miss() {
        let from = Vec2::new(150.0, 0.0);
        let (mut world, _) = race(RaceProgress::on_grid(), from);
        let arrow = |world: &mut World| {
            world.run_system_once(point_arrow).unwrap();
            let (transform, visibility) = world
                .query_filtered::<(&Transform, &Visibility), With<MissedCheckpointArrow>>()
                .single(world)
                .unwrap();
            (*transform, *visibility)
        };
        assert_eq!(arrow(&mut world).1, Visibility::Hidden);

        world.resource_mut::<CheckpointGuide>().missed = true;
        let (transform, visibility) = arrow(&mut world);
        assert_eq!(visibility, Visibility::Inherited);
        let towards = (track().checkpoints()[1].center() - from).normalize();
        let expected = from + towards * ARROW_DISTANCE;
        assert!(transform.translation.truncate().distance(expected) < 1e-3);
    }

    #[test]
    fn the_checkpoint_needed_is_in_order() {
        assert!(!out_of_order(3, 3, 7));
    }

    #[test]
    fn the_one_just_reached_is_in_order() {
        assert!(!out_of_order(2, 3, 7));
        // At the start the car needs checkpoint 1, and crosses the line (0) first.
        assert!(!out_of_order(0, 1, 7));
        // Needing the line again, it may still be in the last checkpoint.
        assert!(!out_of_order(6, 0, 7));
    }

    #[test]
    fn skipping_ahead_or_going_back_is_out_of_order() {
        assert!(out_of_order(4, 2, 7));
        assert!(out_of_order(1, 3, 7));
        assert!(out_of_order(0, 2, 7));
    }
}
