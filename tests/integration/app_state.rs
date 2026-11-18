//! Moving between the menu, connecting and a session: hosting, joining, leaving, and failing to
//! connect.

use std::time::Duration;

use bevy::prelude::Command;
use smocka::app_state::AppState;
use smocka::network::{HostSession, JoinSession};
use smocka::session::SessionPhase;

use crate::common::{TestApp, connected_pair, free_addr, update_until, update_until_within};

#[test]
fn an_app_starts_in_the_menu_without_a_session() {
    let mut app = TestApp::new();
    app.0.update();

    assert_eq!(app.app_state(), AppState::MainMenu);
    assert_eq!(app.phase(), None);
}

#[test]
fn hosting_enters_a_session_in_the_lobby() {
    let mut host = TestApp::host(free_addr());

    update_until(&mut [&mut host], |apps| {
        apps[0].app_state() == AppState::InSession && apps[0].phase() == Some(SessionPhase::Lobby)
    });
}

#[test]
fn joining_enters_the_hosts_session() {
    let (mut host, mut client, _) = connected_pair();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter()
            .all(|app| app.app_state() == AppState::InSession)
    });
}

#[test]
fn a_joiner_who_leaves_goes_back_to_the_menu_without_an_error() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[1].app_state() == AppState::InSession
    });

    client.leave();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].connected_remote_links() == 0 && apps[1].app_state() == AppState::MainMenu
    });
    assert_eq!(client.end_reason(), None);
    assert_eq!(host.app_state(), AppState::InSession);
}

#[test]
fn when_the_host_leaves_everyone_goes_back_to_the_menu() {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter()
            .all(|app| app.app_state() == AppState::InSession)
    });

    host.leave();

    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter().all(|app| app.app_state() == AppState::MainMenu)
    });
    assert_eq!(host.end_reason(), None);
    assert_eq!(
        client.end_reason().as_deref(),
        Some("The host ended the session.")
    );
    assert_eq!(
        client.players(),
        0,
        "the joiner keeps no players from the old session"
    );
}

#[test]
fn a_host_can_leave_and_host_again_on_the_same_port() {
    let addr = free_addr();
    let mut host = TestApp::host(addr);
    update_until(&mut [&mut host], |apps| {
        apps[0].app_state() == AppState::InSession && apps[0].players() == 1
    });

    host.leave();
    update_until(&mut [&mut host], |apps| {
        apps[0].app_state() == AppState::MainMenu && apps[0].players() == 0
    });

    HostSession { addr }.apply(host.0.world_mut());
    let mut client = TestApp::join(addr);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.app_state() == AppState::InSession && app.players() == 2)
    });
}

#[test]
fn joining_where_nobody_hosts_goes_back_to_the_menu_with_an_error() {
    let mut client = TestApp::join(free_addr());
    update_until(&mut [&mut client], |apps| {
        apps[0].app_state() == AppState::Connecting
    });

    update_until_within(&mut [&mut client], Duration::from_secs(20), |apps| {
        apps[0].app_state() == AppState::MainMenu
    });
    assert_eq!(
        client.end_reason().as_deref(),
        Some("Couldn't connect to the host.")
    );
}

#[test]
fn a_joiner_can_join_a_new_session_after_the_host_left() {
    let (mut old_host, mut client, _) = connected_pair();
    update_until(&mut [&mut old_host, &mut client], |apps| {
        apps.iter()
            .all(|app| app.app_state() == AppState::InSession)
    });

    old_host.leave();
    update_until(&mut [&mut old_host, &mut client], |apps| {
        apps[1].app_state() == AppState::MainMenu
    });
    drop(old_host);

    let addr = free_addr();
    let mut new_host = TestApp::host(addr);
    JoinSession { addr }.apply(client.0.world_mut()).unwrap();
    update_until(&mut [&mut new_host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.app_state() == AppState::InSession && app.players() == 2)
    });
}

#[test]
fn a_joiner_can_leave_and_join_the_same_host_again() {
    let (mut host, mut client, addr) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);

    client.leave();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].players() == 1 && apps[1].app_state() == AppState::MainMenu
    });

    JoinSession { addr }.apply(client.0.world_mut()).unwrap();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.app_state() == AppState::InSession && app.players() == 2)
    });
}

#[test]
fn a_former_joiner_can_host() {
    let (mut old_host, mut player, _) = connected_pair();
    update_until(&mut [&mut old_host, &mut player], |apps| {
        apps[1].app_state() == AppState::InSession
    });
    player.leave();
    update_until(&mut [&mut old_host, &mut player], |apps| {
        apps[1].app_state() == AppState::MainMenu
    });

    let addr = free_addr();
    HostSession { addr }.apply(player.0.world_mut());
    let mut joiner = TestApp::join(addr);
    update_until(&mut [&mut player, &mut joiner], |apps| {
        apps.iter_mut()
            .all(|app| app.app_state() == AppState::InSession && app.players() == 2)
    });
}

#[test]
fn a_former_host_can_join() {
    let mut player = TestApp::host(free_addr());
    update_until(&mut [&mut player], |apps| {
        apps[0].app_state() == AppState::InSession
    });
    player.leave();
    update_until(&mut [&mut player], |apps| {
        apps[0].app_state() == AppState::MainMenu
    });

    let addr = free_addr();
    let mut host = TestApp::host(addr);
    JoinSession { addr }.apply(player.0.world_mut()).unwrap();
    update_until(&mut [&mut host, &mut player], |apps| {
        apps.iter_mut()
            .all(|app| app.app_state() == AppState::InSession && app.players() == 2)
    });
}
