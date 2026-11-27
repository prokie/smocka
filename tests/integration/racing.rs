//! Racing: every player gets a car, each player's driving shows up on every peer, cars bump each
//! other, the host counts laps, and the race ends with a result once the cars finish.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::PeerId;
use lightyear::prelude::input::native::ActionState;
use smocka::config::minigames::racing::LAPS;
use smocka::controls::{Control, ControlAction, Controls};
use smocka::minigames::Minigame;
use smocka::minigames::racing::RacingInput;
use smocka::minigames::racing::bumps::contact;
use smocka::minigames::racing::car::{Car, CarState};
use smocka::minigames::racing::progress::RaceProgress;
use smocka::minigames::racing::race::{ChangeLaps, RaceLaps, RaceRules, RaceStatus};
use smocka::minigames::racing::track::{CHECKPOINTS, Surface, surface_at};
use smocka::session::{MinigameResult, SessionPhase};

use crate::common::{TestApp, connected_pair, update_for, update_until};

/// The state of the car whose driver matches `is_driver`, if this app has it yet.
fn car(app: &mut TestApp, is_driver: impl Fn(PeerId) -> bool) -> Option<CarState> {
    app.0
        .world_mut()
        .query::<(&Car, &CarState)>()
        .iter(app.0.world())
        .find(|(car, _)| is_driver(car.owner))
        .map(|(_, state)| *state)
}

/// How far the host's and the joiner's cars overlap on this app: 0 if they don't, or if it can't
/// see both yet.
fn overlap(app: &mut TestApp) -> f32 {
    match (car(app, hosts), car(app, joiners)) {
        (Some(host), Some(joiner)) => contact(host, joiner).map_or(0.0, |contact| contact.depth),
        _ => 0.0,
    }
}

/// The input this app has for the car whose driver matches `is_driver`, if it has any.
fn input(app: &mut TestApp, is_driver: impl Fn(PeerId) -> bool) -> Option<RacingInput> {
    app.0
        .world_mut()
        .query::<(&Car, &ActionState<RacingInput>)>()
        .iter(app.0.world())
        .find(|(car, _)| is_driver(car.owner))
        .map(|(_, input)| input.0)
}

/// The progress of the car whose driver matches `is_driver`, if this app has it yet.
fn progress(app: &mut TestApp, is_driver: impl Fn(PeerId) -> bool) -> Option<RaceProgress> {
    app.0
        .world_mut()
        .query::<(&Car, &RaceProgress)>()
        .iter(app.0.world())
        .find(|(car, _)| is_driver(car.owner))
        .map(|(_, progress)| *progress)
}

/// Host only: park the car whose driver matches `is_driver` at `position`.
fn teleport(app: &mut TestApp, is_driver: impl Fn(PeerId) -> bool, position: Vec2) {
    let world = app.0.world_mut();
    let mut cars = world.query::<(&Car, &mut CarState)>();
    for (car, mut state) in cars.iter_mut(world) {
        if is_driver(car.owner) {
            *state = CarState::parked(position, state.heading);
        }
    }
}

/// Host only: put the car whose driver matches `is_driver` on its last lap, and over the line.
fn finish(app: &mut TestApp, is_driver: impl Fn(PeerId) -> bool + Copy) {
    let world = app.0.world_mut();
    let mut cars = world.query::<(&Car, &mut RaceProgress)>();
    for (car, mut progress) in cars.iter_mut(world) {
        if is_driver(car.owner) {
            *progress = RaceProgress {
                next_checkpoint: 0,
                laps: LAPS - 1,
                place: None,
            };
        }
    }
    teleport(app, is_driver, CHECKPOINTS[0].center());
}

/// This app's race status, if it has one yet.
fn status(app: &mut TestApp) -> Option<RaceStatus> {
    app.0
        .world_mut()
        .query::<&RaceStatus>()
        .iter(app.0.world())
        .next()
        .copied()
}

/// How many laps this app has been told the race has, if it knows yet.
fn race_laps(app: &mut TestApp) -> Option<u32> {
    app.0
        .world_mut()
        .query::<&RaceLaps>()
        .iter(app.0.world())
        .next()
        .map(|laps| laps.0)
}

/// The result this app is showing, if any.
fn result(app: &mut TestApp) -> Option<MinigameResult> {
    app.0
        .world_mut()
        .query::<&MinigameResult>()
        .iter(app.0.world())
        .next()
        .cloned()
}

/// The result's order, as "1st Player 1", "2nd Player 2 (DNF)", ….
fn order(result: &MinigameResult) -> Vec<String> {
    result.placements.iter().map(|p| p.describe()).collect()
}

/// How many cars this app can see.
fn cars(app: &mut TestApp) -> usize {
    app.0
        .world_mut()
        .query_filtered::<(), With<Car>>()
        .iter(app.0.world())
        .count()
}

