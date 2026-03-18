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
    #[serde(default = "default_bg_color")]
    pub bg_color: String,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_corner_radius")]
    pub corner_radius: u32,
    #[serde(default = "default_padding")]
    pub padding: u32,
    #[serde(default = "default_accept_key")]
    pub accept_key: String,
    #[serde(default)]
    pub ignored_apps: Vec<String>,
    #[serde(default = "default_true")]
    pub overlay_enabled: bool,
    #[serde(default = "default_true")]
    pub engine_enabled: bool,
    #[serde(default = "default_position_mode")]
    pub position_mode: String,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default = "default_max_suggestions")]
    pub max_suggestions: usize,
}

fn default_mode() -> String { "hybrid".into() }
fn default_prefix_length() -> usize { 2 }
fn default_color() -> String { "#C8C8E0".into() }
fn default_bg_color() -> String { "#1C1C2E".into() }
fn default_opacity() -> f32 { 0.92 }
fn default_font() -> String { "Segoe UI".into() }
fn default_font_size() -> u32 { 14 }
fn default_corner_radius() -> u32 { 6 }
fn default_padding() -> u32 { 7 }
fn default_accept_key() -> String { "Tab".into() }
fn default_true() -> bool { true }
fn default_position_mode() -> String { "above_caret".into() }
fn default_debounce_ms() -> u64 { 30 }
fn default_max_suggestions() -> usize { 1 }

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            prefix_length: default_prefix_length(),
            color: default_color(),
            bg_color: default_bg_color(),
            opacity: default_opacity(),
            font: default_font(),
            font_size: default_font_size(),
            corner_radius: default_corner_radius(),
            padding: default_padding(),
            accept_key: default_accept_key(),
            ignored_apps: Vec::new(),
            overlay_enabled: default_true(),
            engine_enabled: default_true(),
            position_mode: default_position_mode(),
            debounce_ms: default_debounce_ms(),
            max_suggestions: default_max_suggestions(),
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

    /// Returns (vk_code, need_ctrl, need_shift, need_alt)
    pub fn accept_key_parsed(&self) -> (u32, bool, bool, bool) {
        parse_accept_key(&self.accept_key)
    }

    pub fn parse_color(&self) -> (u8, u8, u8) {
        parse_hex_color(&self.color)
    }

    pub fn parse_bg_color(&self) -> (u8, u8, u8) {
        parse_hex_color(&self.bg_color)
    }
}

pub fn parse_hex_color(s: &str) -> (u8, u8, u8) {
    let hex = s.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(160);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(160);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(160);
        (r, g, b)
    } else {
        (160, 160, 160)
    }
}

/// Parse "Ctrl+Space", "Tab", "Alt+X", "RCtrl" etc.
/// Returns (vk_code, ctrl, shift, alt)
pub fn parse_accept_key(s: &str) -> (u32, bool, bool, bool) {
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    if parts.is_empty() {
        return (VK_TAB.0 as u32, false, false, false);
    }
    let (mut ctrl, mut shift, mut alt) = (false, false, false);
    let key_part = if parts.len() > 1 {
        for &p in &parts[..parts.len() - 1] {
            match p {
                "Ctrl" => ctrl = true,
                "Shift" => shift = true,
                "Alt" => alt = true,
                _ => {}
            }
        }
        parts[parts.len() - 1]
    } else {
        parts[0]
    };
    let vk = name_to_vk(key_part);
    (vk, ctrl, shift, alt)
}

pub fn name_to_vk(name: &str) -> u32 {
    match name {
        "Tab" => VK_TAB.0 as u32,
        "Enter" => VK_RETURN.0 as u32,
        "Space" => VK_SPACE.0 as u32,
        "Backspace" => VK_BACK.0 as u32,
        "Escape" | "Esc" => VK_ESCAPE.0 as u32,
        "CapsLock" => VK_CAPITAL.0 as u32,
        "Insert" => VK_INSERT.0 as u32,
        "Delete" | "Del" => VK_DELETE.0 as u32,
        "Home" => VK_HOME.0 as u32,
        "End" => VK_END.0 as u32,
        "PageUp" => VK_PRIOR.0 as u32,
        "PageDown" => VK_NEXT.0 as u32,
        "Left" | "Right Arrow" => VK_LEFT.0 as u32,
        "Right" => VK_RIGHT.0 as u32,
        "Up" => VK_UP.0 as u32,
        "Down" => VK_DOWN.0 as u32,
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
        "RCtrl" => VK_RCONTROL.0 as u32,
        "LCtrl" => VK_LCONTROL.0 as u32,
        "RAlt" => VK_RMENU.0 as u32,
        "LAlt" => VK_LMENU.0 as u32,
        "RShift" => VK_RSHIFT.0 as u32,
        "LShift" => VK_LSHIFT.0 as u32,
        ";" => VK_OEM_1.0 as u32,
        "=" => VK_OEM_PLUS.0 as u32,
        "," => VK_OEM_COMMA.0 as u32,
        "-" => VK_OEM_MINUS.0 as u32,
        "." => VK_OEM_PERIOD.0 as u32,
        "/" => VK_OEM_2.0 as u32,
        "`" => VK_OEM_3.0 as u32,
        "[" => VK_OEM_4.0 as u32,
        "\\" => VK_OEM_5.0 as u32,
        "]" => VK_OEM_6.0 as u32,
        "'" => VK_OEM_7.0 as u32,
        // Single char: letter or digit
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap().to_ascii_uppercase();
            c as u32
        }
        _ => VK_TAB.0 as u32,
    }
}

