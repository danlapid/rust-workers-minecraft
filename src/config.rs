use crate::settings::Settings;
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration};
use std::num::NonZero;

pub fn configuration(settings: &Settings) -> (BasicConfiguration, AdvancedConfiguration) {
    let mut basic = BasicConfiguration {
        default_level_name: "world".to_string(),
        ..Default::default()
    };
    // Existing worlds retain the seed stored in level.dat.
    if let Some(seed) = &settings.seed {
        basic.seed = seed.as_str().into();
    }

    let mut adv = AdvancedConfiguration::default();
    // Offline Java server with a bounded view distance. The listener binds the
    // default 0.0.0.0:25565 in the Durable Object's port table; the host routes
    // inbound sockets to it by port.
    adv.networking.java.online_mode = false;
    adv.networking.java.encryption = false;
    adv.networking.java.view_distance = NonZero::new(settings.view_distance).unwrap();
    adv.networking.java.simulation_distance = NonZero::new(settings.simulation_distance).unwrap();
    adv.networking.java.max_players = settings.max_players;
    adv.networking.java.motd = settings.motd.clone();
    // Disable native transports, plugins, and console input.
    adv.networking.bedrock.enabled = false;
    // The Bedrock OIDC key fetch spawns on online_mode && auth.enabled regardless of
    // bedrock.enabled; disable it so no reqwest/DNS (thread-spawning) runs at boot.
    adv.networking.bedrock.online_mode = false;
    adv.networking.bedrock.authentication.enabled = false;
    adv.networking.query.enabled = false;
    adv.networking.rcon.enabled = false;
    adv.networking.lan_broadcast.enabled = false;
    adv.plugins.enabled = false;
    adv.commands.use_console = false;
    // A negative threshold disables packet compression.
    adv.networking.java.compression.enabled = settings.compression_threshold >= 0;
    adv.networking.java.compression.info.threshold = settings.compression_threshold.max(0) as u32;
    adv.networking.java.compression.info.level = settings.compression_level;

    adv.logging.enabled = true;
    adv.logging.file.clear();
    adv.logging.level = "info".to_string();
    adv.logging.timestamp = false;
    adv.logging.color = false;

    (basic, adv)
}
