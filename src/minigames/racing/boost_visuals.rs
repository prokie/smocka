//! What boosts look like: the track's boost pads, a flame behind a boosted car, and streaks of
//! wind beside a car in a slipstream (see [`slipstream`](super::slipstream)). Local to each peer,
//! from the track's data and the car states it sees, so nothing extra is sent.

use bevy::prelude::*;
use lightyear::prelude::DisableReplicateHierarchy;
use lightyear::prelude::input::native::InputMarker;

use super::PHASE;
use super::RacingInput;
use super::car::{Car, CarState};
use super::config::CAR_LENGTH;
use super::track::TrackId;

const BOOST_PAD_IMAGE: &str = "minigames/racing/boost_pad.png";
/// Points left, away from the car's nose.
const FLAME_IMAGE: &str = "minigames/racing/flame.png";
/// Streaks either side of a car pointing right, trailing off to the left.
const SLIPSTREAM_IMAGE: &str = "minigames/racing/slipstream.png";

/// The flame's size in world units. Keep `tools/make_art.py` in step.
const FLAME_SIZE: Vec2 = Vec2::new(10.0, 7.0);
/// The streaks' size in world units: wider than a car, so they run beside it. Keep
/// `tools/make_art.py` in step.
const SLIPSTREAM_SIZE: Vec2 = Vec2::new(24.0, 16.0);
/// Where the flame and the streaks are drawn, relative to a car pointing right: the flame just
/// behind its tail, the streaks along its sides and behind it, both under the car.
const FLAME_OFFSET: Vec3 = Vec3::new(-CAR_LENGTH / 2.0 - FLAME_SIZE.x / 2.0 + 1.0, 0.0, -0.1);
const SLIPSTREAM_OFFSET: Vec3 = Vec3::new(-8.0, 0.0, -0.2);
/// Above the track, under skid marks and cars.
const BOOST_PAD_Z: f32 = 0.3;
/// The flame flickers between this much shorter and longer, this many times a second.
const FLICKER: f32 = 0.2;
const FLICKER_HZ: f32 = 9.0;

/// One of the track's boost pads, drawn.
#[derive(Component)]
struct BoostPadSprite;

/// The flame behind a car, shown while it's boosted.
#[derive(Component)]
struct Flame;

/// The streaks beside a car, shown while it's in a slipstream.
#[derive(Component)]
struct SlipstreamStreaks;

/// Draws boost pads and boosted cars. Added by
/// [`RacingVisualsPlugin`](super::RacingVisualsPlugin).
pub struct BoostVisualsPlugin;

impl Plugin for BoostVisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(add_boost_sprites);
        app.add_systems(
            Update,
            (spawn_boost_pads, show_boosts, log_own_slipstream).run_if(in_state(PHASE)),
        );
    }
}

/// Draw the race's boost pads once its [`TrackId`] is known, as with the track's art.
fn spawn_boost_pads(
    track: Option<Single<&TrackId>>,
    drawn: Query<(), With<BoostPadSprite>>,
    assets: Res<AssetServer>,
    mut commands: Commands,
) {
    let Some(track) = track.map(|track| **track) else {
        return;
    };
    if !drawn.is_empty() {
        return;
    }
    let pads = track.track().boost_pads();
    debug!(
        "Drawing the {} track's {} boost pads",
        track.name(),
        pads.len()
    );
    for pad in pads {
        commands.spawn((
            BoostPadSprite,
            DespawnOnExit(PHASE),
            Sprite {
                custom_size: Some(pad.size()),
                ..Sprite::from_image(assets.load(BOOST_PAD_IMAGE))
            },
            Transform::from_translation(pad.center().extend(BOOST_PAD_Z))
                .with_rotation(Quat::from_rotation_z(pad.heading())),
        ));
    }
}

/// Give each car a flame and streaks, hidden until it's boosted or in a slipstream. Every peer
/// draws its own, so the host doesn't replicate them with the car.
fn add_boost_sprites(trigger: On<Add<Car>>, assets: Res<AssetServer>, mut commands: Commands) {
    commands.entity(trigger.entity).with_children(|car| {
        car.spawn((
            Flame,
            DisableReplicateHierarchy,
            Sprite {
                custom_size: Some(FLAME_SIZE),
                ..Sprite::from_image(assets.load(FLAME_IMAGE))
            },
            Transform::from_translation(FLAME_OFFSET),
            Visibility::Hidden,
        ));
        car.spawn((
            SlipstreamStreaks,
            DisableReplicateHierarchy,
            Sprite {
                custom_size: Some(SLIPSTREAM_SIZE),
                ..Sprite::from_image(assets.load(SLIPSTREAM_IMAGE))
            },
            Transform::from_translation(SLIPSTREAM_OFFSET),
            Visibility::Hidden,
        ));
    });
}

/// `Visible` if `shown`, otherwise hidden.
fn visibility(shown: bool) -> Visibility {
    if shown {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

/// A car's slipstream streaks, apart from its flame.
type StreaksOnly = (With<SlipstreamStreaks>, Without<Flame>);

/// Show each car's flame while it's boosted, flickering, and its streaks while it's in a
/// slipstream but not boosted.
fn show_boosts(
    time: Res<Time>,
    cars: Query<&CarState, With<Car>>,
    mut flames: Query<(&ChildOf, &mut Visibility, &mut Transform), With<Flame>>,
    mut streaks: Query<(&ChildOf, &mut Visibility), StreaksOnly>,
) {
    let flicker = 1.0 + FLICKER * (time.elapsed_secs() * FLICKER_HZ * std::f32::consts::TAU).sin();
    for (car, mut shown, mut transform) in &mut flames {
        let Ok(state) = cars.get(car.parent()) else {
            continue;
        };
        shown.set_if_neq(visibility(state.boost > 0.0));
        transform.scale.x = flicker;
    }
    for (car, mut shown) in &mut streaks {
        let Ok(state) = cars.get(car.parent()) else {
            continue;
        };
        shown.set_if_neq(visibility(state.draft > 0.0 && state.boost <= 0.0));
    }
}

/// Log when your own car gets into a slipstream, so each peer's log shows what it saw. (Boost pads
/// are logged with their sound: see [`sounds`](super::sounds).)
fn log_own_slipstream(
    own_car: Query<&CarState, With<InputMarker<RacingInput>>>,
    mut was_drafting: Local<bool>,
) {
    let drafting = own_car.single().is_ok_and(|state| state.draft > 0.0);
    if drafting && !*was_drafting {
        debug!("Your car is in a slipstream");
    }
    *was_drafting = drafting;
}
