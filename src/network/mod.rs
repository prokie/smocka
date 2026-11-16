mod protocol;

use crate::config::network::TICK_RATE;
use bevy::prelude::*;
use lightyear::prelude::{
    client::{ClientPlugins, InputDelayConfig},
    server::ServerPlugins,
    *,
};
use std::time::Duration;

/// The network plugin.
pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        let tick_duration = Duration::from_secs_f32(TICK_RATE);

        app.add_plugins((
            ClientPlugins { tick_duration },
            ServerPlugins { tick_duration },
        ));
        app.add_systems(Startup, configure_input_delay);
    }
}

/// Configure the input delay for the network timeline.
fn configure_input_delay(mut commands: Commands) {
    commands.insert_resource(
        InputTimelineConfig::default().with_input_delay(InputDelayConfig::no_input_delay()),
    );
}
