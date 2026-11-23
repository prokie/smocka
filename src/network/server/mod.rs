use bevy::prelude::*;
use lightyear::prelude::*;

pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(handle_new_client);
    }
}

/// When a new client tries to connect to a server, an entity is created for it with the `LinkOf` component.
/// This entity represents the link between the server and that client.
///
/// You can add additional components to update the link. In this case we will add a `ReplicationSender` that
/// will enable us to replicate local entities to that client.
///
/// The host's own client is also a `LinkOf` the server, but it is skipped (it has `Client`): it
/// keeps its `HostClient` name, and Lightyear replicates to it without a `ReplicationSender`.
pub(crate) fn handle_new_client(
    trigger: On<Add<LinkOf>>,
    remote_links: Query<(), Without<Client>>,
    mut commands: Commands,
) {
    if !remote_links.contains(trigger.entity) {
        return;
    }
    commands
        .entity(trigger.entity)
        .insert((ReplicationSender, Name::from("Client")));
}
