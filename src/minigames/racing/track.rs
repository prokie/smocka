//! The tracks a race can be on. Each is constant data, the same on every peer: only which one a
//! race uses ([`TrackId`]) is replicated.
//!
//! A track is a [`Layout`]: a centre line built from [`Piece`]s, with the road reaching
//! [`Layout::road_half_width`] either side of it, and art drawn to match
//! (`assets/minigames/racing/<track>.png`, from `tools/make_art.py`). Its [`BoostPad`]s aren't in
//! the art: the game draws them where the layout puts them.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::LazyLock;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::config::{
    BOOST_MAX_SPEED, BOOST_PAD_LENGTH, BOOST_PAD_WIDTH, CHECKPOINT_DEPTH, CHECKPOINT_MARGIN,
    GRASS_MAX_SPEED, GRID_LANE_OFFSET, GRID_ROW_SPACING,
};
use crate::config::network::TICK_RATE;

// A car at its fastest must land inside each checkpoint and boost pad on at least one tick, with
// room to spare.
const _: () = assert!(2.0 * BOOST_MAX_SPEED as f64 / TICK_RATE < CHECKPOINT_DEPTH as f64);
const _: () = assert!(2.0 * BOOST_MAX_SPEED as f64 / TICK_RATE < BOOST_PAD_LENGTH as f64);

/// What a car is driving on, which can limit its speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Road,
    Grass,
}

impl Surface {
    /// Top speed either way on this surface, if it holds cars back: grass does, the road
    /// doesn't (the car's own [`top_speed`](super::car::CarState::top_speed) does).
    pub fn speed_limit(self) -> Option<f32> {
        match self {
            Surface::Road => None,
            Surface::Grass => Some(GRASS_MAX_SPEED),
        }
    }
}

/// Which track a race is on. The host picks it in the lobby
/// ([`TRACK_SETTING`](super::TRACK_SETTING)) and puts it on the race's replicated entity, next to
/// [`StartTick`](crate::minigames::countdown::StartTick), so every peer drives and draws the same track from the
/// moment it can start driving.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrackId {
    #[default]
    FigureEight,
    Oval,
}

impl TrackId {
    /// Every track, in the order the lobby's setting lists them.
    pub const ALL: [TrackId; 2] = [TrackId::FigureEight, TrackId::Oval];
    /// What the lobby calls each track, in [`ALL`](Self::ALL)'s order.
    pub const NAMES: [&'static str; 2] = ["Figure eight", "Oval"];

    /// The track a [`TRACK_SETTING`](super::TRACK_SETTING) value picks: its index in
    /// [`ALL`](Self::ALL).
    pub fn from_setting(value: u32) -> Self {
        Self::ALL.get(value as usize).copied().unwrap_or_default()
    }

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    pub fn layout(self) -> &'static Layout {
        static FIGURE_EIGHT_LAYOUT: Layout = FIGURE_EIGHT;
        static OVAL_LAYOUT: Layout = OVAL;
        match self {
            TrackId::FigureEight => &FIGURE_EIGHT_LAYOUT,
            TrackId::Oval => &OVAL_LAYOUT,
        }
    }

    /// The track worked out from its [`Layout`], once, on first use.
    pub fn track(self) -> &'static Track {
        static TRACKS: LazyLock<[Track; 2]> =
            LazyLock::new(|| TrackId::ALL.map(|id| Track::build(id.layout())));
        &TRACKS[self as usize]
    }
}

/// One piece of a track's centre line, followed from where the last one ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Piece {
    /// Straight on for this many world units.
    Straight(f32),
    /// Round part of a circle of `radius`, through `angle` radians: to the left (anticlockwise)
    /// if positive, to the right if negative.
    Turn { radius: f32, angle: f32 },
    /// A [`Checkpoint`] across the road here. The first is the start/finish line.
    Checkpoint,
    /// A [`BoostPad`] starting here, its middle `offset` to the left of the centre line (to the
    /// right if negative).
    BoostPad { offset: f32 },
}

/// A left turn through `degrees`.
const fn left(radius: f32, degrees: f32) -> Piece {
    Piece::Turn {
        radius,
        angle: degrees.to_radians(),
    }
}

