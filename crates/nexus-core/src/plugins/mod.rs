use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Plugin '{0}' not found")]
    NotFound(String),
    #[error("Failed to parse plugin manifest in '{0}': {1}")]
    ManifestError(String, String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCommand {
    pub name: String,
    pub description: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub path: PathBuf,
    pub commands: Vec<PluginCommand>,
    pub is_enabled: bool,
}

pub struct PluginManager;

impl PluginManager {
    /// Workspace plugin directory: `<workspace>/.nexus/plugins`
    pub fn workspace_plugin_dir(workspace: &Path) -> PathBuf {
        workspace.join(".nexus").join("plugins")
    }

    /// User global plugin directory: `~/.nexus/plugins`
    pub fn user_plugin_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".nexus").join("plugins"))
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|h| PathBuf::from(h).join(".nexus").join("plugins"))
            })
    }

    /// Scans workspace and user directories for plugins.
    /// Also checks `~/.claude/plugins/installed_plugins.json` for interoperability with Claude Code plugins.
    pub fn load_all(workspace: &Path) -> Result<Vec<Plugin>, PluginError> {
        let mut plugins = Vec::new();

        // 1. Workspace plugins: .nexus/plugins/<name>/plugin.json
        let ws_dir = Self::workspace_plugin_dir(workspace);
        if ws_dir.exists() {
            Self::scan_plugin_directory(&ws_dir, &mut plugins)?;
        }

        // 2. User plugins: ~/.nexus/plugins/<name>/plugin.json
        if let Some(user_dir) = Self::user_plugin_dir() {
            if user_dir.exists() && user_dir != ws_dir {
                Self::scan_plugin_directory(&user_dir, &mut plugins)?;
            }
        }

        // 3. Claude Code plugin compatibility scan: ~/.claude/plugins/installed_plugins.json
        if let Some(claude_dir) = Self::get_claude_dir() {
            let installed_json = claude_dir.join("plugins").join("installed_plugins.json");
            if installed_json.exists() {
                Self::load_claude_installed_plugins(&installed_json, &mut plugins);
            }
        }

        Ok(plugins)
    }

    fn scan_plugin_directory(dir: &Path, plugins: &mut Vec<Plugin>) -> Result<(), PluginError> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("plugin.json");
                if manifest_path.exists() {
                    if let Ok(content) = fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&content) {
                            plugins.push(Plugin {
                                name: manifest.name,
                                version: manifest.version.unwrap_or_else(|| "1.0.0".to_string()),
                                description: manifest
                                    .description
                                    .unwrap_or_else(|| "Nexus Plugin".to_string()),
                                path: path.clone(),
                                commands: manifest.commands,
                                is_enabled: true,
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn get_claude_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".claude"))
            .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".claude")))
    }

    /// Automatically discovers and adapts installed Claude Code plugins
    fn load_claude_installed_plugins(installed_json: &Path, plugins: &mut Vec<Plugin>) {
        if let Ok(content) = fs::read_to_string(installed_json) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(plugins_obj) = val.get("plugins").and_then(|p| p.as_object()) {
                    for (plugin_id, items) in plugins_obj {
                        if let Some(arr) = items.as_array() {
                            if let Some(first) = arr.first() {
                                let install_path = first
                                    .get("installPath")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let version = first
                                    .get("version")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("1.0.0");

                                let p_path = PathBuf::from(install_path);
                                if p_path.exists() {
                                    let clean_name = plugin_id
                                        .split('@')
                                        .next()
                                        .unwrap_or(plugin_id)
                                        .to_string();

                                    // Check if already loaded
                                    if !plugins.iter().any(|p| p.name == clean_name) {
                                        plugins.push(Plugin {
                                            name: clean_name.clone(),
                                            version: version.to_string(),
                                            description: format!(
                                                "Claude Code compatible plugin: {}",
                                                clean_name
                                            ),
                                            path: p_path,
                                            commands: vec![PluginCommand {
                                                name: clean_name.clone(),
                                                description: format!(
                                                    "Activate {} plugin guidelines",
                                                    clean_name
                                                ),
                                                prompt: format!(
                                                    "Activate guidelines and tools for plugin: {}",
                                                    clean_name
                                                ),
                                            }],
                                            is_enabled: true,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Formats plugin capabilities for the LLM system prompt
    pub fn format_plugins_prompt(plugins: &[Plugin]) -> String {
        let enabled: Vec<&Plugin> = plugins.iter().filter(|p| p.is_enabled).collect();
        if enabled.is_empty() {
            return String::new();
        }

        let mut out = String::from("## Active Plugins & Custom Slash Commands\n");
        for p in enabled {
            out.push_str(&format!(
                "- **{}** (v{}): {}\n",
                p.name, p.version, p.description
            ));
            for cmd in &p.commands {
                out.push_str(&format!("  - `/{}`: {}\n", cmd.name, cmd.description));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manifest_scan() {
        let temp_dir =
            std::env::temp_dir().join(format!("nexus_plugin_test_{}", uuid::Uuid::new_v4()));
        let plugin_folder = temp_dir.join("sample-linter");
        fs::create_dir_all(&plugin_folder).unwrap();

        let manifest = r#"{
            "name": "sample-linter",
            "version": "1.2.0",
            "description": "Custom code linter plugin",
            "commands": [
                {
                    "name": "lint",
                    "description": "Run custom linter",
                    "prompt": "Run deep static analysis on the workspace."
                }
            ]
        }"#;

        fs::write(plugin_folder.join("plugin.json"), manifest).unwrap();

        let mut plugins = Vec::new();
        PluginManager::scan_plugin_directory(&temp_dir, &mut plugins).unwrap();

        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "sample-linter");
        assert_eq!(plugins[0].commands.len(), 1);
        assert_eq!(plugins[0].commands[0].name, "lint");

        let prompt = PluginManager::format_plugins_prompt(&plugins);
        assert!(prompt.contains("sample-linter"));
        assert!(prompt.contains("/lint"));
    }
}