pub fn vk_to_name(vk: u32) -> String {
    match vk {
        v if v == VK_TAB.0 as u32 => "Tab".into(),
        v if v == VK_RETURN.0 as u32 => "Enter".into(),
        v if v == VK_SPACE.0 as u32 => "Space".into(),
        v if v == VK_BACK.0 as u32 => "Backspace".into(),
        v if v == VK_ESCAPE.0 as u32 => "Escape".into(),
        v if v == VK_CAPITAL.0 as u32 => "CapsLock".into(),
        v if v == VK_INSERT.0 as u32 => "Insert".into(),
        v if v == VK_DELETE.0 as u32 => "Delete".into(),
        v if v == VK_HOME.0 as u32 => "Home".into(),
        v if v == VK_END.0 as u32 => "End".into(),
        v if v == VK_PRIOR.0 as u32 => "PageUp".into(),
        v if v == VK_NEXT.0 as u32 => "PageDown".into(),
        v if v == VK_LEFT.0 as u32 => "Left".into(),
        v if v == VK_RIGHT.0 as u32 => "Right".into(),
        v if v == VK_UP.0 as u32 => "Up".into(),
        v if v == VK_DOWN.0 as u32 => "Down".into(),
        v if v == VK_F1.0 as u32 => "F1".into(),
        v if v == VK_F2.0 as u32 => "F2".into(),
        v if v == VK_F3.0 as u32 => "F3".into(),
        v if v == VK_F4.0 as u32 => "F4".into(),
        v if v == VK_F5.0 as u32 => "F5".into(),
        v if v == VK_F6.0 as u32 => "F6".into(),
        v if v == VK_F7.0 as u32 => "F7".into(),
        v if v == VK_F8.0 as u32 => "F8".into(),
        v if v == VK_F9.0 as u32 => "F9".into(),
        v if v == VK_F10.0 as u32 => "F10".into(),
        v if v == VK_F11.0 as u32 => "F11".into(),
        v if v == VK_F12.0 as u32 => "F12".into(),
        v if v == VK_RCONTROL.0 as u32 => "RCtrl".into(),
        v if v == VK_LCONTROL.0 as u32 => "LCtrl".into(),
        v if v == VK_RMENU.0 as u32 => "RAlt".into(),
        v if v == VK_LMENU.0 as u32 => "LAlt".into(),
        v if v == VK_RSHIFT.0 as u32 => "RShift".into(),
        v if v == VK_LSHIFT.0 as u32 => "LShift".into(),
        v if v == VK_OEM_1.0 as u32 => ";".into(),
        v if v == VK_OEM_PLUS.0 as u32 => "=".into(),
        v if v == VK_OEM_COMMA.0 as u32 => ",".into(),
        v if v == VK_OEM_MINUS.0 as u32 => "-".into(),
        v if v == VK_OEM_PERIOD.0 as u32 => ".".into(),
        v if v == VK_OEM_2.0 as u32 => "/".into(),
        v if v == VK_OEM_3.0 as u32 => "`".into(),
        v if v == VK_OEM_4.0 as u32 => "[".into(),
        v if v == VK_OEM_5.0 as u32 => "\\".into(),
        v if v == VK_OEM_6.0 as u32 => "]".into(),
        v if v == VK_OEM_7.0 as u32 => "'".into(),
        // A-Z
        v if v >= 0x41 && v <= 0x5A => {
            char::from_u32(v).map(|c| c.to_string()).unwrap_or_else(|| format!("VK{v:#04X}"))
        }
        // 0-9
        v if v >= 0x30 && v <= 0x39 => {
            char::from_u32(v).map(|c| c.to_string()).unwrap_or_else(|| format!("VK{v:#04X}"))
        }
        v => format!("VK{v:#04X}"),
    }
}

pub fn is_modifier_key(vk: u32) -> bool {
    let modifiers: &[u32] = &[
        VK_CONTROL.0 as u32,
        VK_LCONTROL.0 as u32,
        VK_RCONTROL.0 as u32,
        VK_SHIFT.0 as u32,
        VK_LSHIFT.0 as u32,
        VK_RSHIFT.0 as u32,
        VK_MENU.0 as u32,
        VK_LMENU.0 as u32,
        VK_RMENU.0 as u32,
        VK_LWIN.0 as u32,
        VK_RWIN.0 as u32,
    ];
    modifiers.contains(&vk)
}
