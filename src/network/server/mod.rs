use bevy::prelude::*;
use lightyear::prelude::*;

pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(handle_new_client);
    }
}

/// Lets the server replicate to each remote client. The host's own client is skipped: Lightyear
/// replicates to it without a `ReplicationSender`.
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
