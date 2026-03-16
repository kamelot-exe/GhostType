use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

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

    /// Convert accept_key string to Win32 virtual key code
    pub fn accept_vk(&self) -> u32 {
        key_name_to_vk(&self.accept_key)
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

pub const ACCEPT_KEY_OPTIONS: &[&str] = &[
    "Tab",
    "Right Arrow",
    "Insert",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "Ctrl+Space",
    "Ctrl+Enter",
];

pub fn key_name_to_vk(name: &str) -> u32 {
    match name.trim() {
        "Tab" => VK_TAB.0 as u32,
        "Right Arrow" => VK_RIGHT.0 as u32,
        "Insert" => VK_INSERT.0 as u32,
        "F1" => VK_F1.0 as u32,
        "F2" => VK_F2.0 as u32,
        "F3" => VK_F3.0 as u32,
        "F4" => VK_F4.0 as u32,
        "F5" => VK_F5.0 as u32,
        "F6" => VK_F6.0 as u32,
        "F7" => VK_F7.0 as u32,
        "F8" => VK_F8.0 as u32,
        "F9" => VK_F9.0 as u32,
        "F10" => VK_F10.0 as u32,
        "F11" => VK_F11.0 as u32,
        "F12" => VK_F12.0 as u32,
        "Ctrl+Space" => 0xFF01, // Custom sentinel, handled in engine
        "Ctrl+Enter" => 0xFF02, // Custom sentinel, handled in engine
        _ => VK_TAB.0 as u32,
    }
}
