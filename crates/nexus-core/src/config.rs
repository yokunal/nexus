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
    Claude,
    Dark,
    Cyberpunk,
    TokyoNight,
    Amber,
    Nordic,
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
    #[serde(default)]
    pub max_tool_turns: Option<usize>,
}

impl Default for NexusConfig {
    fn default() -> Self {
        Self {
            color_mode: ColorMode::Claude,
            display_mode: DisplayMode::Rich,
            api_key: String::new(),
            base_url: None,
            model: "claude-3-7-sonnet-20250219".to_string(),
            max_tool_turns: Some(50),
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

    /// Flexible parser that supports both native Nexus config and Claude Code `settings.json` format
    pub fn parse_flexible(content: &str) -> Option<Self> {
        let val: serde_json::Value = serde_json::from_str(content).ok()?;

        // If it's a Claude Code style settings.json: { "env": { "ANTHROPIC_AUTH_TOKEN": "...", ... } }
        if let Some(env_obj) = val.get("env") {
            let token = env_obj
                .get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| env_obj.get("ANTHROPIC_API_KEY"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let base_url = env_obj
                .get("ANTHROPIC_BASE_URL")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string());
            let model = env_obj
                .get("ANTHROPIC_MODEL")
                .and_then(|v| v.as_str())
                .unwrap_or("claude-3-7-sonnet-20250219")
                .trim()
                .to_string();

            if !token.is_empty() {
                return Some(Self {
                    color_mode: ColorMode::Claude,
                    display_mode: DisplayMode::Rich,
                    api_key: token,
                    base_url,
                    model,
                    max_tool_turns: Some(50),
                });
            }
        }

        // Otherwise try standard NexusConfig
        serde_json::from_value(val).ok()
    }

    pub fn load_from_workspace_or_global(workspace: &std::path::Path) -> Option<Self> {
        // 1. Workspace .nexus/config.json
        let ws_cfg = workspace.join(".nexus").join("config.json");
        if ws_cfg.exists() {
            if let Ok(data) = fs::read_to_string(&ws_cfg) {
                if let Some(cfg) = Self::parse_flexible(&data) {
                    return Some(cfg);
                }
            }
        }

        // 2. Workspace .nexus/settings.json
        let ws_settings = workspace.join(".nexus").join("settings.json");
        if ws_settings.exists() {
            if let Ok(data) = fs::read_to_string(&ws_settings) {
                if let Some(cfg) = Self::parse_flexible(&data) {
                    return Some(cfg);
                }
            }
        }

        // 3. User global ~/.nexus/config.json
        if let Some(user_cfg) = Self::config_path() {
            if user_cfg.exists() {
                if let Ok(data) = fs::read_to_string(&user_cfg) {
                    if let Some(cfg) = Self::parse_flexible(&data) {
                        return Some(cfg);
                    }
                }
            }
        }

        // 4. User global ~/.nexus/settings.json
        if let Some(user_base) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
        {
            let user_settings = user_base.join(".nexus").join("settings.json");
            if user_settings.exists() {
                if let Ok(data) = fs::read_to_string(&user_settings) {
                    if let Some(cfg) = Self::parse_flexible(&data) {
                        return Some(cfg);
                    }
                }
            }

            // 5. User global ~/.claude/settings.json
            let claude_settings = user_base.join(".claude").join("settings.json");
            if claude_settings.exists() {
                if let Ok(data) = fs::read_to_string(&claude_settings) {
                    if let Some(cfg) = Self::parse_flexible(&data) {
                        return Some(cfg);
                    }
                }
            }
        }

        None
    }

    pub fn load() -> Option<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let data = fs::read_to_string(&path).ok()?;
            Self::parse_flexible(&data)
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
            max_tool_turns: Some(50),
        };

        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let loaded: NexusConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.color_mode, ColorMode::Dark);
        assert_eq!(loaded.display_mode, DisplayMode::Rich);
        assert_eq!(loaded.api_key, "sk-ant-test-key");
        assert_eq!(loaded.base_url.as_deref(), Some("https://api.aicredits.in"));
        assert_eq!(loaded.model, "z-ai/glm-5.3-flash");
    }

    #[test]
    fn test_parse_claude_settings_format() {
        let claude_json = r#"{
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.aicredits.in",
                "ANTHROPIC_AUTH_TOKEN": "sk-live-123456789",
                "ANTHROPIC_MODEL": "deepseek/deepseek-v4-flash"
            }
        }"#;

        let parsed = NexusConfig::parse_flexible(claude_json).expect("Must parse Claude format");
        assert_eq!(parsed.api_key, "sk-live-123456789");
        assert_eq!(parsed.base_url.as_deref(), Some("https://api.aicredits.in"));
        assert_eq!(parsed.model, "deepseek/deepseek-v4-flash");
    }
}
