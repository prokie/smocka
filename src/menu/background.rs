//! Animated twilight backdrop behind every menu, lobby and waiting screen.
//!
//! Renders a full-window illustrated backdrop ([`BACKGROUND_IMAGE`]) behind all menu UI at
//! [`BACKGROUND_Z_INDEX`], layered with subtle parallax breathing, rustling corner foliage fronds,
//! drifting wind-borne leaves and warm glowing fireflies. Automatically hides whenever a
//! [`GameplayCamera`] is active so minigames draw unhindered.

use std::f32::consts::TAU;

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::camera::GameplayCamera;

const BACKGROUND_IMAGE: &str = "menu/background.png";
const LEAF_IMAGE: &str = "menu/leaf.png";
const FROND_IMAGE: &str = "menu/frond.png";

/// Behind all menu screens, overlays and notices (which keep the default `GlobalZIndex(0)`).
const BACKGROUND_Z_INDEX: i32 = -10;
/// Overscale on the backdrop image so slow parallax sway never exposes bare window edges.
const BACKDROP_SCALE: f32 = 1.07;
/// Light darkening veil over the backdrop so white menu text stays crisp everywhere.
const SCRIM_COLOR: Color = Color::srgba(0.04, 0.06, 0.12, 0.22);

const LEAF_COUNT: usize = 14;
const FIREFLY_COUNT: usize = 14;

/// Root container for the animated menu backdrop; hidden while a minigame is playing.
#[derive(Component)]
struct MenuBackground;

/// The full-bleed twilight festival illustration, slowly drifting in a gentle breeze.
#[derive(Component)]
struct BackgroundArtwork;

/// A leafy branch silhouette anchored to a screen edge that rocks gently in the wind.
#[derive(Component, Clone, Copy)]
struct SwayingFrond {
    base_angle: f32,
    amplitude: f32,
    speed: f32,
    phase: f32,
}

/// One stylized leaf carried across the window on sinusoidal wind currents.
#[derive(Component, Clone, Copy)]
struct DriftingLeaf {
    /// Horizontal position in `0.0..100.0` (with wrap margin `-6.0..106.0`).
    x: f32,
    /// Vertical position in `0.0..100.0` (with wrap margin `-6.0..106.0`).
    y: f32,
    /// Horizontal breeze speed, in percent of window width per second.
    speed_x: f32,
    /// Gentle downward drift, in percent of window height per second.
    speed_y: f32,
    flutter_speed: f32,
    flutter_amp: f32,
    spin_speed: f32,
    phase: f32,
}

/// One small evening firefly rising slowly and pulsing softly in alpha.
#[derive(Component, Clone, Copy)]
struct FireflyMote {
    x: f32,
    y: f32,
    rise_speed: f32,
    sway_speed: f32,
    sway_amp: f32,
    pulse_speed: f32,
    phase: f32,
    base_color: Color,
}

/// Spawns and animates the living background behind menus, hiding it during minigames.
pub(super) struct MenuBackgroundPlugin;

impl Plugin for MenuBackgroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_menu_background);
        app.add_systems(
            Update,
            (
                animate_backdrop,
                animate_fronds,
                animate_leaves,
                animate_fireflies,
            )
                .run_if(no_gameplay_camera),
        );
        app.add_systems(PostUpdate, sync_background_visibility);
    }
}

fn no_gameplay_camera(gameplay: Query<(), With<GameplayCamera>>) -> bool {
    gameplay.is_empty()
}

