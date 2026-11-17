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
pub(crate) fn handle_new_client(trigger: On<Add<LinkOf>>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert((ReplicationSender, Name::from("Client")));
}
