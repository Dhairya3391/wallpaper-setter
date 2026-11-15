//! Configuration management — TOML config with automatic creation on first launch,
//! screen resolution detection, crop algorithms, priority lists, and blacklist filtering.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

const CONFIG_DIR_NAME: &str = "wall";
const CONFIG_FILE_NAME: &str = "config.toml";
const LEGACY_CONFIG_FILE: &str = "wall_cfg";

/// Top-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub screen: Screen,
    #[serde(default)]
    pub fit: FitConfig,
    #[serde(default)]
    pub filter: Filter,
    #[serde(default)]
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitConfig {
    /// Cropping/fitting mode:
    /// - "smart": Centers horizontally, protects top 35% vertically (preserves subjects/heads).
    /// - "cover": Standard exact center-crop.
    /// - "contain_blur": Scales image to fit completely, fills edges with ambient blurred backdrop.
    #[serde(default = "default_fit_mode")]
    pub mode: String,
}

impl Default for FitConfig {
    fn default() -> Self {
        Self {
            mode: default_fit_mode(),
        }
    }
}

fn default_fit_mode() -> String {
    "smart".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Filter {
    /// Categories to never pick from (case-insensitive).
    #[serde(default)]
    pub blacklist: Vec<String>,
    /// Preferred categories — these get 60% bias in random mode.
    #[serde(default)]
    pub priority: Vec<String>,
    /// Preferred colors — random mode will bias toward these.
    #[serde(default)]
    pub colors: Vec<String>,
    /// Colors to never pick from.
    #[serde(default)]
    pub blacklist_colors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Source {
    /// Optional local wallpaper directory (fallback for `--local` mode).
    #[serde(default)]
    pub walls_dir: Option<String>,
}

fn config_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        let dot_config = home.join(".config").join(CONFIG_DIR_NAME);
        if dot_config.exists() || cfg!(target_os = "android") || cfg!(target_os = "linux") {
            return dot_config;
        }
    }
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".config"))
        .join(CONFIG_DIR_NAME)
}

pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILE_NAME)
}

pub fn cache_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        let dot_cache = home.join(".cache").join(CONFIG_DIR_NAME);
        if dot_cache.exists() || cfg!(target_os = "android") || cfg!(target_os = "linux") {
            return dot_cache;
        }
    }
    dirs::cache_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".cache"))
        .join(CONFIG_DIR_NAME)
}

/// Detect screen resolution dynamically (e.g. from Android `wm size`), or fallback to 1080x2400.
pub fn detect_screen_resolution() -> (u32, u32) {
    if let Ok(output) = std::process::Command::new("wm").arg("size").output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            for line in s.lines() {
                if let Some(pos) = line.find(':') {
                    let parts: Vec<&str> = line[pos + 1..].trim().split('x').collect();
                    if parts.len() == 2 {
                        if let (Ok(w), Ok(h)) = (parts[0].trim().parse::<u32>(), parts[1].trim().parse::<u32>()) {
                            if w > 0 && h > 0 {
                                return (w, h);
                            }
                        }
                    }
                }
            }
        }
    }
    (1080, 2400)
}

/// Generates a rich, commented TOML configuration string with helpful defaults.
pub fn default_config_toml(width: u32, height: u32) -> String {
    format!(
r#"# ==============================================================================
# wall - Android Wallpaper Setter Configuration
# Location: ~/.config/wall/config.toml
# ==============================================================================

[screen]
# Screen dimensions for your Android device in pixels.
# Images are scaled and cropped to match this aspect ratio.
width = {width}
height = {height}

[fit]
# Wallpaper fitting & cropping algorithm:
# - "smart"       : Intelligent cover-fit. Centers horizontally, protects the top
#                   35% vertically (avoids cutting off characters, faces, horizons).
# - "cover"       : Standard symmetrical center-crop.
# - "contain_blur": Scales image to fit completely without any cropping;
#                   fills letterbox bars with a smooth, dimmed blurred ambient backdrop.
mode = "smart"

[filter]
# Categories to exclude (case-insensitive).
# Official categories available on WallWidgy:
#   abstract, amoled, anime, architecture, art, cars, minimal, nature, tech
blacklist = ["abstract"]

# Preferred categories for random mode (60% chance to pick from these).
priority = ["anime", "nature"]

# Preferred colors for random mode.
# Run `wall --colors` to see all 39 supported colors.
colors = ["blue", "red", "white", "black", "purple", "green", "gray"]

# Colors to exclude completely (case-insensitive).
blacklist_colors = []

[source]
# Optional fallback folder when using `wall --local`.
# walls_dir = "/sdcard/Pictures/Wallpapers"
"#
    )
}