/// A right turn through `degrees`.
const fn right(radius: f32, degrees: f32) -> Piece {
    Piece::Turn {
        radius,
        angle: -degrees.to_radians(),
    }
}

/// A track as data. Keep `tools/make_art.py`'s copy in step, then redraw the art.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// The middle of the start/finish line, where [`pieces`](Self::pieces) start and end.
    pub start_line: Vec2,
    /// Which way cars face on the grid, along the start straight, in radians anticlockwise from
    /// +x.
    pub start_heading: f32,
    /// The road is every point within this distance of the centre line.
    pub road_half_width: f32,
    /// The centre line, from the start line round to it again, in the direction the cars race.
    /// Turn radii set how fast a corner can be taken: a car at full steer turns at
    /// [`TURN_RATE`](super::config::TURN_RATE), so it holds a circle of radius
    /// `speed / TURN_RATE` (57 at top speed).
    pub pieces: &'static [Piece],
    /// Where the centre line crosses itself, if it does.
    pub crossing: Option<Vec2>,
    /// Its art, which covers the whole screen.
    pub image: &'static str,
    /// Racing's picture on its lobby tile while this track is picked (see
    /// [`Minigame::thumbnail`](crate::minigames::Minigame::thumbnail)).
    pub thumbnail: &'static str,
}

/// A figure of eight: tight and technical, with braking for a dip into the infield and two
/// hairpins. Where the line crosses itself, the start straight runs across the road coming down
/// from the top of the screen, so cars meet side-on there.
pub const FIGURE_EIGHT: Layout = Layout {
    start_line: Vec2::new(-60.0, -40.0),
    start_heading: 0.0,
    road_half_width: 24.0,
    pieces: &FIGURE_EIGHT_PIECES,
    crossing: Some(Vec2::new(50.0, -40.0)),
    image: "minigames/racing/figure_eight.png",
    thumbnail: "minigames/racing/figure_eight_thumbnail.png",
};

const FIGURE_EIGHT_PIECES: [Piece; 35] = [
    Piece::Checkpoint,
    // The start straight, across the crossing.
    Piece::Straight(280.0),
    left(45.0, 90.0),
    Piece::Checkpoint,
    Piece::Straight(85.0),
    left(45.0, 90.0),
    Piece::Straight(20.0),
    // The dip: down into the infield and back up, with a checkpoint at the bottom so it can't
    // be skipped across the grass.
    left(25.0, 90.0),
    Piece::Straight(20.0),
    right(32.0, 90.0),
    Piece::Checkpoint,
    right(32.0, 90.0),
    Piece::Straight(20.0),
    left(25.0, 90.0),
    // The top straight, into the hairpin: a boost on the inside, if you can still make the
    // hairpin after it.
    Piece::Straight(40.0),
    Piece::BoostPad { offset: 10.0 },
    Piece::Straight(266.0),
    left(32.0, 90.0),
    Piece::Checkpoint,
    left(32.0, 90.0),
    // Back across, and down through the crossing, with a boost on the right.
    Piece::Straight(60.0),
    Piece::BoostPad { offset: -10.0 },
    Piece::Straight(170.0),
    right(40.0, 90.0),
    Piece::Straight(106.0),
    Piece::Checkpoint,
    Piece::Straight(15.0),
    // The bottom loop, back onto the start straight.
    right(50.0, 90.0),
    Piece::Straight(105.0),
    Piece::Checkpoint,
    Piece::Straight(105.0),
    right(50.0, 90.0),
    Piece::Checkpoint,
    right(50.0, 90.0),
    Piece::Straight(150.0),
];

/// A wide oval, raced anticlockwise: flat out all the way round, so it's all about bumping.
pub const OVAL: Layout = Layout {
    start_line: Vec2::new(0.0, -130.0),
    start_heading: 0.0,
    road_half_width: 30.0,
    pieces: &OVAL_PIECES,
    crossing: None,
    image: "minigames/racing/oval.png",
    thumbnail: "minigames/racing/oval_thumbnail.png",
};

