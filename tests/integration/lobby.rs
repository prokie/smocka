//! Lobby players are spawned by the host, replicated to everyone, and removed when their client leaves.

use std::time::Duration;

use bevy::prelude::Command;
use smocka::app_state::AppState;
use smocka::players::PickColor;
use smocka::profile::LocalProfile;

use crate::common::{TestApp, connected_pair, free_addr, update_until, update_until_within};

#[test]
fn host_has_a_player_for_itself() {
    let mut host = TestApp::host(free_addr());

    update_until(&mut [&mut host], |apps| apps[0].players() == 1);
}

#[test]
fn joiner_sees_both_players() {
    let (mut host, mut client, _) = connected_pair();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].players() == 2 && apps[1].players() == 2
    });
}

#[test]
fn players_are_replicated_to_every_client() {
    let (mut host, mut first, addr) = connected_pair();
    let mut second = TestApp::join(addr);

    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| app.players() == 3)
    });
}

#[test]
fn leaving_removes_the_player() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);

    client.disconnect();

    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 1);
}

#[test]
fn players_are_numbered_in_joining_order_on_every_peer() {
    let (mut host, mut first, addr) = connected_pair();
    update_until(&mut [&mut host, &mut first], |apps| apps[0].players() == 2);
    let mut second = TestApp::join(addr);

    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| app.player_numbers() == [1, 2, 3])
    });
}

#[test]
fn every_peer_finds_its_own_player() {
    let (mut host, mut client, _) = connected_pair();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].own_player_number() == Some(1) && apps[1].own_player_number() == Some(2)
    });
}

#[test]
fn everyone_left_behind_is_told_who_left() {
    let (mut host, mut first, addr) = connected_pair();
    update_until(&mut [&mut host, &mut first], |apps| apps[0].players() == 2);
    let mut second = TestApp::join(addr);
    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps.iter_mut().all(|app| app.players() == 3)
    });

    first.leave();

    update_until(&mut [&mut host, &mut first, &mut second], |apps| {
        apps[0].departures() == [2] && apps[2].departures() == [2]
    });
    assert_eq!(first.departures(), Vec::<u32>::new());
}

#[test]
fn leaving_is_noticed_without_waiting_for_a_timeout() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);

    client.leave();

    // The connection times out after 3 seconds; a goodbye packet is much faster.
    update_until_within(
        &mut [&mut host, &mut client],
        Duration::from_secs(1),
        |apps| apps[0].departures() == [2],
    );
}

#[test]
fn the_host_ending_the_session_is_not_a_player_leaving() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| apps[1].players() == 2);

    host.leave();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps[1].app_state() == AppState::MainMenu && apps[1].players() == 0
    });
    assert_eq!(client.departures(), Vec::<u32>::new());
    assert_eq!(host.departures(), Vec::<u32>::new());
}

/// Host as `host_name` and join as `joiner_name`, and wait until both see both players.
fn named_pair(host_name: &str, joiner_name: &str) -> (TestApp, TestApp) {
    let addr = free_addr();
    let mut host = TestApp::new();
    host.0.world_mut().insert_resource(LocalProfile {
        name: host_name.into(),
    });
    smocka::network::HostSession { addr }.apply(host.0.world_mut());
    let mut client = TestApp::join_as(addr, joiner_name);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| app.players() == 2)
    });
    (host, client)
}

#[test]
fn everyone_sees_the_names_players_picked() {
    let (mut host, mut client) = named_pair("Ada", "Bo");
    for app in [&mut host, &mut client] {
        let names: Vec<String> = app.player_list().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Ada", "Bo"]);
    }
}

#[test]
fn players_without_a_name_are_numbered() {
    let (_host, mut client) = named_pair("", "  ");
    let names: Vec<String> = client.player_list().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["Player 1", "Player 2"]);
}

#[test]
fn new_players_get_different_colours() {
    let (mut host, _client) = named_pair("Ada", "Bo");
    let colors: Vec<u8> = host.player_list().into_iter().map(|p| p.color).collect();
    assert_eq!(colors, [0, 1]);
}

#[test]
fn a_joiner_picks_a_free_colour_and_everyone_sees_it() {
    let (mut host, mut client) = named_pair("Ada", "Bo");

    PickColor(5).apply(client.0.world_mut());

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.player_named("Bo").is_some_and(|bo| bo.color == 5))
    });
}

#[test]
fn the_host_picks_a_colour_and_everyone_sees_it() {
    let (mut host, mut client) = named_pair("Ada", "Bo");

    PickColor(3).apply(host.0.world_mut());

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.player_named("Ada").is_some_and(|ada| ada.color == 3))
    });
}

#[test]
fn a_colour_someone_has_cant_be_picked() {
    let (mut host, mut client) = named_pair("Ada", "Bo");

    // The host has colour 0. Ask for it, then for a free one: only the second is applied, and
    // messages arrive in order, so once Bo has colour 4 the first request was refused.
    PickColor(0).apply(client.0.world_mut());
    PickColor(4).apply(client.0.world_mut());

    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].player_named("Bo").is_some_and(|bo| bo.color == 4)
    });
    assert_eq!(host.player_named("Ada").unwrap().color, 0);
}
