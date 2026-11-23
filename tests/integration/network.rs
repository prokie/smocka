//! Hosting, joining and leaving over real UDP on localhost.

use std::time::Duration;

use bevy::prelude::*;
use lightyear::prelude::*;

use crate::common::{TestApp, connected_pair, free_addr, update_until, update_until_within};

#[test]
fn host_connects_its_local_client() {
    let mut host = TestApp::host(free_addr());

    update_until(&mut [&mut host], |apps| apps[0].connected_clients() == 1);
}

#[test]
fn client_joins_host() {
    let addr = free_addr();
    let mut host = TestApp::host(addr);
    let mut client = TestApp::join(addr);

    // The host sees the joiner as a remote link, and the joiner's client is connected.
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].connected_remote_links() == 1 && apps[1].connected_clients() == 1
    });
}

#[test]
fn only_remote_links_are_set_up_as_joining_clients() {
    let (mut host, _client, _) = connected_pair();
    let world = host.0.world_mut();

    let host_client: Vec<(String, bool)> = world
        .query_filtered::<(&Name, Has<ReplicationSender>), (With<Client>, With<LinkOf>)>()
        .iter(world)
        .map(|(name, sender)| (name.to_string(), sender))
        .collect();
    assert_eq!(host_client, [("HostClient".to_string(), false)]);

    let remote: Vec<(String, bool)> = world
        .query_filtered::<(&Name, Has<ReplicationSender>), (With<LinkOf>, Without<Client>)>()
        .iter(world)
        .map(|(name, sender)| (name.to_string(), sender))
        .collect();
    assert_eq!(remote, [("Client".to_string(), true)]);
}

#[test]
fn client_can_leave() {
    let (mut host, mut client, _) = connected_pair();

    client.disconnect();

    // The joiner is disconnected and the host has cleaned up its link to it.
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].connected_remote_links() == 0 && apps[1].disconnected_clients() == 1
    });
    // The host keeps running with its own local client.
    assert_eq!(
        host.connected_clients(),
        1,
        "host's own client should stay connected"
    );
}

#[test]
fn client_can_leave_and_a_new_client_can_join() {
    let (mut host, mut client, addr) = connected_pair();

    client.disconnect();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].connected_remote_links() == 0
    });
    drop(client);

    let mut rejoined = TestApp::join(addr);
    update_until(&mut [&mut host, &mut rejoined], |apps| {
        apps[0].connected_remote_links() == 1 && apps[1].connected_clients() == 1
    });
}

#[test]
fn host_drops_client_that_vanishes() {
    let (mut host, client, _) = connected_pair();

    // Simulate a crash or lost network: the client stops sending without saying goodbye.
    drop(client);

    // The host only notices once the netcode timeout expires.
    update_until_within(&mut [&mut host], Duration::from_secs(20), |apps| {
        apps[0].connected_remote_links() == 0
    });
    assert_eq!(
        host.connected_clients(),
        1,
        "host's own client should stay connected"
    );
}
