use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    #[default]
    Dark,
    Light,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    #[default]
    Rich,
    Minimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusConfig {
    #[serde(default)]
    pub color_mode: ColorMode,
    #[serde(default)]
    pub display_mode: DisplayMode,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
}

impl Default for NexusConfig {
    fn default() -> Self {
        Self {
            color_mode: ColorMode::Dark,
            display_mode: DisplayMode::Rich,
            api_key: String::new(),
            base_url: None,
            model: "claude-3-7-sonnet-20250219".to_string(),
        }
    }
}

impl NexusConfig {
    pub fn config_path() -> Option<PathBuf> {
        let base = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)?;
        Some(base.join(".nexus").join("config.json"))
    }

    pub fn load() -> Option<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let data = fs::read_to_string(&path).ok()?;
            serde_json::from_str(&data).ok()
        } else {
            None
        }
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let json = serde_json::to_string_pretty(self)?;
            fs::write(path, json)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization_roundtrip() {
        let cfg = NexusConfig {
            color_mode: ColorMode::Dark,
            display_mode: DisplayMode::Rich,
            api_key: "sk-ant-test-key".to_string(),
            base_url: Some("https://api.aicredits.in".to_string()),
            model: "z-ai/glm-5.3-flash".to_string(),
        };

        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let loaded: NexusConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.color_mode, ColorMode::Dark);
        assert_eq!(loaded.display_mode, DisplayMode::Rich);
        assert_eq!(loaded.api_key, "sk-ant-test-key");
        assert_eq!(loaded.base_url.as_deref(), Some("https://api.aicredits.in"));
        assert_eq!(loaded.model, "z-ai/glm-5.3-flash");
    }
}
