use bevy::prelude::*;
use lightyear::prelude::*;

/// What this app is doing. Local to each peer and never replicated.
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

/// Why the last session ended or failed to start, if it wasn't this player's choice. Shown in the
/// main menu; cleared when a new session starts.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct SessionEndReason(pub Option<String>);

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

fn enter_session_when_connected(
    trigger: On<Add<Connected>>,
    clients: Query<(), With<Client>>,
    mut next: ResMut<NextState<AppState>>,
) {
    if clients.contains(trigger.entity) {
        info!("Connected: entering the session");
        next.set(AppState::InSession);
    }
}

/// A joining client is created already [`Disconnected`] while the app is still in the menu; that
/// is ignored.
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
    // The menu only shows a friendly summary, so keep Lightyear's own reason in the log.
    info!(
        "Disconnected while {state:?}: {:?}. Back to the menu",
        disconnected.reason
    );
    reason.0 = describe(&disconnected.reason, state);
    next.set(AppState::MainMenu);
}

/// `None` if this player chose to leave.
fn describe(reason: &DisconnectedReason, state: AppState) -> Option<String> {
    let text = match (reason, state) {
        (DisconnectedReason::UserRequested(_), _) => return None,
        (_, AppState::Connecting) => {
            "Couldn't connect to the host. Check the address, and that you both have the same version."
        }
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
            Some(
                "Couldn't connect to the host. Check the address, and that you both have the same version."
            )
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