const OVAL_PIECES: [Piece; 15] = [
    Piece::Checkpoint,
    Piece::Straight(140.0),
    left(130.0, 90.0),
    Piece::Checkpoint,
    // A boost on the outside of the bend...
    Piece::BoostPad { offset: -14.0 },
    left(130.0, 90.0),
    // ...and one on the inside of the top straight.
    Piece::Straight(50.0),
    Piece::BoostPad { offset: 14.0 },
    Piece::Straight(90.0),
    Piece::Checkpoint,
    Piece::Straight(140.0),
    left(130.0, 90.0),
    Piece::Checkpoint,
    left(130.0, 90.0),
    Piece::Straight(140.0),
];

/// A strip across the road that a car must be seen in for its lap to count: [`CHECKPOINT_DEPTH`]
/// along the road, and reaching [`CHECKPOINT_MARGIN`] past both edges, so a car running a little
/// wide still counts but one cutting across the grass misses it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkpoint {
    centre: Vec2,
    /// Which way the road runs through it, in radians anticlockwise from +x.
    heading: f32,
    /// Its size along the road, then across it.
    size: Vec2,
    /// How far along the lap it starts, from the start line.
    along: f32,
}

impl Checkpoint {
    pub fn center(self) -> Vec2 {
        self.centre
    }

    /// Which way the road runs through it, in radians anticlockwise from +x.
    pub fn heading(self) -> f32 {
        self.heading
    }

    /// Its size along the road, then across it.
    pub fn size(self) -> Vec2 {
        self.size
    }

    /// Where it is and which way it points, for drawing it.
    pub fn isometry(self) -> Isometry2d {
        Isometry2d::new(self.centre, Rot2::radians(self.heading))
    }

    pub fn contains(self, position: Vec2) -> bool {
        in_rectangle(self.centre, self.heading, self.size, position)
    }
}

/// A patch of road a car on it is boosted by (see
/// [`CarState::boost`](super::car::CarState::boost)): [`BOOST_PAD_LENGTH`] along the road and
/// [`BOOST_PAD_WIDTH`] across it. A car is on it when its centre is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoostPad {
    centre: Vec2,
    /// Which way the road runs along it, in radians anticlockwise from +x.
    heading: f32,
}

impl BoostPad {
    pub fn center(self) -> Vec2 {
        self.centre
    }

    /// Which way the road runs along it, in radians anticlockwise from +x.
    pub fn heading(self) -> f32 {
        self.heading
    }

    /// Its size along the road, then across it.
    pub fn size(self) -> Vec2 {
        Vec2::new(BOOST_PAD_LENGTH, BOOST_PAD_WIDTH)
    }

    /// Where it is and which way it points, for drawing it.
    pub fn isometry(self) -> Isometry2d {
        Isometry2d::new(self.centre, Rot2::radians(self.heading))
    }

    pub fn contains(self, position: Vec2) -> bool {
        in_rectangle(self.centre, self.heading, self.size(), position)
    }
}

/// Whether `position` is in the rectangle of `size` (along `heading`, then across it) centred on
/// `centre`.
fn in_rectangle(centre: Vec2, heading: f32, size: Vec2, position: Vec2) -> bool {
    let local = Rot2::radians(-heading) * (position - centre);
    local.abs().cmple(size / 2.0).all()
}

/// A track worked out from its [`Layout`]: its centre line as exact straights and arcs, its
/// checkpoints and its boost pads.
pub struct Track {
    layout: &'static Layout,
    segments: Vec<Segment>,
    checkpoints: Vec<Checkpoint>,
    boost_pads: Vec<BoostPad>,
}

