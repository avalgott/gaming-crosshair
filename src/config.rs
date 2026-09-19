use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub dot: DotConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DotConfig {
    pub size: u32,
    pub color: String,
    /// Nudge the dot off the screen center: positive x = right, positive
    /// y = down, in logical pixels. Signed so the dot can be pushed any way.
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Default for DotConfig {
    fn default() -> Self {
        Self {
            size: 4,
            color: "#ffffff".into(),
            offset_x: 0,
            offset_y: 0,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dot: DotConfig::default(),
        }
    }
}

fn config_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return std::path::PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").expect("crosshair: HOME is not set");
    std::path::PathBuf::from(home).join(".config")
}

pub fn load() -> Config {
    let path = config_dir().join("crosshair").join("config.toml");
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
            eprintln!(
                "crosshair: bad config {}: {e}; using defaults",
                path.display()
            );
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

/// Parse "#rrggbb" or "#rgb" into 0..1 channel floats. Returns None on bad input.
pub fn parse_hex_color(s: &str) -> Option<(f64, f64, f64)> {
    let s = s.trim().trim_start_matches('#');
    let (r, g, b) = match s.len() {
        6 => (
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
        ),
        3 => (
            u8::from_str_radix(&s[0..1], 16).ok()? * 17,
            u8::from_str_radix(&s[1..2], 16).ok()? * 17,
            u8::from_str_radix(&s[2..3], 16).ok()? * 17,
        ),
        _ => return None,
    };
    Some((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0))
}
