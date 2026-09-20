use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub dot: DotConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

fn config_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return std::path::PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").expect("crosshair: HOME is not set");
    std::path::PathBuf::from(home).join(".config")
}

fn config_path() -> std::path::PathBuf {
    config_dir().join("crosshair").join("config.toml")
}

pub fn load() -> Config {
    let path = config_path();
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

/// Write the config to disk, creating the directory on first save. The file
/// is replaced atomically (temp file + rename), so a reader sees either the
/// old or the new contents, never a torn write. No fsync: the panel saves
/// on every nudge, and the rename already carries the atomicity guarantee;
/// losing the last save to a crash is acceptable for a calibration offset.
/// Saving rewrites the file from the parsed config, so comments and unknown
/// keys are not preserved.
pub fn save(cfg: &Config) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(cfg).map_err(std::io::Error::other)?;
    let tmp = path.with_file_name("config.toml.tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut file, text.as_bytes())?;
    }
    std::fs::rename(&tmp, &path)
}

/// Parse "#rrggbb" or "#rgb" into 0..1 channel floats. Returns None on bad input.
pub fn parse_hex_color(s: &str) -> Option<(f64, f64, f64)> {
    let s = s.trim().trim_start_matches('#');
    // The arms below slice at byte offsets, only sound for ASCII.
    if !s.is_ascii() {
        return None;
    }
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
