use bevy::prelude::*;
use lightyear::input::client::InputSystems;
use lightyear::prelude::{client::InputDelayConfig, *};

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, configure_input_delay);
        // Apply replicated despawns before receiving rebroadcast inputs: otherwise Lightyear can
        // queue a deferred `insert((InputBuffer, ActionState))` for a remote predicted entity that
        // `ReplicationSystems::Receive` despawns in the same `PreUpdate` before commands flush.
        app.configure_sets(
            PreUpdate,
            InputSystems::ReceiveInputMessages.after(ReplicationSystems::Receive),
        );
    }
}
fn configure_input_delay(mut commands: Commands) {
    commands.insert_resource(
        InputTimelineConfig::default().with_input_delay(InputDelayConfig::no_input_delay()),
    );
}
