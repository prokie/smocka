//! Smooth movement on screen between fixed ticks.
//!
//! Minigames simulate in `FixedUpdate` at [`TICK_RATE`](crate::config::network::TICK_RATE)
//! (128 Hz), but screens refresh at their own rate, so a frame can see no tick, one, or several.
//! Drawing things where their latest tick left them makes movement uneven: on a 144 Hz screen about
//! one frame in nine repeats the one before, and the next jumps twice as far.
//!
//! So things this app simulates are drawn part of the way between their last two ticks, as far as
//! the clock is into the next tick (`Time<Fixed>::overstep_fraction`). They're drawn one tick
//! (7.8 ms) late, but moving evenly.
//!
//! Only `Transform` is smoothed: the simulated component keeps its tick value, so the simulation,
//! replication and tests never see a blend. (Lightyear's `FrameInterpolationPlugin` writes the
//! blend into the component itself, which marks it changed every frame, so the host would re-send
//! idle cars and paddles.)
//!
//! Interpolated entities (a spectator's cars, the other player's paddle, a joiner's ball) are
//! already moved every frame by Lightyear, so they're drawn as they are.

use bevy::prelude::*;
use lightyear::prelude::Interpolated;

/// The value `C` had when the latest fixed tick began: where the tick before left it. Kept by
/// [`remember_previous_tick`] on the entities this app simulates, and only those.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PreviousTick<C: Send + Sync + 'static>(pub C);

/// What a system drawing a `C` smoothly reads: the latest tick's value, its [`PreviousTick`] if it
/// has one, and the `Transform` to draw it with. Pass the first two to [`smoothed`].
pub type Smoothed<C> = (
    &'static C,
    Option<&'static PreviousTick<C>>,
    &'static mut Transform,
);

/// Simulated entities with a `C` that [`remember_previous_tick`] hasn't seen yet.
type Unremembered<C> = (Without<PreviousTick<C>>, Without<Interpolated>);

/// Keeps [`PreviousTick<C>`] up to date on every entity with `C` that isn't interpolated. Add it
/// to `FixedFirst`: it then also runs at the start of each tick a rollback replays, so after a
/// rollback [`PreviousTick`] holds the replayed value.
pub fn remember_previous_tick<C: Component + Clone>(
    mut remembered: Query<(&C, &mut PreviousTick<C>), Without<Interpolated>>,
    new: Query<(Entity, &C), Unremembered<C>>,
    interpolated: Query<Entity, (With<PreviousTick<C>>, With<Interpolated>)>,
    mut commands: Commands,
) {
    for (value, mut previous) in &mut remembered {
        previous.0.clone_from(value);
    }
    for (entity, value) in &new {
        commands.entity(entity).insert(PreviousTick(value.clone()));
    }
    for entity in &interpolated {
        commands.entity(entity).remove::<PreviousTick<C>>();
    }
}

/// What to draw this frame for something whose latest tick left it at `current`. With a
/// [`PreviousTick`], it's that far from `previous` to `current`: `overstep` is how far the clock
/// is into the next tick, from 0 to 1. Without one (an interpolated entity, or one too new to
/// have had a tick), it's `current`.
pub fn smoothed<C: Ease + Clone + Send + Sync + 'static>(
    current: &C,
    previous: Option<&PreviousTick<C>>,
    overstep: f32,
) -> C {
    match previous {
        Some(PreviousTick(previous)) => {
            C::interpolating_curve_unbounded(previous.clone(), current.clone())
                .sample_unchecked(overstep.clamp(0.0, 1.0))
        }
        None => current.clone(),
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use lightyear::prelude::Interpolated;

    use super::{PreviousTick, remember_previous_tick, smoothed};

    #[derive(Component, Clone, Copy, Debug, PartialEq)]
    struct Spot(f32);

    impl Ease for Spot {
        fn interpolating_curve_unbounded(start: Self, end: Self) -> impl Curve<Self> {
            FunctionCurve::new(Interval::UNIT, move |t| Spot(start.0.lerp(end.0, t)))
        }
    }

    #[test]
    fn draws_between_the_last_two_ticks() {
        let previous = PreviousTick(Spot(10.0));
        assert_eq!(smoothed(&Spot(20.0), Some(&previous), 0.0), Spot(10.0));
        assert_eq!(smoothed(&Spot(20.0), Some(&previous), 0.25), Spot(12.5));
        assert_eq!(smoothed(&Spot(20.0), Some(&previous), 1.0), Spot(20.0));
        // Never past the latest tick, or behind the one before.
        assert_eq!(smoothed(&Spot(20.0), Some(&previous), 1.5), Spot(20.0));
        assert_eq!(smoothed(&Spot(20.0), Some(&previous), -0.5), Spot(10.0));
    }

    #[test]
    fn without_a_previous_tick_draws_the_latest() {
        assert_eq!(smoothed(&Spot(20.0), None, 0.5), Spot(20.0));
    }

    #[test]
    fn remembers_each_tick_for_simulated_entities_only() {
        let mut world = World::new();
        let simulated = world.spawn(Spot(1.0)).id();
        let interpolated = world.spawn((Spot(1.0), Interpolated)).id();
        world
            .run_system_once(remember_previous_tick::<Spot>)
            .unwrap();
        assert_eq!(
            world.get::<PreviousTick<Spot>>(simulated),
            Some(&PreviousTick(Spot(1.0)))
        );
        assert_eq!(world.get::<PreviousTick<Spot>>(interpolated), None);

        world.get_mut::<Spot>(simulated).unwrap().0 = 2.0;
        world
            .run_system_once(remember_previous_tick::<Spot>)
            .unwrap();
        assert_eq!(
            world.get::<PreviousTick<Spot>>(simulated),
            Some(&PreviousTick(Spot(2.0)))
        );
    }

    #[test]
    fn forgets_the_previous_tick_once_interpolated() {
        let mut world = World::new();
        let entity = world
            .spawn((Spot(1.0), PreviousTick(Spot(0.0)), Interpolated))
            .id();
        world
            .run_system_once(remember_previous_tick::<Spot>)
            .unwrap();
        assert_eq!(world.get::<PreviousTick<Spot>>(entity), None);
    }
}