/// The host's own player has a local peer id; the joiner's comes from netcode.
fn hosts(owner: PeerId) -> bool {
    matches!(owner, PeerId::Local(_))
}

fn joiners(owner: PeerId) -> bool {
    matches!(owner, PeerId::Netcode(_))
}

fn hold(app: &mut TestApp, key: KeyCode) {
    app.0
        .world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
}

/// Connect a host and a joiner, start a race with no countdown, and wait until both see both cars.
fn race_pair() -> (TestApp, TestApp) {
    race_pair_with(RaceRules {
        countdown: Duration::ZERO,
        ..default()
    })
}

/// [`race_pair`], with the host using `rules`.
fn race_pair_with(rules: RaceRules) -> (TestApp, TestApp) {
    let (host, client, _) = connected_pair();
    start_race(host, client, rules)
}

/// [`race_pair`], with the joiner playing with `controls`.
fn race_pair_with_joiner_controls(controls: Controls) -> (TestApp, TestApp) {
    let (host, mut client, _) = connected_pair();
    client.0.world_mut().insert_resource(controls);
    start_race(
        host,
        client,
        RaceRules {
            countdown: Duration::ZERO,
            ..default()
        },
    )
}

/// Start a race between a connected host and joiner, the host using `rules`, and wait until both
/// see both cars.
fn start_race(mut host: TestApp, mut client: TestApp, rules: RaceRules) -> (TestApp, TestApp) {
    host.0.world_mut().insert_resource(rules);
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);
    host.start_minigame(Minigame::Racing);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            app.phase() == Some(SessionPhase::Racing)
                && car(app, hosts).is_some()
                && car(app, joiners).is_some()
        })
    });
    (host, client)
}

#[test]
fn every_player_gets_one_car_on_every_peer() {
    let (mut host, mut client) = race_pair();
    assert_eq!(cars(&mut host), 2);
    assert_eq!(cars(&mut client), 2);
}

#[test]
fn cars_start_on_the_road() {
    let (mut host, mut client) = race_pair();
    for app in [&mut host, &mut client] {
        for driver in [hosts, joiners] {
            let position = car(app, driver).unwrap().position;
            assert_eq!(surface_at(position), Surface::Road, "{position}");
        }
    }
}

#[test]
fn joiner_drives_their_car_on_every_peer() {
    let (mut host, mut client) = race_pair();
    let start = car(&mut host, joiners).unwrap().position;

    hold(&mut client, KeyCode::KeyW);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| car(app, joiners).is_some_and(|car| car.position.x > start.x + 20.0))
    });
    // Nobody touched the host's car.
    assert_eq!(car(&mut host, hosts).unwrap().speed, 0.0);
}

#[test]
fn a_joiner_drives_with_the_controls_they_picked() {
    let mut controls = Controls::default();
    controls.bind(ControlAction::Throttle, 0, Control::Key(KeyCode::KeyI));
    let (mut host, mut client) = race_pair_with_joiner_controls(controls);
    let start = car(&mut host, joiners).unwrap().position;

    // W isn't the throttle any more. The joiner's own car would move at once (it's predicted),
    // long before the host sees it.
    hold(&mut client, KeyCode::KeyW);
    update_for(&mut [&mut host, &mut client], Duration::from_millis(100));
    assert_eq!(car(&mut client, joiners).unwrap().speed, 0.0);

    hold(&mut client, KeyCode::KeyI);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| car(app, joiners).is_some_and(|car| car.position.x > start.x + 20.0))
    });
}

#[test]
fn a_joiner_drives_and_steers_with_a_gamepad_on_every_peer() {
    let (mut host, mut client) = race_pair();
    // The default gamepad controls: the right trigger accelerates, the left stick steers.
    let mut gamepad = Gamepad::default();
    gamepad.digital_mut().press(GamepadButton::RightTrigger2);
    gamepad.analog_mut().set(GamepadButton::RightTrigger2, 1.0);
    gamepad.analog_mut().set(GamepadAxis::LeftStickX, -0.8);
    client.0.world_mut().spawn(gamepad);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| car(app, joiners).is_some_and(|car| car.speed > 50.0 && car.heading > 0.3))
    });
}

#[test]
fn a_trigger_pressed_only_a_little_doesnt_accelerate() {
    let (mut host, mut client) = race_pair();
    let mut gamepad = Gamepad::default();
    gamepad.analog_mut().set(GamepadButton::RightTrigger2, 0.2);
    client.0.world_mut().spawn(gamepad);

    // The joiner's own car would move at once (it's predicted), long before the host sees it.
    update_for(&mut [&mut host, &mut client], Duration::from_millis(100));
    assert_eq!(car(&mut client, joiners).unwrap().speed, 0.0);
}

