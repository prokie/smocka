use bevy::prelude::*;
use lightyear::prelude::{client::InputDelayConfig, *};

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, configure_input_delay);
    }
}
/// Configure the input delay for the network timeline.
fn configure_input_delay(mut commands: Commands) {
    commands.insert_resource(
        InputTimelineConfig::default().with_input_delay(InputDelayConfig::no_input_delay()),
    );
}
