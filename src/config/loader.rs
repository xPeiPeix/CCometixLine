use super::types::Config;
use std::fs;
use std::path::{Path, PathBuf};

/// Result of config initialization
#[derive(Debug)]
pub enum InitResult {
    /// Config was created at the given path
    Created(PathBuf),
    /// Config already existed at the given path
    AlreadyExists(PathBuf),
}

pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load() -> Config {
        Config::load().unwrap_or_else(|_| Config::default())
    }

    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Config, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Initialize themes directory and create built-in theme files
    pub fn init_themes() -> Result<(), Box<dyn std::error::Error>> {
        let themes_dir = Self::get_themes_path();

        // Create themes directory
        fs::create_dir_all(&themes_dir)?;

        let mut created_any = false;

        for theme_name in Self::builtin_theme_names() {
            let theme_path = themes_dir.join(format!("{}.toml", theme_name));

            if !theme_path.exists() {
                let theme_config = crate::ui::themes::ThemePresets::get_theme(theme_name);
                let content = toml::to_string_pretty(&theme_config)?;
                fs::write(&theme_path, content)?;
                println!("Created theme file: {}", theme_path.display());
                created_any = true;
            }
        }

        if !created_any {
            // all built-in theme files already exist
        }

        Ok(())
    }

    /// Get the themes directory path (~/.claude/ccline/themes/)
    pub fn get_themes_path() -> PathBuf {
        if let Some(home) = dirs::home_dir() {
            home.join(".claude").join("ccline").join("themes")
        } else {
            PathBuf::from(".claude/ccline/themes")
        }
    }

    /// Ensure themes directory exists and has built-in themes (silent mode).
    /// Runs filesystem checks only once per process — subsequent calls are no-ops
    /// so hot paths (e.g. UsageSegment re-loading config for API fallback) don't
    /// re-stat 9 theme files on every statusline tick.
    pub fn ensure_themes_exist() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let _ = Self::init_themes_silent();
        });
    }

    /// Overwrite every built-in theme file with the current in-binary defaults.
    /// Called by `ccline --reset-themes` — useful after upgrade when palette
    /// changes ship but existing toml files on disk block them.
    pub fn reset_builtin_themes() -> Result<Vec<std::path::PathBuf>, Box<dyn std::error::Error>> {
        let themes_dir = Self::get_themes_path();
        fs::create_dir_all(&themes_dir)?;
        let mut written = Vec::new();
        for theme_name in Self::builtin_theme_names() {
            let theme_path = themes_dir.join(format!("{}.toml", theme_name));
            let theme_config = crate::ui::themes::ThemePresets::get_theme(theme_name);
            let content = toml::to_string_pretty(&theme_config)?;
            fs::write(&theme_path, content)?;
            written.push(theme_path);
        }
        Ok(written)
    }

    fn builtin_theme_names() -> &'static [&'static str] {
        &[
            "default",
            "minimal",
            "gruvbox",
            "nord",
            "cometix",
            "powerline-dark",
            "powerline-light",
            "powerline-rose-pine",
            "powerline-tokyo-night",
        ]
    }

    /// Initialize themes directory and create built-in theme files (silent mode)
    fn init_themes_silent() -> Result<(), Box<dyn std::error::Error>> {
        let themes_dir = Self::get_themes_path();

        // Create themes directory
        fs::create_dir_all(&themes_dir)?;

        for theme_name in Self::builtin_theme_names() {
            let theme_path = themes_dir.join(format!("{}.toml", theme_name));

            if !theme_path.exists() {
                let theme_config = crate::ui::themes::ThemePresets::get_theme(theme_name);
                let content = toml::to_string_pretty(&theme_config)?;
                fs::write(&theme_path, content)?;
            }
        }

        Ok(())
    }
}

impl Config {
    /// Stable-sort segments so inter-group order is fixed (per GROUP_ORDER)
    /// while intra-group order is preserved from the current Vec.
    pub fn normalize_segment_order(&mut self) {
        self.segments
            .sort_by_key(|s| crate::core::SegmentGroup::of(&s.id).sort_key());
    }

    /// Load configuration from default location.
    ///
    /// Does NOT call `normalize_segment_order` — the user's on-disk segment
    /// order is authoritative; the TUI renders a group-aware view without
    /// mutating the underlying Vec, and runtime rendering doesn't care about
    /// group adjacency (separator logic uses `SegmentGroup::of(id)` directly).
    pub fn load() -> Result<Config, Box<dyn std::error::Error>> {
        ConfigLoader::ensure_themes_exist();

        let config_path = Self::get_config_path();

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to default location
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path();

        // Ensure config directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }

    /// Get the default config file path (~/.claude/ccline/config.toml)
    fn get_config_path() -> PathBuf {
        if let Some(home) = dirs::home_dir() {
            home.join(".claude").join("ccline").join("config.toml")
        } else {
            PathBuf::from(".claude/ccline/config.toml")
        }
    }

    /// Initialize config directory and create default config
    pub fn init() -> Result<InitResult, Box<dyn std::error::Error>> {
        let config_path = Self::get_config_path();

        // Create directory
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Initialize themes directory and built-in themes
        ConfigLoader::init_themes()?;

        // Create default config if it doesn't exist
        if !config_path.exists() {
            let default_config = Config::default();
            default_config.save()?;
            Ok(InitResult::Created(config_path))
        } else {
            Ok(InitResult::AlreadyExists(config_path))
        }
    }

    /// Validate configuration
    pub fn check(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Basic validation
        if self.segments.is_empty() {
            return Err("No segments configured".into());
        }

        // Validate segment IDs are unique
        let mut seen_ids = std::collections::HashSet::new();
        for segment in &self.segments {
            if !seen_ids.insert(segment.id) {
                return Err(format!("Duplicate segment ID: {:?}", segment.id).into());
            }
        }

        Ok(())
    }

    /// Print configuration as TOML
    pub fn print(&self) -> Result<(), Box<dyn std::error::Error>> {
        let content = toml::to_string_pretty(self)?;
        println!("{}", content);
        Ok(())
    }
}
