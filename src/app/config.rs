// Eden DAW — User configuration
// Persists settings between sessions (theme, favorites, toggles, etc.)

use serde::{Deserialize, Serialize};

/// User configuration saved/loaded from disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    /// Name of the selected theme
    pub theme_name: String,
    /// Favorite folder paths for the sample browser
    pub favorite_folders: Vec<String>,
    /// Auto-return to start on stop
    pub auto_return: bool,
    /// Last used UI scale
    pub ui_scale: f32,
    /// Snap enabled
    pub snap_enabled: bool,
    /// Snap resolution index
    pub snap_resolution_idx: usize,
    /// Sample browser open
    pub sample_browser_open: bool,
    /// Sample browser width
    pub sample_browser_width: i32,
    /// Bottom panel open
    pub bottom_panel_open: bool,
    /// Bottom panel height
    pub bottom_panel_height: i32,
    /// Velocity editor visible
    pub velocity_editor_visible: bool,
    /// Last window width
    pub window_width: u32,
    /// Last window height
    pub window_height: u32,
    /// Left panel active tab (0=Files, 1=Clips, 2=Instruments, 3=Themes)
    pub left_panel_tab: u8,
    /// Sample auto-play
    pub sample_auto_play: bool,
    /// Audio device index
    pub audio_device_idx: usize,
    /// Recently opened project file paths (newest first, max 10)
    pub recent_projects: Vec<String>,
    /// Whether the arranger follows the playhead during playback
    pub follow_playhead: bool,
    /// Autosave enabled
    #[serde(default)]
    pub autosave_enabled: bool,
    /// Autosave interval index into AUTOSAVE_INTERVALS
    #[serde(default = "default_autosave_idx")]
    pub autosave_interval_idx: usize,
}

/// Available autosave intervals: (display label, seconds)
pub const AUTOSAVE_INTERVALS: &[(&str, u64)] = &[("5 min", 300), ("15 min", 900), ("30 min", 1800)];

fn default_autosave_idx() -> usize {
    1 // default: 15 minutes
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            theme_name: "Dark".into(),
            favorite_folders: Vec::new(),
            auto_return: true,
            ui_scale: 1.0,
            snap_enabled: true,
            snap_resolution_idx: 2,
            sample_browser_open: true,
            sample_browser_width: 220,
            bottom_panel_open: false,
            bottom_panel_height: 24,
            velocity_editor_visible: true,
            window_width: 1280,
            window_height: 800,
            left_panel_tab: 0,
            sample_auto_play: true,
            audio_device_idx: 0,
            recent_projects: Vec::new(),
            follow_playhead: false,
            autosave_enabled: false,
            autosave_interval_idx: 1,
        }
    }
}

impl UserConfig {
    /// Get the config file path (~/.config/eden/config.json)
    pub fn config_path() -> std::path::PathBuf {
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        home.join(".config").join("eden").join("config.json")
    }

    /// Load config from disk, or return default if not found.
    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str(&data) {
                return config;
            }
        }
        Self::default()
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }
        let json =
            serde_json::to_string_pretty(self).map_err(|e| format!("Serialize error: {}", e))?;
        std::fs::write(&path, json).map_err(|e| format!("Write error: {}", e))?;
        Ok(())
    }
}
