//! Corner minimap showing the whole track and every car's position in real time. Drawn as a
//! translucent HUD card in the bottom-right corner with a crisp procedural silhouette built
//! directly from [`Track`], plus a coloured dot for each car (larger with a white ring for yours).

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use lightyear::prelude::input::native::InputMarker;

use super::PHASE;
use super::RacingInput;
use super::car::Car;
use super::track::{Track, TrackId};
use super::visuals::sync_car_transform;
use crate::players::player_color;

/// Size of the minimap in screen pixels (16:9, matching the tracks).
const MAP_WIDTH: f32 = 224.0;
const MAP_HEIGHT: f32 = 126.0;
const MAP_MARGIN: f32 = 16.0;
const MAP_PADDING: f32 = 6.0;
const MAP_RADIUS: f32 = 8.0;

/// Resolution of the procedural track silhouette texture.
const TEX_WIDTH: u32 = 256;
const TEX_HEIGHT: u32 = 144;

/// Card backdrop and border colours.
const PANEL_BG: Color = Color::srgba(0.06, 0.07, 0.11, 0.65);
const PANEL_BORDER: Color = Color::srgba(1.0, 1.0, 1.0, 0.18);

/// Road and start-line colours in the generated texture (sRGBA bytes).
const ROAD_FILL: [u8; 4] = [214, 218, 228, 215];
const ROAD_OUTLINE: [u8; 4] = [24, 26, 36, 230];
const START_LINE_COLOR: [u8; 4] = [255, 255, 255, 255];

/// Car dot sizes in screen pixels.
const OWN_DOT_SIZE: f32 = 10.0;
const OTHER_DOT_SIZE: f32 = 7.0;
const OWN_DOT_BORDER: f32 = 2.0;
const OTHER_DOT_BORDER: f32 = 1.5;
const OTHER_DOT_BORDER_COLOR: Color = Color::srgba(0.08, 0.09, 0.13, 0.95);

/// Container for the minimap track texture and car dots.
#[derive(Component)]
pub struct MinimapRoot;

/// A car's dot on the minimap, tracking [`Self::car`].
#[derive(Component)]
pub struct MinimapDot {
    car: Entity,
}

/// Draws the corner minimap during a race. Added by
/// [`RacingVisualsPlugin`](super::RacingVisualsPlugin).
pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_minimap.run_if(in_state(PHASE)));
        app.add_systems(
            PostUpdate,
            sync_minimap_dots
                .after(sync_car_transform)
                .run_if(in_state(PHASE)),
        );
    }
}

/// Map a world point inside a track of `track_size` (centred on `(0, 0)`, y-up) to minimap pixel
/// coordinates inside `map_size` (top-left origin, y-down).
pub fn world_to_minimap(world: Vec2, track_size: Vec2, map_size: Vec2) -> Vec2 {
    let u = (world.x / track_size.x + 0.5).clamp(0.0, 1.0);
    let v = (0.5 - world.y / track_size.y).clamp(0.0, 1.0);
    Vec2::new(u * map_size.x, v * map_size.y)
}