fn spawn_menu_background(assets: Res<AssetServer>, mut commands: Commands) {
    let backdrop = assets.load(BACKGROUND_IMAGE);
    let leaf_image = assets.load(LEAF_IMAGE);
    let frond_image = assets.load(FROND_IMAGE);

    // Fronds tucked into the upper left and right edges, matching the twilight foliage palette.
    let fronds = [
        (
            Val::Percent(-3.0),
            Val::Percent(-2.0),
            false,
            260.0,
            Color::srgba(0.11, 0.28, 0.30, 0.85),
            SwayingFrond {
                base_angle: 0.06,
                amplitude: 0.055,
                speed: 0.75,
                phase: 0.0,
            },
        ),
        (
            Val::Percent(-5.0),
            Val::Percent(8.0),
            false,
            210.0,
            Color::srgba(0.18, 0.42, 0.38, 0.70),
            SwayingFrond {
                base_angle: 0.22,
                amplitude: 0.075,
                speed: 0.95,
                phase: 1.7,
            },
        ),
        (
            Val::Percent(82.0),
            Val::Percent(-2.0),
            true,
            260.0,
            Color::srgba(0.54, 0.37, 0.31, 0.78),
            SwayingFrond {
                base_angle: -0.06,
                amplitude: 0.055,
                speed: 0.82,
                phase: 3.1,
            },
        ),
        (
            Val::Percent(86.0),
            Val::Percent(10.0),
            true,
            200.0,
            Color::srgba(0.16, 0.39, 0.33, 0.72),
            SwayingFrond {
                base_angle: -0.20,
                amplitude: 0.07,
                speed: 1.05,
                phase: 4.5,
            },
        ),
    ];

    // Leaf tints drawn from the illustration's canopy & dusk highlights.
    let leaf_palette = [
        Color::srgba(0.24, 0.58, 0.52, 0.70),
        Color::srgba(0.32, 0.68, 0.48, 0.65),
        Color::srgba(0.84, 0.55, 0.40, 0.68),
        Color::srgba(0.88, 0.70, 0.44, 0.62),
        Color::srgba(0.20, 0.46, 0.48, 0.66),
    ];

    let firefly_palette = [
        Color::srgb(1.0, 0.85, 0.48),  // warm lantern gold
        Color::srgb(1.0, 0.74, 0.46),  // soft dusk amber
        Color::srgb(0.68, 0.88, 1.0),  // cool twilight starlight
        Color::srgb(0.62, 0.96, 0.74), // soft meadow mint
    ];

    commands
        .spawn((
            MenuBackground,
            GlobalZIndex(BACKGROUND_Z_INDEX),
            Pickable::IGNORE,
            BackgroundColor(Color::srgb(0.07, 0.09, 0.16)),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .with_children(|root| {
            // Illustrated twilight meadow backdrop with gentle parallax drift.
            root.spawn((
                BackgroundArtwork,
                Pickable::IGNORE,
                ImageNode::new(backdrop),
                UiTransform::from_scale(Vec2::splat(BACKDROP_SCALE)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
            ));

            // Soft dark veil for reliable UI contrast across every menu screen.
            root.spawn((
                Pickable::IGNORE,
                BackgroundColor(SCRIM_COLOR),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
            ));

            // Corner branches rustling softly in the breeze.
            for (left, top, flip_x, width, color, frond) in fronds {
                let height = width * (36.0 / 64.0);
                root.spawn((
                    frond,
                    Pickable::IGNORE,
                    ImageNode {
                        image: frond_image.clone(),
                        color,
                        flip_x,
                        ..default()
                    },
                    UiTransform::from_rotation(Rot2::radians(frond.base_angle)),
                    Node {
                        position_type: PositionType::Absolute,
                        left,
                        top,
                        width: px(width),
                        height: px(height),
                        ..default()
                    },
                ));
            }

            // Drifting leaves spread deterministically across the viewport from frame 1.
            for index in 0..LEAF_COUNT {
                let f = index as f32;
                let x = ((f * 37.3 + 11.0) % 108.0) - 4.0;
                let y = ((f * 23.7 + 7.0) % 92.0) + 2.0;
                let size = 14.0 + (index % 4) as f32 * 4.5;
                let leaf = DriftingLeaf {
                    x,
                    y,
                    speed_x: 1.8 + (index % 5) as f32 * 0.55,
                    speed_y: 0.7 + (index % 3) as f32 * 0.35,
                    flutter_speed: 1.2 + (index % 4) as f32 * 0.35,
                    flutter_amp: 2.2 + (index % 3) as f32 * 0.8,
                    spin_speed: 0.7 + (index % 4) as f32 * 0.25,
                    phase: f * 1.13,
                };
                let color = leaf_palette[index % leaf_palette.len()];
                root.spawn((
                    leaf,
                    Pickable::IGNORE,
                    ImageNode {
                        image: leaf_image.clone(),
                        color,
                        ..default()
                    },
                    UiTransform::default(),
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(leaf.x),
                        top: percent(leaf.y),
                        width: px(size),
                        height: px(size),
                        ..default()
                    },
                ));
            }

            // Warm evening fireflies rising gently in the lower and middle meadow air.
            for index in 0..FIREFLY_COUNT {
                let f = index as f32;
                let x = ((f * 29.1 + 5.0) % 94.0) + 3.0;
                let y = 32.0 + ((f * 19.7) % 64.0);
                let size = 3.5 + (index % 3) as f32 * 1.5;
                let base_color = firefly_palette[index % firefly_palette.len()];
                let mote = FireflyMote {
                    x,
                    y,
                    rise_speed: 0.65 + (index % 4) as f32 * 0.22,
                    sway_speed: 0.6 + (index % 3) as f32 * 0.25,
                    sway_amp: 1.4 + (index % 3) as f32 * 0.6,
                    pulse_speed: 1.1 + (index % 5) as f32 * 0.32,
                    phase: f * 1.47,
                    base_color,
                };
                root.spawn((
                    mote,
                    Pickable::IGNORE,
                    BackgroundColor(base_color.with_alpha(0.35)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(mote.x),
                        top: percent(mote.y),
                        width: px(size),
                        height: px(size),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                ));
            }
        });
}

/// Hide the menu backdrop whenever a minigame's [`GameplayCamera`] is on screen.
fn sync_background_visibility(
    gameplay: Query<(), With<GameplayCamera>>,
    mut roots: Query<&mut Visibility, With<MenuBackground>>,
) {
    let expected = if gameplay.is_empty() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut visibility in &mut roots {
        if *visibility != expected {
            *visibility = expected;
        }
    }
}

/// Gentle camera-drift parallax on the backdrop image.
fn backdrop_offset(elapsed: f32) -> Vec2 {
    Vec2::new(
        (elapsed * TAU / 22.0).sin() * 1.15,
        (elapsed * TAU / 31.0).cos() * 0.85,
    )
}

fn animate_backdrop(
    time: Res<Time>,
    mut artworks: Query<&mut UiTransform, With<BackgroundArtwork>>,
) {
    let offset = backdrop_offset(time.elapsed_secs());
    for mut transform in &mut artworks {
        transform.translation = Val2::new(Val::Percent(offset.x), Val::Percent(offset.y));
        transform.scale = Vec2::splat(BACKDROP_SCALE);
    }
}

/// Layered wind sway for a corner foliage frond, combining a slow breeze with a softer secondary
/// gust so the movement stays organic.
fn frond_angle(frond: SwayingFrond, elapsed: f32) -> f32 {
    let primary = (elapsed * frond.speed + frond.phase).sin();
    let gust = (elapsed * frond.speed * 1.83 + frond.phase * 1.7).sin() * 0.35;
    frond.base_angle + (primary + gust) * frond.amplitude
}

fn animate_fronds(time: Res<Time>, mut fronds: Query<(&SwayingFrond, &mut UiTransform)>) {
    let elapsed = time.elapsed_secs();
    for (frond, mut transform) in &mut fronds {
        transform.rotation = Rot2::radians(frond_angle(*frond, elapsed));
    }
}

fn wrap_percent(value: f32, min: f32, max: f32) -> f32 {
    let span = max - min;
    (value - min).rem_euclid(span) + min
}

/// Advance a drifting leaf by `dt` seconds and return its display `(x, y)` percent coordinates and
/// rotation angle in radians.
fn step_leaf(leaf: &mut DriftingLeaf, elapsed: f32, dt: f32) -> (f32, f32, f32) {
    let gust = 1.0 + 0.25 * (elapsed * 0.45 + leaf.phase).sin();
    leaf.x = wrap_percent(leaf.x + leaf.speed_x * gust * dt, -6.0, 106.0);
    leaf.y = wrap_percent(leaf.y + leaf.speed_y * dt, -6.0, 106.0);
    let flutter_y = (elapsed * leaf.flutter_speed + leaf.phase).sin() * leaf.flutter_amp;
    let angle =
        (elapsed * leaf.spin_speed + leaf.phase).sin() * 0.9 + (elapsed * 0.35 + leaf.phase);
    (leaf.x, leaf.y + flutter_y, angle)
}

fn animate_leaves(
    time: Res<Time>,
    mut leaves: Query<(&mut DriftingLeaf, &mut Node, &mut UiTransform)>,
) {
    let elapsed = time.elapsed_secs();
    let dt = time.delta_secs();
    for (mut leaf, mut node, mut transform) in &mut leaves {
        let (x, y, angle) = step_leaf(&mut leaf, elapsed, dt);
        node.left = percent(x);
        node.top = percent(y);
        transform.rotation = Rot2::radians(angle);
    }
}

fn firefly_alpha(mote: FireflyMote, elapsed: f32) -> f32 {
    let wave = 0.5 + 0.5 * (elapsed * mote.pulse_speed + mote.phase).sin();
    0.08 + 0.58 * wave * wave
}

fn animate_fireflies(
    time: Res<Time>,
    mut motes: Query<(&mut FireflyMote, &mut Node, &mut BackgroundColor)>,
) {
    let elapsed = time.elapsed_secs();
    let dt = time.delta_secs();
    for (mut mote, mut node, mut color) in &mut motes {
        mote.y = wrap_percent(mote.y - mote.rise_speed * dt, 18.0, 102.0);
        let sway = (elapsed * mote.sway_speed + mote.phase).sin() * mote.sway_amp;
        node.left = percent(mote.x + sway);
        node.top = percent(mote.y);
        color.0 = mote.base_color.with_alpha(firefly_alpha(*mote, elapsed));
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;

    use super::{
        DriftingLeaf, FireflyMote, MenuBackground, SwayingFrond, backdrop_offset, firefly_alpha,
        frond_angle, step_leaf, sync_background_visibility,
    };
    use crate::camera::GameplayCamera;

    #[test]
    fn background_hides_only_while_a_gameplay_camera_exists() {
        let mut world = World::new();
        let root = world.spawn((MenuBackground, Visibility::Inherited)).id();

        world.run_system_once(sync_background_visibility).unwrap();
        assert_eq!(
            *world.get::<Visibility>(root).unwrap(),
            Visibility::Inherited
        );

        let camera = world.spawn(GameplayCamera).id();
        world.run_system_once(sync_background_visibility).unwrap();
        assert_eq!(*world.get::<Visibility>(root).unwrap(), Visibility::Hidden);

        world.despawn(camera);
        world.run_system_once(sync_background_visibility).unwrap();
        assert_eq!(
            *world.get::<Visibility>(root).unwrap(),
            Visibility::Inherited
        );
    }

    #[test]
    fn backdrop_leaves_fronds_and_fireflies_stay_within_expected_bounds() {
        for step in 0..60 {
            let t = step as f32 * 0.5;
            let offset = backdrop_offset(t);
            assert!(offset.x.abs() <= 1.2 && offset.y.abs() <= 0.9, "{offset}");
        }

        let frond = SwayingFrond {
            base_angle: 0.1,
            amplitude: 0.06,
            speed: 0.8,
            phase: 0.5,
        };
        for step in 0..40 {
            let angle = frond_angle(frond, step as f32 * 0.25);
            assert!((angle - frond.base_angle).abs() <= frond.amplitude * 1.4);
        }

        let mut leaf = DriftingLeaf {
            x: 104.0,
            y: 105.0,
            speed_x: 3.0,
            speed_y: 2.0,
            flutter_speed: 1.5,
            flutter_amp: 2.0,
            spin_speed: 1.0,
            phase: 0.0,
        };
        let (x, y, angle) = step_leaf(&mut leaf, 1.0, 1.0);
        assert!((-6.0..106.0).contains(&x), "{x}");
        assert!((-8.5..108.5).contains(&y), "{y}");
        assert!(angle.is_finite());

        let mote = FireflyMote {
            x: 50.0,
            y: 60.0,
            rise_speed: 1.0,
            sway_speed: 1.0,
            sway_amp: 1.5,
            pulse_speed: 1.5,
            phase: 0.0,
            base_color: Color::WHITE,
        };
        for step in 0..40 {
            let alpha = firefly_alpha(mote, step as f32 * 0.2);
            assert!((0.0..=1.0).contains(&alpha), "{alpha}");
        }
    }
}