#[test]
fn host_steers_their_car_on_every_peer() {
    let (mut host, mut client) = race_pair();

    hold(&mut host, KeyCode::KeyW);
    hold(&mut host, KeyCode::KeyA);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| car(app, hosts).is_some_and(|car| car.heading > 0.5))
    });
}

/// Host only: park the car whose driver matches `is_driver` at `position`, and wait until every
/// app sees it there.
fn park(apps: &mut [&mut TestApp], is_driver: impl Fn(PeerId) -> bool + Copy, position: Vec2) {
    teleport(apps[0], is_driver, position);
    update_until(apps, |apps| {
        apps.iter_mut().all(|app| {
            car(app, is_driver).is_some_and(|car| car.position.distance(position) < 0.01)
        })
    });
}

/// Update `apps` until the car whose driver matches `is_driver` is past `x` on all of them,
/// checking on every update that no app shows the two cars inside each other.
fn push_past(apps: &mut [&mut TestApp], is_driver: impl Fn(PeerId) -> bool + Copy, x: f32) {
    update_until(apps, |apps| {
        for app in apps.iter_mut() {
            let overlap = overlap(app);
            assert!(overlap < 1.0, "the cars overlap by {overlap}");
        }
        apps.iter_mut()
            .all(|app| car(app, is_driver).is_some_and(|car| car.position.x > x))
    });
}

#[test]
fn the_joiner_bumps_the_hosts_car_along_on_every_peer() {
    let (mut host, mut client) = race_pair();
    // Park the host's car in the joiner's lane, a little way ahead of it.
    let ahead = car(&mut host, joiners).unwrap().position + Vec2::new(40.0, 0.0);
    park(&mut [&mut host, &mut client], hosts, ahead);

    hold(&mut client, KeyCode::KeyW);

    push_past(&mut [&mut host, &mut client], hosts, ahead.x + 10.0);
}

#[test]
fn the_host_bumps_the_joiners_car_along_on_every_peer() {
    let (mut host, mut client) = race_pair();
    // Park the host's car in the joiner's lane, a little way behind it.
    let joiner = car(&mut host, joiners).unwrap().position;
    park(
        &mut [&mut host, &mut client],
        hosts,
        joiner - Vec2::new(40.0, 0.0),
    );

    hold(&mut host, KeyCode::KeyW);

    push_past(&mut [&mut host, &mut client], joiners, joiner.x + 10.0);
}

#[test]
fn joiners_predict_the_hosts_car_from_the_hosts_input() {
    let (mut host, mut client) = race_pair();

    hold(&mut host, KeyCode::KeyW);

    update_until(&mut [&mut host, &mut client], |apps| {
        input(apps[1], hosts).is_some_and(|input| input.throttle)
    });
}

#[test]
fn cars_are_removed_when_the_race_ends() {
    let (mut host, mut client) = race_pair();

    host.leave();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| cars(app) == 0)
    });
}

#[test]
fn every_car_starts_with_no_laps_on_every_peer() {
    let (mut host, mut client) = race_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            progress(app, hosts) == Some(RaceProgress::on_grid())
                && progress(app, joiners) == Some(RaceProgress::on_grid())
        })
    });
}

#[test]
fn the_host_counts_a_lap_and_every_peer_sees_it() {
    let (mut host, mut client) = race_pair();

    // Take the host's car round the checkpoints in order, ending on the line.
    for checkpoint in [1, 2, 3, 0] {
        teleport(&mut host, hosts, CHECKPOINTS[checkpoint].center());
        update_until(&mut [&mut host, &mut client], |apps| {
            progress(apps[0], hosts).is_some_and(|progress| {
                progress.next_checkpoint == (checkpoint + 1) % CHECKPOINTS.len()
            })
        });
    }

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| progress(app, hosts).is_some_and(|progress| progress.laps == 1))
    });
    // The joiner's car stayed on the grid.
    assert_eq!(progress(&mut host, joiners), Some(RaceProgress::on_grid()));
}

#[test]
fn cars_wait_for_the_countdown() {
    let (mut host, mut client) = race_pair_with(RaceRules {
        countdown: Duration::from_secs(1),
        ..default()
    });
    hold(&mut client, KeyCode::KeyW);

    // The joiner holds the throttle, but nothing moves until the countdown is over.
    for _ in 0..20 {
        host.0.update();
        client.0.update();
    }
    assert!(matches!(status(&mut host), Some(RaceStatus::Countdown(_))));
    assert_eq!(car(&mut host, joiners).unwrap().speed, 0.0);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            status(app) == Some(RaceStatus::Racing)
                && car(app, joiners).is_some_and(|car| car.speed > 0.0)
        })
    });
}