/// Try to migrate the old `wall_cfg` format into the new TOML config.
fn migrate_legacy() -> Option<Config> {
    let legacy = cache_dir()
        .parent()
        .unwrap_or(&cache_dir())
        .join(LEGACY_CONFIG_FILE);

    let contents = fs::read_to_string(&legacy).ok()?;
    let mut lines = contents.lines();

    let mut dims = lines.next()?.split_whitespace();
    let width: u32 = dims.next()?.parse().ok()?;
    let height: u32 = dims.next()?.parse().ok()?;
    let walls_dir = lines.next().map(|s| s.to_owned());

    let config = Config {
        screen: Screen { width, height },
        fit: FitConfig::default(),
        filter: Filter::default(),
        source: Source { walls_dir },
    };

    if config.save().is_ok() {
        let _ = fs::remove_file(&legacy);
        eprintln!("Migrated legacy config to {}", config_path().display());
    }

    Some(config)
}

impl Config {
    /// Loads existing config, migrates legacy config, or creates the default config on first launch.
    pub fn load_or_create() -> io::Result<(Self, bool)> {
        // 1. Try reading existing config.toml
        if let Some(cfg) = Self::read() {
            return Ok((cfg, false));
        }

        // 2. Try migrating legacy config
        if let Some(cfg) = migrate_legacy() {
            return Ok((cfg, false));
        }

        // 3. First launch: auto-create the best default config!
        let (width, height) = detect_screen_resolution();
        let cfg = Self::create_default(width, height)?;
        Ok((cfg, true))
    }

    fn read() -> Option<Self> {
        let contents = fs::read_to_string(config_path()).ok()?;
        toml::from_str(&contents).ok()
    }

    /// Creates and writes the default configuration file.
    pub fn create_default(width: u32, height: u32) -> io::Result<Self> {
        let dir = config_dir();
        fs::create_dir_all(&dir)?;

        let toml_str = default_config_toml(width, height);
        let path = config_path();
        let temp = path.with_extension("tmp");
        fs::write(&temp, toml_str.as_bytes())?;
        fs::rename(temp, &path)?;

        let cfg: Config = toml::from_str(&toml_str)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(cfg)
    }

    /// Reset configuration file back to default.
    pub fn reset_to_default() -> io::Result<Self> {
        let (width, height) = detect_screen_resolution();
        Self::create_default(width, height)
    }

    pub fn save(&self) -> io::Result<()> {
        let dir = config_dir();
        fs::create_dir_all(&dir)?;

        let contents = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let path = config_path();
        let temp = path.with_extension("tmp");
        fs::write(&temp, contents.as_bytes())?;
        fs::rename(temp, path)
    }

    pub fn is_blacklisted(&self, category: &str) -> bool {
        let cat = category.trim_start_matches('#').to_lowercase();
        self.filter.blacklist.iter().any(|b| b.trim_start_matches('#').to_lowercase() == cat)
    }

    pub fn is_color_blacklisted(&self, color: &str) -> bool {
        let c = color.to_lowercase();
        self.filter.blacklist_colors.iter().any(|b| b.to_lowercase() == c)
    }
}
