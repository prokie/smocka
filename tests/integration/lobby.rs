//! Lobby players are spawned by the host, replicated to everyone, and removed when their client leaves.

use crate::common::{TestApp, connected_pair, free_addr, update_until};

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
