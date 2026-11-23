//! Lobby players are spawned by the host, replicated to everyone, and removed when their client leaves.

use std::time::Duration;

use smocka::app_state::AppState;

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
