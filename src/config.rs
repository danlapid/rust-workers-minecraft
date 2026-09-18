use pumpkin_config::{AdvancedConfiguration, BasicConfiguration};
use serde::Deserialize;
use std::num::NonZero;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    view_distance: u8,
    simulation_distance: u8,
    max_players: u32,
    compression_threshold: i32,
    compression_level: u32,
    motd: String,
    seed: Option<String>,
}

pub fn configuration(json: &str) -> Result<(BasicConfiguration, AdvancedConfiguration), String> {
    let settings: Settings =
        serde_json::from_str(json).map_err(|e| format!("Invalid server settings: {e}"))?;
    if !(2..=32).contains(&settings.view_distance)
        || !(2..=settings.view_distance).contains(&settings.simulation_distance)
        || !(1..=1000).contains(&settings.max_players)
        || !(-1..=2_097_152).contains(&settings.compression_threshold)
        || settings.compression_level > 9
    {
        return Err("Server settings are outside the supported ranges".to_owned());
    }
    let mut basic = BasicConfiguration {
        default_level_name: "world".to_string(),
        ..Default::default()
    };
    // Existing worlds retain the seed stored in level.dat.
    if let Some(seed) = settings.seed {
        basic.seed = seed.as_str().into();
    }

    let mut adv = AdvancedConfiguration::default();
    // Offline Java server with a bounded view distance.
    adv.networking.java.enabled = false; // Connections are supplied by the Durable Object.
    adv.networking.java.online_mode = false;
    adv.networking.java.encryption = false;
    adv.networking.java.view_distance = NonZero::new(settings.view_distance).unwrap();
    adv.networking.java.simulation_distance = NonZero::new(settings.simulation_distance).unwrap();
    adv.networking.java.max_players = settings.max_players;
    adv.networking.java.motd = settings.motd;
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
    adv.networking.java.compression.enabled = settings.compression_threshold >= 0;
    adv.networking.java.compression.info.threshold = settings.compression_threshold.max(0) as u32;
    adv.networking.java.compression.info.level = settings.compression_level;

    adv.logging.enabled = true;
    adv.logging.file.clear();
    adv.logging.level = "info".to_string();
    adv.logging.timestamp = false;
    adv.logging.color = false;

    Ok((basic, adv))
}
