//! Shootout: every player gets a gunner on its own spawn point, on every peer, and the shootout is
//! abandoned when the other players leave.

use bevy::prelude::*;
use lightyear::prelude::{Interpolated, PeerId, Predicted};
use smocka::minigames::Minigame;
use smocka::minigames::shootout::config::SPAWNS;
use smocka::minigames::shootout::gunner::{Gunner, GunnerState};
use smocka::session::SessionPhase;

use crate::common::{TestApp, connected_pair, update_until};

/// Each gunner this app can see: its player, where it is, and whether it's predicted here
/// (rather than interpolated).
fn gunners(app: &mut TestApp) -> Vec<(PeerId, Vec2, bool)> {
    app.0
        .world_mut()
        .query::<(&Gunner, &GunnerState, Has<Predicted>, Has<Interpolated>)>()
        .iter(app.0.world())
        .map(|(gunner, state, predicted, _)| (gunner.owner, state.position, predicted))
        .collect()
}

/// The host's own player has a local peer id; the joiner's comes from netcode.
fn joiners(owner: PeerId) -> bool {
    matches!(owner, PeerId::Netcode(_))
}

/// Connect a host and a joiner, start a shootout, and wait until both see both gunners.
fn shootout_pair() -> (TestApp, TestApp) {
    let (mut host, mut client, _) = connected_pair();
    update_until(&mut [&mut host, &mut client], |apps| apps[0].players() == 2);
    host.start_minigame(Minigame::Shootout);
    update_until(&mut [&mut host, &mut client], |apps| {
        apps.iter_mut().all(|app| {
            app.phase() == Some(SessionPhase::Playing(Minigame::Shootout))
                && gunners(app).len() == 2
        })
    });
    (host, client)
}

#[test]
fn every_player_gets_a_gunner_on_its_own_spawn_point_on_every_peer() {
    let (mut host, mut client) = shootout_pair();
    for app in [&mut host, &mut client] {
        let gunners = gunners(app);
        let positions: Vec<Vec2> = gunners.iter().map(|(_, position, _)| *position).collect();
        assert!(
            positions.iter().all(|position| SPAWNS.contains(position)),
            "{positions:?}"
        );
        assert_ne!(positions[0], positions[1]);
    }
    // The joiner predicts their own gunner and interpolates the host's.
    for (owner, _, predicted) in gunners(&mut client) {
        assert_eq!(predicted, joiners(owner), "{owner:?}");
    }
}

#[test]
fn the_shootout_is_abandoned_when_the_other_player_leaves() {
    let (mut host, mut client) = shootout_pair();
    client.leave();
    update_until(&mut [&mut host, &mut client], |apps| {
        apps[0].phase() == Some(SessionPhase::Lobby) && gunners(apps[0]).is_empty()
    });
}