/// Render a crisp anti-aliased silhouette of `track` (road, dark edge and start line) on a
/// transparent background.
fn render_track_image(track: &Track) -> Image {
    let layout = track.layout();
    let track_size = track.size();
    let half_width = layout.road_half_width;
    let world_per_px = track_size.x / TEX_WIDTH as f32;
    let outline_width = 1.6 * world_per_px;
    let aa = 1.0 * world_per_px;
    let start_rot = Rot2::radians(-layout.start_heading);
    let start_half_depth = 1.2 * world_per_px;

    let mut data = vec![0u8; (TEX_WIDTH * TEX_HEIGHT * 4) as usize];
    for py in 0..TEX_HEIGHT {
        let wy = (0.5 - (py as f32 + 0.5) / TEX_HEIGHT as f32) * track_size.y;
        for px in 0..TEX_WIDTH {
            let wx = ((px as f32 + 0.5) / TEX_WIDTH as f32 - 0.5) * track_size.x;
            let world = Vec2::new(wx, wy);
            let edge = track.distance_from_centre_line(world) - half_width;
            if edge > outline_width + aa {
                continue;
            }
            let local_start = start_rot * (world - layout.start_line);
            let pixel =
                if local_start.x.abs() <= start_half_depth && local_start.y.abs() <= half_width {
                    START_LINE_COLOR
                } else if edge <= -aa * 0.5 {
                    ROAD_FILL
                } else if edge <= aa * 0.5 {
                    let t = (edge + aa * 0.5) / aa;
                    blend_rgba(ROAD_FILL, ROAD_OUTLINE, t)
                } else if edge <= outline_width {
                    ROAD_OUTLINE
                } else {
                    let alpha = 1.0 - (edge - outline_width) / aa;
                    [
                        ROAD_OUTLINE[0],
                        ROAD_OUTLINE[1],
                        ROAD_OUTLINE[2],
                        (f32::from(ROAD_OUTLINE[3]) * alpha.clamp(0.0, 1.0)).round() as u8,
                    ]
                };
            let idx = ((py * TEX_WIDTH + px) * 4) as usize;
            data[idx..idx + 4].copy_from_slice(&pixel);
        }
    }

    Image::new(
        Extent3d {
            width: TEX_WIDTH,
            height: TEX_HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn blend_rgba(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (f32::from(x) * (1.0 - t) + f32::from(y) * t).round() as u8;
    [
        mix(a[0], b[0]),
        mix(a[1], b[1]),
        mix(a[2], b[2]),
        mix(a[3], b[3]),
    ]
}

/// Spawn the minimap card once the race's [`TrackId`] is known.
fn spawn_minimap(
    track: Option<Single<&TrackId>>,
    existing: Query<(), With<MinimapRoot>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    let Some(track) = track.map(|id| id.track()) else {
        return;
    };
    if !existing.is_empty() {
        return;
    }
    let texture = images.add(render_track_image(track));
    commands.spawn((
        DespawnOnExit(PHASE),
        Pickable::IGNORE,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(MAP_MARGIN),
            right: px(MAP_MARGIN),
            padding: UiRect::all(px(MAP_PADDING)),
            border: UiRect::all(px(1.5)),
            border_radius: BorderRadius::all(px(MAP_RADIUS)),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
        children![(
            MinimapRoot,
            Pickable::IGNORE,
            ImageNode::new(texture),
            Node {
                width: px(MAP_WIDTH),
                height: px(MAP_HEIGHT),
                ..default()
            },
        )],
    ));
}

type MappedCar = (
    Entity,
    &'static Car,
    &'static Transform,
    Has<InputMarker<RacingInput>>,
);

/// Keep one minimap dot per active car, positioned from its smoothed world transform.
fn sync_minimap_dots(
    track: Option<Single<&TrackId>>,
    map: Option<Single<Entity, With<MinimapRoot>>>,
    cars: Query<MappedCar, With<Car>>,
    mut dots: Query<(Entity, &MinimapDot, &mut Node)>,
    mut commands: Commands,
) {
    let (Some(track), Some(map_entity)) = (track.map(|id| id.track()), map.map(|m| *m)) else {
        return;
    };
    let track_size = track.size();
    let map_size = Vec2::new(MAP_WIDTH, MAP_HEIGHT);

    // Update existing dots and drop dots for cars that left.
    let mut mapped_cars = Vec::new();
    for (dot_entity, dot, mut node) in &mut dots {
        let Ok((_, _, transform, own)) = cars.get(dot.car) else {
            commands.entity(dot_entity).despawn();
            continue;
        };
        let size = if own { OWN_DOT_SIZE } else { OTHER_DOT_SIZE };
        let pos = world_to_minimap(transform.translation.truncate(), track_size, map_size);
        node.left = px(pos.x - size / 2.0);
        node.top = px(pos.y - size / 2.0);
        mapped_cars.push(dot.car);
    }

    // Spawn dots for cars that don't have one yet.
    for (car_entity, car, transform, own) in &cars {
        if mapped_cars.contains(&car_entity) {
            continue;
        }
        let (size, border, border_color, z) = if own {
            (OWN_DOT_SIZE, OWN_DOT_BORDER, Color::WHITE, ZIndex(2))
        } else {
            (
                OTHER_DOT_SIZE,
                OTHER_DOT_BORDER,
                OTHER_DOT_BORDER_COLOR,
                ZIndex(1),
            )
        };
        let pos = world_to_minimap(transform.translation.truncate(), track_size, map_size);
        commands.entity(map_entity).with_children(|parent| {
            parent.spawn((
                MinimapDot { car: car_entity },
                Pickable::IGNORE,
                z,
                BackgroundColor(player_color(car.color)),
                BorderColor::all(border_color),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(pos.x - size / 2.0),
                    top: px(pos.y - size / 2.0),
                    width: px(size),
                    height: px(size),
                    border: UiRect::all(px(border)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
            ));
        });
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::Vec2;

    use super::{TEX_HEIGHT, TEX_WIDTH, render_track_image, world_to_minimap};
    use crate::minigames::racing::track::TrackId;

    #[test]
    fn world_to_minimap_maps_centre_and_corners() {
        let track = Vec2::new(1280.0, 720.0);
        let map = Vec2::new(224.0, 126.0);
        assert_eq!(
            world_to_minimap(Vec2::ZERO, track, map),
            Vec2::new(112.0, 63.0)
        );
        // Top-left in world (-x, +y) maps to (0, 0) in UI coordinates.
        assert_eq!(
            world_to_minimap(Vec2::new(-640.0, 360.0), track, map),
            Vec2::ZERO
        );
        // Bottom-right in world (+x, -y) maps to (map.x, map.y).
        assert_eq!(world_to_minimap(Vec2::new(640.0, -360.0), track, map), map);
        // Out-of-bounds points clamp inside the minimap.
        assert_eq!(
            world_to_minimap(Vec2::new(999.0, 999.0), track, map),
            Vec2::new(224.0, 0.0)
        );
    }

    #[test]
    fn track_image_draws_road_and_leaves_infield_clear() {
        for id in TrackId::ALL {
            let image = render_track_image(id.track());
            assert_eq!(image.width(), TEX_WIDTH);
            assert_eq!(image.height(), TEX_HEIGHT);
            let data = image.data.as_ref().expect("image has pixel bytes");
            let opaque = data
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|px| px[3] > 0)
                .count();
            let total = (TEX_WIDTH * TEX_HEIGHT) as usize;
            assert!(
                opaque > total / 10 && opaque < total * 3 / 4,
                "{id:?}: unexpected opaque pixels {opaque}/{total}"
            );
        }
    }
}
