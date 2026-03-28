// Eden DAW — User configuration
// Persists settings between sessions (theme, favorites, toggles, etc.)

use serde::{Deserialize, Serialize};

/// User configuration saved/loaded from disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Build a UserConfig snapshot from the current application state.
    pub fn from_state(state: &super::state::AppState) -> Self {
        Self {
            theme_name: state.theme.name.clone(),
            favorite_folders: state.favorite_folders.clone(),
            auto_return: state.auto_return,
            ui_scale: state.ui_scale,
            snap_enabled: state.snap.enabled,
            snap_resolution_idx: state.snap.resolution_idx,
            sample_browser_open: state.sample_browser_open,
            sample_browser_width: state.sample_browser_width,
            bottom_panel_open: state.bottom_panel_open,
            bottom_panel_height: state.bottom_panel_height,
            velocity_editor_visible: state.velocity_editor_visible,
            window_width: state.window_width,
            window_height: state.window_height,
            left_panel_tab: state.left_panel_tab.to_index(),
            sample_auto_play: state.sample_auto_play,
            audio_device_idx: state.audio_device_idx,
            recent_projects: state.recent_projects.clone(),
            follow_playhead: state.follow_playhead,
            autosave_enabled: state.autosave_enabled,
            autosave_interval_idx: state.autosave_interval_idx,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_config_default_values() {
        let cfg = UserConfig::default();
        assert_eq!(cfg.theme_name, "Dark");
        assert!(cfg.auto_return);
        assert_eq!(cfg.ui_scale, 1.0);
        assert!(cfg.snap_enabled);
        assert_eq!(cfg.snap_resolution_idx, 2);
        assert!(cfg.sample_browser_open);
        assert_eq!(cfg.left_panel_tab, 0);
        assert!(!cfg.autosave_enabled);
        assert_eq!(cfg.autosave_interval_idx, 1);
    }

    #[test]
    fn test_user_config_serialize_roundtrip() {
        let cfg = UserConfig {
            theme_name: "Neon".into(),
            ui_scale: 1.5,
            snap_enabled: false,
            follow_playhead: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let restored: UserConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, restored);
    }

    #[test]
    fn test_user_config_partial_eq() {
        let a = UserConfig::default();
        let b = UserConfig::default();
        assert_eq!(a, b);
        let c = UserConfig {
            theme_name: "Other".into(),
            ..Default::default()
        };
        assert_ne!(a, c);
    }

    #[test]
    fn test_autosave_intervals_valid() {
        assert!(AUTOSAVE_INTERVALS.len() >= 2);
        for &(label, secs) in AUTOSAVE_INTERVALS {
            assert!(!label.is_empty());
            assert!(secs > 0);
        }
    }
}
