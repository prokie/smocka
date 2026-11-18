//! What this app is doing: sitting in the main menu, connecting, or in a session.
//!
//! [`AppState`] is local to each peer and never replicated: only this machine decides whether it
//! is in the menu. Inside a session, the host-decided [`SessionPhase`](crate::session::SessionPhase)
//! takes over as a sub-state of [`AppState::InSession`].
//!
//! The network drives the transitions:
//!
//! - [`HostSession`](crate::network::HostSession) and [`JoinSession`](crate::network::JoinSession)
//!   go to [`AppState::Connecting`].
//! - When this app's client connects, it goes to [`AppState::InSession`].
//! - When this app's client disconnects (the host left, the connection was lost or never came up),
//!   or after [`LeaveSession`](crate::network::LeaveSession), it goes back to [`AppState::MainMenu`].

use bevy::prelude::*;
use lightyear::prelude::*;

/// What this app is doing. Local to each peer.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    /// Choosing whether to host or join.
    #[default]
    MainMenu,
    /// Waiting for this app's client to connect to a session.
    Connecting,
    /// Connected to a session. See [`SessionPhase`](crate::session::SessionPhase).
    InSession,
}

/// Why the last session ended or failed to start, if it wasn't this player's choice.
///
/// Set when the app is sent back to the menu by a disconnect, so the menu can say why.
/// Cleared when a new session starts.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct SessionEndReason(pub Option<String>);

/// Registers [`AppState`] and moves between its states as this app's client connects and
/// disconnects.
///
/// Must be added before [`SessionPlugin`](crate::session::SessionPlugin), whose state is a
/// sub-state of [`AppState`].
pub struct AppStatePlugin;

impl Plugin for AppStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>();
        app.init_resource::<SessionEndReason>();
        app.add_observer(enter_session_when_connected);
        app.add_observer(return_to_menu_when_disconnected);
    }
}

/// When this app's client connects, enter the session.
fn enter_session_when_connected(
    trigger: On<Add<Connected>>,
    clients: Query<(), With<Client>>,
    mut next: ResMut<NextState<AppState>>,
) {
    if clients.contains(trigger.entity) {
        next.set(AppState::InSession);
    }
}

/// When this app's client disconnects during a session (or while connecting), go back to the menu
/// and remember why.
///
/// A joining client is created already [`Disconnected`] while the app is still in the menu; that
/// is ignored, because the app hasn't left the menu yet.
fn return_to_menu_when_disconnected(
    trigger: On<Add<Disconnected>>,
    clients: Query<&Disconnected, With<Client>>,
    state: Option<Res<State<AppState>>>,
    mut next: ResMut<NextState<AppState>>,
    mut reason: ResMut<SessionEndReason>,
) {
    let Ok(disconnected) = clients.get(trigger.entity) else {
        return;
    };
    let Some(state) = state.map(|state| *state.get()) else {
        return;
    };
    if state == AppState::MainMenu {
        return;
    }
    reason.0 = describe(&disconnected.reason, state);
    next.set(AppState::MainMenu);
}

/// A short, player-facing explanation of why the connection ended while in `state`, or `None` if
/// this player chose to leave.
fn describe(reason: &DisconnectedReason, state: AppState) -> Option<String> {
    let text = match (reason, state) {
        (DisconnectedReason::UserRequested(_), _) => return None,
        (_, AppState::Connecting) => "Couldn't connect to the host.",
        (DisconnectedReason::ByPeer(_), _) => "The host ended the session.",
        _ => "The connection to the host was lost.",
    };
    Some(text.into())
}

#[cfg(test)]
mod tests {
    use super::{AppState, describe};
    use lightyear::prelude::DisconnectedReason;

    #[test]
    fn leaving_on_purpose_has_no_reason() {
        let reason = DisconnectedReason::UserRequested(None);
        assert_eq!(describe(&reason, AppState::InSession), None);
    }

    #[test]
    fn failing_while_connecting_says_so() {
        let reason = DisconnectedReason::TransportError("ConnectionRequestTimedOut".into());
        assert_eq!(
            describe(&reason, AppState::Connecting).as_deref(),
            Some("Couldn't connect to the host.")
        );
    }

    #[test]
    fn the_host_leaving_says_so() {
        let reason = DisconnectedReason::ByPeer(None);
        assert_eq!(
            describe(&reason, AppState::InSession).as_deref(),
            Some("The host ended the session.")
        );
    }
}