impl Track {
    pub fn layout(&self) -> &'static Layout {
        self.layout
    }

    /// The surface at `position`.
    pub fn surface_at(&self, position: Vec2) -> Surface {
        if self.distance_from_centre_line(position) <= self.layout.road_half_width {
            Surface::Road
        } else {
            Surface::Grass
        }
    }

    /// Where car `slot` (from 0) starts: two staggered lanes behind the start line, with the
    /// front car in the left lane.
    pub fn grid_position(&self, slot: usize) -> Vec2 {
        let row = (slot / 2) as f32;
        let outside = slot % 2 == 1;
        let back =
            GRID_ROW_SPACING * (row + 1.0) + if outside { GRID_ROW_SPACING / 2.0 } else { 0.0 };
        let lane = if outside {
            -GRID_LANE_OFFSET
        } else {
            GRID_LANE_OFFSET
        };
        let forward = Vec2::from_angle(self.layout.start_heading);
        self.layout.start_line - forward * back + forward.perp() * lane
    }

    /// The checkpoints a car must reach in order to complete a lap, in the order its pieces
    /// list them. Checkpoint 0 is the start/finish line (just past it), so reaching it after
    /// the others completes a lap.
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// The checkpoint `position` is in, if any.
    pub fn checkpoint_at(&self, position: Vec2) -> Option<usize> {
        self.checkpoints
            .iter()
            .position(|checkpoint| checkpoint.contains(position))
    }

    /// The boost pads, in the order its pieces list them.
    pub fn boost_pads(&self) -> &[BoostPad] {
        &self.boost_pads
    }

    /// The boost pad a car centred at `position` is on, if any.
    pub fn boost_pad_at(&self, position: Vec2) -> Option<usize> {
        self.boost_pads
            .iter()
            .position(|pad| pad.contains(position))
    }

    /// Points all the way round the track, `offset` to the left of the centre line (to the
    /// right if negative), at most `step` apart: `±road_half_width` gives the road's edges. The
    /// last point repeats the first.
    pub fn outline(&self, offset: f32, step: f32) -> Vec<Vec2> {
        let mut points: Vec<Vec2> = self.samples(offset, step).map(|(_, point)| point).collect();
        points.push(points[0]);
        points
    }

    fn build(layout: &'static Layout) -> Self {
        let mut segments: Vec<Segment> = Vec::new();
        let mut checkpoint_starts = Vec::new();
        let mut pad_starts = Vec::new();
        let (mut position, mut heading, mut along) = (layout.start_line, layout.start_heading, 0.0);
        for piece in layout.pieces {
            let (shape, length) = match *piece {
                Piece::Straight(length) => (
                    Shape::Line {
                        start: position,
                        direction: Vec2::from_angle(heading),
                    },
                    length,
                ),
                Piece::Turn { radius, angle } => {
                    // The circle's centre is off to the side the car turns towards.
                    let towards_centre = Vec2::from_angle(heading).perp() * angle.signum();
                    let centre = position + towards_centre * radius;
                    (
                        Shape::Arc {
                            centre,
                            radius,
                            start_angle: (-towards_centre).to_angle(),
                            sweep: angle,
                        },
                        radius * angle.abs(),
                    )
                }
                Piece::Checkpoint => {
                    checkpoint_starts.push(along);
                    continue;
                }
                Piece::BoostPad { offset } => {
                    pad_starts.push((along, offset));
                    continue;
                }
            };
            let segment = Segment {
                shape,
                along,
                length,
            };
            (position, heading) = segment.point(length);
            along += length;
            segments.push(segment);
        }

        let mut track = Self {
            layout,
            segments,
            checkpoints: Vec::new(),
            boost_pads: Vec::new(),
        };
        let size = Vec2::new(
            CHECKPOINT_DEPTH,
            2.0 * (layout.road_half_width + CHECKPOINT_MARGIN),
        );
        track.checkpoints = checkpoint_starts
            .into_iter()
            .map(|along| {
                let (centre, heading) = track.point_at(along + CHECKPOINT_DEPTH / 2.0);
                Checkpoint {
                    centre,
                    heading,
                    size,
                    along,
                }
            })
            .collect();
        track.boost_pads = pad_starts
            .into_iter()
            .map(|(along, offset)| {
                let (middle, heading) = track.point_at(along + BOOST_PAD_LENGTH / 2.0);
                BoostPad {
                    centre: middle + Vec2::from_angle(heading).perp() * offset,
                    heading,
                }
            })
            .collect();
        track
    }

    /// The length of one lap.
    fn length(&self) -> f32 {
        self.segments
            .last()
            .map_or(0.0, |last| last.along + last.length)
    }

    /// The point `along` the lap from the start line, and which way the track runs there.
    fn point_at(&self, along: f32) -> (Vec2, f32) {
        let along = along.rem_euclid(self.length());
        let segment = self
            .segments
            .iter()
            .rfind(|segment| segment.along <= along)
            .unwrap_or(&self.segments[0]);
        segment.point(along - segment.along)
    }

    fn distance_from_centre_line(&self, position: Vec2) -> f32 {
        self.segments
            .iter()
            .map(|segment| segment.distance_to(position))
            .fold(f32::INFINITY, f32::min)
    }

    /// How far along the lap, and the point `offset` to the left of the centre line there, at
    /// most `step` apart and at both ends of every segment, once round.
    fn samples(&self, offset: f32, step: f32) -> impl Iterator<Item = (f32, Vec2)> + '_ {
        self.segments.iter().flat_map(move |segment| {
            let count = (segment.length / step).ceil().max(1.0) as usize;
            (0..count).map(move |i| {
                let distance = segment.length * i as f32 / count as f32;
                let (point, heading) = segment.point(distance);
                (
                    segment.along + distance,
                    point + Vec2::from_angle(heading).perp() * offset,
                )
            })
        })
    }
}

