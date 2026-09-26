//! Operator settings, read from the Worker's environment variables per world
//! before its server starts.

use crate::host::property;
use wasm_bindgen::JsValue;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub view_distance: u8,
    pub simulation_distance: u8,
    pub max_players: u32,
    pub compression_threshold: i32,
    pub compression_level: u32,
    pub idle_timeout_seconds: u32,
    pub motd: String,
    pub seed: Option<String>,
}

impl Settings {
    pub fn from_env(env: &JsValue) -> Result<Settings, String> {
        let var = |name: &str| -> Result<Option<String>, String> {
            let value = property(env, name).map_err(|_| format!("{name} is unreadable"))?;
            if value.is_undefined() || value.is_null() {
                return Ok(None);
            }
            value
                .as_string()
                .map(Some)
                .ok_or_else(|| format!("{name} must be a string"))
        };
        let integer = |name: &str, fallback: i64, min: i64, max: i64| -> Result<i64, String> {
            let Some(raw) = var(name)? else {
                return Ok(fallback);
            };
            let value = raw
                .parse::<i64>()
                .ok()
                .filter(|v| (min..=max).contains(v))
                .ok_or_else(|| format!("{name} must be an integer between {min} and {max}"))?;
            Ok(value)
        };
        let view_distance = integer("VIEW_DISTANCE", 4, 2, 32)? as u8;
        let simulation_distance = integer("SIMULATION_DISTANCE", 3, 2, 32)? as u8;
        if simulation_distance > view_distance {
            return Err("SIMULATION_DISTANCE must not exceed VIEW_DISTANCE".into());
        }
        let motd = var("MOTD")?.unwrap_or_else(|| "Minecraft on Cloudflare Workers".into());
        if motd.trim().is_empty() || motd.len() > 512 {
            return Err("MOTD must contain between 1 and 512 characters".into());
        }
        let seed = var("WORLD_SEED")?;
        if let Some(seed) = &seed {
            if seed.trim().is_empty() || seed.len() > 256 {
                return Err(
                    "WORLD_SEED must be a nonempty string of at most 256 characters".into(),
                );
            }
        }
        Ok(Settings {
            view_distance,
            simulation_distance,
            max_players: integer("MAX_PLAYERS", 20, 1, 1000)? as u32,
            compression_threshold: integer("COMPRESSION_THRESHOLD", 512, -1, 2 * 1024 * 1024)?
                as i32,
            compression_level: integer("COMPRESSION_LEVEL", 1, 0, 9)? as u32,
            idle_timeout_seconds: integer("IDLE_TIMEOUT_SECONDS", 10, 0, 30)? as u32,
            motd,
            seed,
        })
    }

    pub fn to_js(&self) -> Result<JsValue, JsValue> {
        let object = js_sys::Object::new();
        let set = |key: &str, value: JsValue| js_sys::Reflect::set(&object, &key.into(), &value);
        set("view_distance", self.view_distance.into())?;
        set("simulation_distance", self.simulation_distance.into())?;
        set("max_players", self.max_players.into())?;
        set("compression_threshold", self.compression_threshold.into())?;
        set("compression_level", self.compression_level.into())?;
        set("idle_timeout_seconds", self.idle_timeout_seconds.into())?;
        set("motd", self.motd.as_str().into())?;
        set(
            "seed",
            self.seed.as_deref().map_or(JsValue::NULL, JsValue::from),
        )?;
        Ok(object.into())
    }
}
