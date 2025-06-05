use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::utils::{default_log_format, default_log_level, default_log_output};
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Logging {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default = "default_log_output")]
    pub output: String,
    pub file_path: Option<String>,
    pub rotation: Option<LogRotation>,
}
impl Default for Logging {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            output: default_log_output(),
            file_path: None,
            rotation: None,
        }
    }
}
impl Logging {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.level.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.level.clone()));
        }

        let valid_formats = ["json", "plain", "structured"];
        if !valid_formats.contains(&self.format.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.format.clone()));
        }

        Ok(())
    }
}
// Placeholder implementations for remaining structs
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LogRotation {
    pub max_size_mb: u32,
    pub max_files: u32,
}