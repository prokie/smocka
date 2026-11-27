//! Moving between the menu, connecting and a session: hosting, joining, leaving, and failing to
//! connect.

use std::time::Duration;

use bevy::prelude::Command;
use smocka::app_state::AppState;
use smocka::network::{HostSession, JoinSession};
use smocka::session::SessionPhase;

use crate::common::{TestApp, connected_pair, silent_addr, update_until, update_until_within};

#[test]
fn an_app_starts_in_the_menu_without_a_session() {
    let mut app = TestApp::new();
    app.0.update();

    assert_eq!(app.app_state(), AppState::MainMenu);
    assert_eq!(app.phase(), None);
}

#[test]
fn hosting_on_a_port_another_game_holds_stays_in_the_menu() {
    let mut first = TestApp::host();
    let addr = first.host_addr();
    update_until(&mut [&mut first], |apps| {
        apps[0].app_state() == AppState::InSession
    });

    let mut second = TestApp::new();
    assert!(HostSession { addr }.apply(second.0.world_mut()).is_err());
    for _ in 0..10 {
        second.0.update();
    }

    assert_eq!(second.app_state(), AppState::MainMenu);
    assert_eq!(
        second.end_reason().as_deref(),
        Some(
            format!(
                "Port {} is in use. Is the game already hosting?",
                addr.port()
            )
            .as_str()
        )
    );
    assert_eq!(second.connected_clients(), 0);
    // The first game's session carries on.
    first.0.update();
    assert_eq!(first.app_state(), AppState::InSession);
}

#[test]
fn hosting_enters_a_session_in_the_lobby() {
    let mut host = TestApp::host();

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
    let mut host = TestApp::host();
    let addr = host.host_addr();
    update_until(&mut [&mut host], |apps| {
        apps[0].app_state() == AppState::InSession && apps[0].players() == 1
    });

    host.leave();
    update_until(&mut [&mut host], |apps| {
        apps[0].app_state() == AppState::MainMenu && apps[0].players() == 0
    });

    HostSession { addr }.apply(host.0.world_mut()).unwrap();
    let mut client = TestApp::join(addr);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.app_state() == AppState::InSession && app.players() == 2)
    });
}

#[test]
fn joining_where_nobody_hosts_goes_back_to_the_menu_with_an_error() {
    // Kept until the test ends, so nobody can start hosting at `addr` meanwhile.
    let (_silent, addr) = silent_addr();
    let mut client = TestApp::join(addr);
    update_until(&mut [&mut client], |apps| {
        apps[0].app_state() == AppState::Connecting
    });

    update_until_within(&mut [&mut client], Duration::from_secs(20), |apps| {
        apps[0].app_state() == AppState::MainMenu
    });
    assert_eq!(
        client.end_reason().as_deref(),
        Some(
            "Couldn't connect to the host. Check the address, and that you both have the same version."
        )
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

    let mut new_host = TestApp::host();
    let addr = new_host.host_addr();
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
    let first_addr = host.remote_client_addrs();

    client.leave();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].players() == 1 && apps[1].app_state() == AppState::MainMenu
    });

    JoinSession { addr }.apply(client.0.world_mut()).unwrap();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut()
            .all(|app| app.app_state() == AppState::InSession && app.players() == 2)
    });
    // A fresh socket, so the host doesn't mistake the new connection for the old one.
    assert_ne!(host.remote_client_addrs(), first_addr);
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

    let addr = player.start_hosting();
    let mut joiner = TestApp::join(addr);
    update_until(&mut [&mut player, &mut joiner], |apps| {
        apps.iter_mut()
            .all(|app| app.app_state() == AppState::InSession && app.players() == 2)
    });
}

#[test]
fn a_former_host_can_join() {
    let mut player = TestApp::host();
    update_until(&mut [&mut player], |apps| {
        apps[0].app_state() == AppState::InSession
    });
    player.leave();
    update_until(&mut [&mut player], |apps| {
        apps[0].app_state() == AppState::MainMenu
    });

    let mut host = TestApp::host();
    let addr = host.host_addr();
    JoinSession { addr }.apply(player.0.world_mut()).unwrap();
    update_until(&mut [&mut host, &mut player], |apps| {
        apps.iter_mut()
            .all(|app| app.app_state() == AppState::InSession && app.players() == 2)
    });
}