/// A straight or a turn of the centre line, placed where it is on the track.
struct Segment {
    shape: Shape,
    /// How far along the lap it starts.
    along: f32,
    length: f32,
}

enum Shape {
    Line {
        start: Vec2,
        /// A unit vector.
        direction: Vec2,
    },
    Arc {
        centre: Vec2,
        radius: f32,
        /// Where it starts, as an angle from `centre`.
        start_angle: f32,
        /// How far round it goes, anticlockwise if positive.
        sweep: f32,
    },
}

impl Segment {
    /// The point `distance` along this segment, and which way the track runs there.
    fn point(&self, distance: f32) -> (Vec2, f32) {
        match self.shape {
            Shape::Line { start, direction } => {
                (start + direction * distance, direction.to_angle())
            }
            Shape::Arc {
                centre,
                radius,
                start_angle,
                sweep,
            } => {
                let angle = start_angle + sweep.signum() * distance / radius;
                (
                    centre + Vec2::from_angle(angle) * radius,
                    angle + sweep.signum() * FRAC_PI_2,
                )
            }
        }
    }

    fn distance_to(&self, position: Vec2) -> f32 {
        match self.shape {
            Shape::Line { start, direction } => {
                let along = (position - start).dot(direction).clamp(0.0, self.length);
                position.distance(start + direction * along)
            }
            Shape::Arc {
                centre,
                radius,
                start_angle,
                sweep,
            } => {
                let offset = position - centre;
                // How far round from the start, in the direction the arc goes.
                let round = ((offset.to_angle() - start_angle) * sweep.signum()).rem_euclid(TAU);
                if round <= sweep.abs() {
                    (offset.length() - radius).abs()
                } else {
                    let (first, _) = self.point(0.0);
                    let (last, _) = self.point(self.length);
                    position.distance(first).min(position.distance(last))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use bevy::math::Vec2;

    use super::{Piece, Surface, Track, TrackId};
    use crate::minigames::racing::config::{
        CAR_LENGTH, CAR_MAX_X, CAR_MAX_Y, CAR_WIDTH, CHECKPOINT_DEPTH, CHECKPOINT_MARGIN,
        GRID_SLOTS,
    };

    /// The least grass between two stretches of road that aren't the same bit of track, so
    /// players can tell them apart.
    const MIN_GRASS_BETWEEN: f32 = 12.0;

    fn tracks() -> impl Iterator<Item = (TrackId, &'static Track)> {
        TrackId::ALL.into_iter().map(|id| (id, id.track()))
    }

    fn half_width(track: &Track) -> f32 {
        track.layout().road_half_width
    }

    /// Whether `point` is near where the track crosses itself, where the road meets itself on
    /// purpose.
    fn near_crossing(track: &Track, point: Vec2) -> bool {
        track
            .layout()
            .crossing
            .is_some_and(|crossing| point.distance(crossing) < 3.0 * half_width(track))
    }

    /// How far apart two distances along the lap are, the short way round.
    fn apart_along(track: &Track, a: f32, b: f32) -> f32 {
        let gap = (a - b).abs();
        gap.min(track.length() - gap)
    }

    #[test]
    fn track_ids_index_their_tracks() {
        for (index, id) in TrackId::ALL.into_iter().enumerate() {
            assert_eq!(id as usize, index);
            assert_eq!(TrackId::from_setting(index as u32), id);
            assert!(std::ptr::eq(id.track().layout(), id.layout()));
        }
        assert_eq!(TrackId::from_setting(99), TrackId::default());
    }

    #[test]
    fn every_centre_line_ends_where_it_starts() {
        for (id, track) in tracks() {
            let layout = track.layout();
            let (end, heading) = track.point_at(track.length() - 1e-3);
            assert!(
                end.distance(layout.start_line) < 0.01,
                "{id:?} ends at {end}"
            );
            let turned = (heading - layout.start_heading).rem_euclid(TAU);
            assert!(
                turned < 1e-3 || turned > TAU - 1e-3,
                "{id:?} ends heading {heading}"
            );
        }
    }

    #[test]
    fn every_turn_is_wider_than_the_road() {
        for (id, track) in tracks() {
            for piece in track.layout().pieces {
                if let Piece::Turn { radius, .. } = piece {
                    assert!(*radius > half_width(track), "{id:?} {piece:?}");
                }
            }
        }
    }

    #[test]
    fn a_car_anywhere_on_the_road_is_on_screen() {
        let limit = Vec2::new(CAR_MAX_X, CAR_MAX_Y);
        for (id, track) in tracks() {
            for offset in [-half_width(track), half_width(track)] {
                for point in track.outline(offset, 2.0) {
                    assert!(point.abs().cmple(limit).all(), "{id:?} at {point}");
                }
            }
        }
    }

    #[test]
    fn the_whole_width_of_the_road_is_road() {
        for (id, track) in tracks() {
            let inside = half_width(track) - 0.5;
            for offset in [0.0, inside, -inside] {
                for point in track.outline(offset, 2.0) {
                    assert_eq!(track.surface_at(point), Surface::Road, "{id:?} at {point}");
                }
            }
        }
    }

    #[test]
    fn just_off_the_road_is_grass() {
        for (id, track) in tracks() {
            let outside = half_width(track) + 1.0;
            for offset in [outside, -outside] {
                for point in track.outline(offset, 2.0) {
                    if !near_crossing(track, point) {
                        assert_eq!(track.surface_at(point), Surface::Grass, "{id:?} at {point}");
                    }
                }
            }
        }
    }

    #[test]
    fn separate_stretches_of_road_have_grass_between_them() {
        for (id, track) in tracks() {
            let width = 2.0 * half_width(track);
            let line: Vec<(f32, Vec2)> = track.samples(0.0, 2.0).collect();
            for (i, &(a_along, a)) in line.iter().enumerate() {
                for &(b_along, b) in &line[i + 1..] {
                    if apart_along(track, a_along, b_along) < 3.0 * width
                        || near_crossing(track, a)
                        || near_crossing(track, b)
                    {
                        continue;
                    }
                    assert!(
                        a.distance(b) >= width + MIN_GRASS_BETWEEN,
                        "{id:?}: {a} and {b} are too close"
                    );
                }
            }
        }
    }

    #[test]
    fn a_track_crosses_itself_only_where_it_says() {
        // A track without a crossing is checked never to meet itself by
        // `separate_stretches_of_road_have_grass_between_them`.
        for (id, track) in tracks() {
            let Some(crossing) = track.layout().crossing else {
                continue;
            };
            assert_eq!(track.surface_at(crossing), Surface::Road, "{id:?}");
            let passes: Vec<f32> = track
                .samples(0.0, 0.5)
                .filter(|(_, point)| point.distance(crossing) < 0.5)
                .map(|(along, _)| along)
                .collect();
            assert!(!passes.is_empty(), "{id:?}");
            let first = passes[0];
            assert!(
                passes
                    .iter()
                    .any(|&along| apart_along(track, along, first) > 100.0),
                "{id:?} passes its crossing only once: {passes:?}"
            );
        }
    }

    #[test]
    fn the_infields_are_grass() {
        let figure_eight = TrackId::FigureEight.track();
        for point in [
            // Inside the loop at the top right, and the one at the bottom left.
            Vec2::new(150.0, 0.0),
            Vec2::new(-120.0, -90.0),
            // Between the top straight and the straight back from the hairpin.
            Vec2::new(-100.0, 103.0),
            // Across the top of the dip: driving straight on past it leaves the road.
            Vec2::new(143.0, 135.0),
        ] {
            assert_eq!(figure_eight.surface_at(point), Surface::Grass, "{point}");
        }
        let oval = TrackId::Oval.track();
        for point in [Vec2::ZERO, Vec2::new(0.0, -99.0), Vec2::new(0.0, -161.0)] {
            assert_eq!(oval.surface_at(point), Surface::Grass, "{point}");
        }
    }

    #[test]
    fn every_grid_slot_is_on_the_start_straight_behind_the_line() {
        for (id, track) in tracks() {
            let layout = track.layout();
            let forward = Vec2::from_angle(layout.start_heading);
            let side = forward.perp();
            let slots: Vec<Vec2> = (0..GRID_SLOTS)
                .map(|slot| track.grid_position(slot))
                .collect();
            for (i, slot) in slots.iter().enumerate() {
                assert!(
                    (*slot - layout.start_line).dot(forward) < 0.0,
                    "{id:?} slot {i}"
                );
                // The whole car, all four corners.
                for along in [-CAR_LENGTH / 2.0, CAR_LENGTH / 2.0] {
                    for across in [-CAR_WIDTH / 2.0, CAR_WIDTH / 2.0] {
                        let corner = *slot + forward * along + side * across;
                        assert_eq!(
                            track.surface_at(corner),
                            Surface::Road,
                            "{id:?} slot {i} at {corner}"
                        );
                    }
                }
                for other in &slots[i + 1..] {
                    assert!(
                        slot.distance(*other) >= CAR_LENGTH,
                        "{id:?} slot {i} overlaps"
                    );
                }
                assert_eq!(track.checkpoint_at(*slot), None, "{id:?} slot {i}");
            }
        }
    }

    #[test]
    fn driving_the_centre_line_reaches_every_checkpoint_once_in_order() {
        for (id, track) in tracks() {
            let mut reached = Vec::new();
            for (_, point) in track.samples(0.0, 1.0) {
                if let Some(checkpoint) = track.checkpoint_at(point)
                    && reached.last() != Some(&checkpoint)
                {
                    reached.push(checkpoint);
                }
            }
            let expected: Vec<usize> = (0..track.checkpoints().len()).collect();
            assert_eq!(reached, expected, "{id:?}");
        }
    }

    #[test]
    fn every_checkpoint_reaches_just_past_the_road_edges() {
        for (id, track) in tracks() {
            let reach = half_width(track) + CHECKPOINT_MARGIN;
            for (i, checkpoint) in track.checkpoints().iter().enumerate() {
                let side = Vec2::from_angle(checkpoint.heading()).perp();
                let centre = checkpoint.center();
                assert_eq!(track.surface_at(centre), Surface::Road, "{id:?} {i}");
                for sign in [1.0, -1.0] {
                    let inside = centre + side * sign * (reach - 1.0);
                    let outside = centre + side * sign * (reach + 1.0);
                    assert!(checkpoint.contains(inside), "{id:?} {i} at {inside}");
                    assert!(!checkpoint.contains(outside), "{id:?} {i} at {outside}");
                }
            }
        }
    }

    #[test]
    fn no_checkpoint_reaches_another_stretch_of_road() {
        for (id, track) in tracks() {
            // Further along the lap than a checkpoint's own corner can reach.
            let own_stretch = CHECKPOINT_DEPTH + 2.0 * (half_width(track) + CHECKPOINT_MARGIN);
            for offset in [0.0, half_width(track), -half_width(track)] {
                for (along, point) in track.samples(offset, 2.0) {
                    for (i, checkpoint) in track.checkpoints().iter().enumerate() {
                        if apart_along(track, along, checkpoint.along) > own_stretch {
                            assert!(!checkpoint.contains(point), "{id:?} {i} at {point}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn checkpoints_dont_overlap() {
        for (id, track) in tracks() {
            for x in (-320..=320).step_by(2) {
                for y in (-180..=180).step_by(2) {
                    let point = Vec2::new(x as f32, y as f32);
                    let count = track
                        .checkpoints()
                        .iter()
                        .filter(|checkpoint| checkpoint.contains(point))
                        .count();
                    assert!(count <= 1, "{id:?}: {count} checkpoints at {point}");
                }
            }
        }
    }

    #[test]
    fn the_first_checkpoint_is_just_past_the_start_line() {
        for (id, track) in tracks() {
            let layout = track.layout();
            let first = track.checkpoints()[0];
            let forward = Vec2::from_angle(layout.start_heading);
            assert_eq!(first.along, 0.0, "{id:?}");
            assert!(first.contains(layout.start_line + forward), "{id:?}");
            assert!(!first.contains(layout.start_line - forward), "{id:?}");
        }
    }

    /// The four corners of a car centred at `centre`, pointing `heading`.
    fn car_corners(centre: Vec2, heading: f32) -> impl Iterator<Item = Vec2> {
        let forward = Vec2::from_angle(heading);
        let side = forward.perp();
        [-CAR_LENGTH / 2.0, CAR_LENGTH / 2.0]
            .into_iter()
            .flat_map(move |along| {
                [-CAR_WIDTH / 2.0, CAR_WIDTH / 2.0]
                    .map(|across| centre + forward * along + side * across)
            })
    }

    #[test]
    fn every_track_has_boost_pads() {
        for (id, track) in tracks() {
            assert!(track.boost_pads().len() >= 2, "{id:?}");
        }
    }

    #[test]
    fn every_boost_pad_is_on_the_road() {
        for (id, track) in tracks() {
            for (i, pad) in track.boost_pads().iter().enumerate() {
                let half = pad.size() / 2.0;
                for corner in [
                    Vec2::new(-half.x, -half.y),
                    Vec2::new(-half.x, half.y),
                    Vec2::new(half.x, -half.y),
                    Vec2::new(half.x, half.y),
                ] {
                    let corner = pad.isometry() * corner;
                    assert_eq!(
                        track.surface_at(corner),
                        Surface::Road,
                        "{id:?} pad {i} at {corner}"
                    );
                }
                assert_eq!(track.boost_pad_at(pad.center()), Some(i), "{id:?}");
            }
        }
    }

    #[test]
    fn boost_pads_reach_along_and_across_the_road_from_their_middle() {
        let pad = TrackId::Oval.track().boost_pads()[1];
        let along = Vec2::from_angle(pad.heading());
        let across = along.perp();
        let size = pad.size();
        for (direction, reach) in [(along, size.x / 2.0), (across, size.y / 2.0)] {
            for sign in [1.0, -1.0] {
                let inside = pad.center() + direction * sign * (reach - 0.5);
                let outside = pad.center() + direction * sign * (reach + 0.5);
                assert!(pad.contains(inside), "{inside}");
                assert!(!pad.contains(outside), "{outside}");
            }
        }
    }

    #[test]
    fn no_car_starts_on_a_boost_pad() {
        for (id, track) in tracks() {
            let heading = track.layout().start_heading;
            for slot in 0..GRID_SLOTS {
                let centre = track.grid_position(slot);
                for point in car_corners(centre, heading).chain([centre]) {
                    assert_eq!(
                        track.boost_pad_at(point),
                        None,
                        "{id:?} slot {slot} at {point}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_oval_keeps_its_old_shape() {
        // A stadium: straights at y = ±130 from x = -140 to 140, bends of radius 130 round each
        // end, and the road 60 wide.
        let oval = TrackId::Oval.track();
        for (point, expected) in [
            (Vec2::new(0.0, -101.0), Surface::Road),
            (Vec2::new(0.0, -159.0), Surface::Road),
            (Vec2::new(140.0 + 101.0, 0.0), Surface::Road),
            (Vec2::new(-140.0 - 159.0, 0.0), Surface::Road),
            (Vec2::new(0.0, 99.0), Surface::Grass),
            (Vec2::new(140.0 + 120.0, 120.0), Surface::Grass),
        ] {
            assert_eq!(oval.surface_at(point), expected, "{point}");
        }
        assert_eq!(oval.checkpoints().len(), 4);
    }
}
