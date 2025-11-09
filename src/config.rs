use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub alerts: AlertConfig,
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    #[serde(default)]
    pub slack: Option<SlackConfig>,
    #[serde(default)]
    pub webhook: Option<WebhookConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    #[serde(default = "default_enabled_channels")]
    pub enabled_channels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    #[serde(default = "default_check_interval")]
    pub check_interval_seconds: u64,
    #[serde(default = "default_disk_warning")]
    pub disk_threshold_warning: u8,
    #[serde(default = "default_disk_critical")]
    pub disk_threshold_critical: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    pub webhook_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            alerts: AlertConfig::default(),
            monitoring: MonitoringConfig::default(),
            slack: None,
            webhook: None,
        }
    }
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            enabled_channels: default_enabled_channels(),
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            check_interval_seconds: default_check_interval(),
            disk_threshold_warning: default_disk_warning(),
            disk_threshold_critical: default_disk_critical(),
        }
    }
}

fn default_enabled_channels() -> Vec<String> {
    vec!["console".to_string()]
}

fn default_check_interval() -> u64 {
    300
}

fn default_disk_warning() -> u8 {
    80
}

fn default_disk_critical() -> u8 {
    90
}

impl Config {
    /// Get the default config file path
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Could not determine config directory")?
            .join("catdog");

        Ok(config_dir.join("config.toml"))
    }

    /// Load configuration from file, or create default if it doesn't exist
    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;

        if !path.exists() {
            // Create default config
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = Self::default_path()?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let contents = toml::to_string_pretty(self).context("Failed to serialize configuration")?;

        fs::write(&path, contents)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        Ok(())
    }

    /// Get the path to display to users
    pub fn display_path() -> String {
        Self::default_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "~/.config/catdog/config.toml".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.monitoring.check_interval_seconds, 300);
        assert_eq!(config.monitoring.disk_threshold_warning, 80);
        assert_eq!(config.monitoring.disk_threshold_critical, 90);
        assert_eq!(config.alerts.enabled_channels, vec!["console"]);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("[alerts]"));
        assert!(toml_str.contains("[monitoring]"));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
[alerts]
enabled_channels = ["console", "slack"]

[monitoring]
check_interval_seconds = 60
disk_threshold_warning = 75
disk_threshold_critical = 95
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.monitoring.check_interval_seconds, 60);
        assert_eq!(config.monitoring.disk_threshold_warning, 75);
        assert_eq!(config.alerts.enabled_channels.len(), 2);
    }
}
