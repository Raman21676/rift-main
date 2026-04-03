//! Configuration file support
//!
//! Loads configuration from `~/.config/rift/config.toml`

use crate::capability::Capability;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration file format
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub capabilities: HashMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub key: Option<String>,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            key: None,
            base_url: default_base_url(),
            model: default_model(),
        }
    }
}

fn default_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            max_concurrent_tasks: default_max_concurrent_tasks(),
        }
    }
}

fn default_max_iterations() -> usize {
    10
}

fn default_max_concurrent_tasks() -> usize {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_auto_confirm")]
    pub auto_confirm: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            auto_confirm: default_auto_confirm(),
        }
    }
}

fn default_auto_confirm() -> bool {
    false
}

impl ConfigFile {
    /// Get the default config directory path
    pub fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("rift"))
    }

    /// Get the default config file path
    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|d| d.join("config.toml"))
    }

    /// Load config from file, or return defaults if file doesn't exist
    pub fn load() -> Self {
        match Self::config_path() {
            Some(path) if path.exists() => {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match toml::from_str(&content) {
                        Ok(config) => config,
                        Err(e) => {
                            eprintln!("Warning: Failed to parse config file {}: {}", path.display(), e);
                            Self::default()
                        }
                    },
                    Err(e) => {
                        eprintln!("Warning: Failed to read config file {}: {}", path.display(), e);
                        Self::default()
                    }
                }
            }
            _ => Self::default(),
        }
    }

    /// Parse capabilities from the config file
    pub fn parse_capabilities(&self) -> Vec<Capability> {
        let mut caps = Vec::new();

        for (key, value) in &self.capabilities {
            if !value.as_bool().unwrap_or(true) {
                continue;
            }

            match key.as_str() {
                "file_read" => caps.push(Capability::FileRead),
                "file_write" => caps.push(Capability::FileWrite),
                "shell_execute" => caps.push(Capability::ShellExecute),
                "network_access" => caps.push(Capability::NetworkAccess),
                _ => {
                    // Try to parse scoped capabilities
                    if key.starts_with("file_read_scoped") {
                        if let Some(patterns) = parse_string_array(value) {
                            caps.push(Capability::FileReadScoped(patterns));
                        }
                    } else if key.starts_with("file_write_scoped") {
                        if let Some(patterns) = parse_string_array(value) {
                            caps.push(Capability::FileWriteScoped(patterns));
                        }
                    } else if key.starts_with("shell_execute_scoped") {
                        if let Some(dirs) = parse_string_array(value) {
                            caps.push(Capability::ShellExecuteScoped(dirs));
                        }
                    } else if key.starts_with("network_host") {
                        if let Some(host) = value.as_str() {
                            caps.push(Capability::NetworkHost(host.to_string()));
                        }
                    }
                }
            }
        }

        if caps.is_empty() {
            // Default capabilities if none specified
            caps = vec![
                Capability::FileRead,
                Capability::FileWrite,
                Capability::ShellExecute,
                Capability::NetworkAccess,
            ];
        }

        caps
    }
}

fn parse_string_array(value: &toml::Value) -> Option<Vec<String>> {
    value.as_array().map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    })
}

/// Ensure the config directory exists
pub fn ensure_config_dir() -> std::io::Result<PathBuf> {
    let dir = ConfigFile::config_dir().expect("Could not determine config directory");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Create a sample config file if one doesn't exist
pub fn create_sample_config() -> std::io::Result<PathBuf> {
    let dir = ensure_config_dir()?;
    let path = dir.join("config.toml");

    if !path.exists() {
        let sample = r#"# Rift configuration file
# Located at ~/.config/rift/config.toml

[api]
# key = "sk-or-v1-..."
# base_url = "https://openrouter.ai/api/v1"
# model = "qwen/qwen-2.5-coder-32b-instruct"

[runtime]
max_iterations = 10
max_concurrent_tasks = 4

[agent]
auto_confirm = false

[capabilities]
file_read = true
file_write = true
shell_execute = true
network_access = true
"#;
        std::fs::write(&path, sample)?;
    }

    Ok(path)
}
