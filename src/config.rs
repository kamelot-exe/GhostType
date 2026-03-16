use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const CONFIG_PATH: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_prefix_length")]
    pub prefix_length: usize,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_accept_key")]
    pub accept_key: String,
    #[serde(default)]
    pub ignored_apps: Vec<String>,
    #[serde(default = "default_overlay_enabled")]
    pub overlay_enabled: bool,
    #[serde(default = "default_engine_enabled")]
    pub engine_enabled: bool,
}

fn default_mode() -> String { "hybrid".into() }
fn default_prefix_length() -> usize { 2 }
fn default_color() -> String { "#A0A0A0".into() }
fn default_opacity() -> f32 { 0.7 }
fn default_font() -> String { "Segoe UI".into() }
fn default_font_size() -> u32 { 16 }
fn default_accept_key() -> String { "Tab".into() }
fn default_overlay_enabled() -> bool { true }
fn default_engine_enabled() -> bool { true }

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            prefix_length: default_prefix_length(),
            color: default_color(),
            opacity: default_opacity(),
            font: default_font(),
            font_size: default_font_size(),
            accept_key: default_accept_key(),
            ignored_apps: Vec::new(),
            overlay_enabled: default_overlay_enabled(),
            engine_enabled: default_engine_enabled(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        if Path::new(CONFIG_PATH).exists() {
            match fs::read_to_string(CONFIG_PATH) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(cfg) => return cfg,
                    Err(e) => eprintln!("config parse error: {e}, using defaults"),
                },
                Err(e) => eprintln!("config read error: {e}, using defaults"),
            }
        }
        let cfg = Config::default();
        cfg.save();
        cfg
    }

    pub fn save(&self) {
        match toml::to_string_pretty(self) {
            Ok(content) => {
                if let Err(e) = fs::write(CONFIG_PATH, content) {
                    eprintln!("config save error: {e}");
                }
            }
            Err(e) => eprintln!("config serialize error: {e}"),
        }
    }

    pub fn parse_color(&self) -> (u8, u8, u8) {
        let hex = self.color.trim_start_matches('#');
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(160);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(160);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(160);
            (r, g, b)
        } else {
            (160, 160, 160)
        }
    }
}