#[test]
fn the_first_car_over_the_line_wins_on_every_peer() {
    let (mut host, mut client) = race_pair();

    finish(&mut host, hosts);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| progress(app, hosts).is_some_and(|progress| progress.place == Some(1)))
    });
    finish(&mut host, joiners);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.phase() == Some(SessionPhase::Results) && result(app).is_some())
    });
    for app in [&mut host, &mut client] {
        let result = result(app).unwrap();
        assert!(hosts(result.placements[0].player), "{result:?}");
        assert_eq!(order(&result), ["1st Player 1", "2nd Player 2"]);
        // Both drove one lap, from the same start: the host crossed first.
        assert!(
            result.summary.starts_with("Fastest lap: Player 1, "),
            "{result:?}"
        );
    }
}

#[test]
fn the_race_ends_a_while_after_the_first_finish() {
    let (mut host, mut client) = race_pair_with(RaceRules {
        countdown: Duration::ZERO,
        finish_grace: Duration::from_secs(1),
        ..default()
    });

    finish(&mut host, joiners);

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.phase() == Some(SessionPhase::Results) && result(app).is_some())
    });
    let result = result(&mut client).unwrap();
    assert!(joiners(result.placements[0].player), "{result:?}");
    assert_eq!(order(&result), ["1st Player 2", "2nd Player 1 (DNF)"]);
}

#[test]
fn the_race_ends_when_the_only_other_player_leaves_after_you_finish() {
    let (mut host, mut client) = race_pair();

    finish(&mut host, hosts);
    update_until(&mut [&mut host, &mut client], |apps| {
        progress(apps[0], hosts).is_some_and(|progress| progress.place == Some(1))
    });
    client.leave();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].phase() == Some(SessionPhase::Results)
    });
    // The joiner's car left with them, so only the host is placed.
    let result = result(&mut host).unwrap();
    assert!(hosts(result.placements[0].player), "{result:?}");
    assert_eq!(order(&result), ["1st Player 1"]);
}

#[test]
fn the_race_is_abandoned_when_the_only_other_player_leaves_before_anyone_finishes() {
    let (mut host, mut client) = race_pair();

    client.leave();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].phase() == Some(SessionPhase::Lobby) && cars(apps[0]) == 0
    });
    assert!(result(&mut host).is_none());
}

#[test]
fn the_race_goes_on_while_two_cars_are_left() {
    let (mut host, mut first, addr) = connected_pair();
    let mut second = TestApp::join(addr);
    host.0.world_mut().insert_resource(RaceRules {
        countdown: Duration::ZERO,
        ..default()
    });
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| app.players() == 3)
    });
    host.start_minigame(Minigame::Racing);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut()
            .all(|app| app.phase() == Some(SessionPhase::Racing) && cars(app) == 3)
    });

    second.leave();
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        cars(apps[0]) == 2 && cars(apps[1]) == 2
    });
    for _ in 0..20 {
        host.0.update();
        first.0.update();
    }
    assert_eq!(host.phase(), Some(SessionPhase::Racing));
}

#[test]
fn cars_carry_their_drivers_name_and_colour() {
    let (_host, mut client) = race_pair();
    let players = client.player_list();
    let world = client.0.world_mut();
    let cars: Vec<Car> = world.query::<&Car>().iter(world).cloned().collect();
    for player in players {
        let car = cars.iter().find(|car| car.owner == player.id).unwrap();
        assert_eq!((&car.name, car.color), (&player.name, player.color));
    }
}

#[test]
fn the_host_picks_how_many_laps_the_race_has() {
    let (mut host, mut client) = race_pair_with(RaceRules {
        countdown: Duration::ZERO,
        laps: 1,
        ..default()
    });
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| race_laps(app) == Some(1))
    });

    // One lap: round every checkpoint in order, ending on the line.
    for checkpoint in [1, 2, 3, 0] {
        teleport(&mut host, hosts, CHECKPOINTS[checkpoint].center());
        update_until(&mut [&mut host, &mut client], |apps| {
            progress(apps[0], hosts).is_some_and(|progress| {
                progress.next_checkpoint == (checkpoint + 1) % CHECKPOINTS.len()
            })
        });
    }
    let progress = progress(&mut host, hosts).unwrap();
    assert_eq!((progress.laps, progress.place), (1, Some(1)));
}

#[test]
fn joiners_see_the_hosts_lap_pick_in_the_lobby() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].players() == 2 && race_laps(apps[1]) == Some(LAPS)
    });

    host.0.world_mut().commands().queue(ChangeLaps(2));
    host.0.world_mut().flush();
    update_until(&mut [&mut host, &mut client], |apps| {
        race_laps(apps[1]) == Some(LAPS + 2)
    });
    assert_eq!(client.phase(), Some(SessionPhase::Lobby));
}
