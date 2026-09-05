use pumpkin_config::{AdvancedConfiguration, BasicConfiguration};
use std::num::NonZero;

pub fn configuration() -> (BasicConfiguration, AdvancedConfiguration) {
    let mut basic = BasicConfiguration {
        default_level_name: "world".to_string(),
        ..Default::default()
    };
    // Workers exposes configured string variables through the Emscripten
    // environment. Existing worlds retain the seed stored in level.dat.
    if let Ok(seed) = std::env::var("WORLD_SEED") {
        basic.seed = seed.as_str().into();
    }

    let mut adv = AdvancedConfiguration::default();
    // Offline Java server with a bounded view distance.
    adv.networking.java.enabled = false; // Connections are supplied by the Durable Object.
    adv.networking.java.online_mode = false;
    adv.networking.java.encryption = false;
    adv.networking.java.view_distance = NonZero::new(3).unwrap();
    adv.networking.java.simulation_distance = NonZero::new(3).unwrap();
    adv.networking.java.motd = "Minecraft on Cloudflare Workers".to_string();
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
    // Packet compression is disabled for this configuration.
    adv.networking.java.compression.enabled = false;

    adv.logging.enabled = true;
    adv.logging.file.clear();
    adv.logging.level = "info".to_string();
    adv.logging.timestamp = false;
    adv.logging.color = false;

    (basic, adv)
}
