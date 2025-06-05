pub(crate) mod yetii;
pub(crate) mod database;
pub(crate) mod global_settings;
pub(crate) mod connection_config;
pub(crate) mod error_handling;
pub(crate) mod query_config;
pub(crate) mod schedule_config;
mod utils;
pub(crate) mod security_settings;
pub(crate) mod logging;
pub(crate) mod sql_query;
pub(crate) mod transform_config;
pub(crate) mod endpoint_config;
pub(crate) mod request_config;
pub(crate) mod execution_config;
pub(crate) mod monitor_config;
mod environment_config;

use std::fmt;
use once_cell::sync::OnceCell;
use std::sync::RwLock;

pub static CONFIG: OnceCell<RwLock<yetii::YetiiConfig>> = OnceCell::new();

// Custom error type for configuration validation
#[derive(Debug)]
pub enum ConfigError {
    InvalidDatabaseType(String),
    InvalidSchedule(String),
    MissingRequiredField(String),
    InvalidTimeout(Option<u32>),
    NotInitialized,
    LockPoisoned,
    IoError(std::io::Error),
    SerializationError(serde_yaml::Error),
    ConfigAlreadySet,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::InvalidDatabaseType(db_type) => {
                write!(f, "Invalid database type: {}", db_type)
            }
            ConfigError::InvalidSchedule(schedule) => {
                write!(f, "Invalid schedule format: {}", schedule)
            }
            ConfigError::MissingRequiredField(field) => {
                write!(f, "Missing required field: {}", field)
            }
            ConfigError::InvalidTimeout(timeout) => {
                write!(f, "Invalid timeout value: {:?}", timeout)
            }
            ConfigError::NotInitialized => {
                write!(f, "Configuration not initialized. Call load_config_once() first")
            }
            ConfigError::LockPoisoned => {
                write!(f, "Configuration lock is poisoned")
            }
            ConfigError::IoError(err) => {
                write!(f, "IO error: {}", err)
            }
            ConfigError::SerializationError(err) => {
                write!(f, "Serialization error: {}", err)
            }
            ConfigError::ConfigAlreadySet => {
                write!(f, "Configuration has already been initialized")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::IoError(err) => Some(err),
            ConfigError::SerializationError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        ConfigError::IoError(err)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(err: serde_yaml::Error) -> Self {
        ConfigError::SerializationError(err)
    }
}

/// Load configuration from a file path
pub fn load_config(path: &str) -> Result<yetii::YetiiConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let config: yetii::YetiiConfig = serde_yaml::from_str(&content)?;

    // Validate the configuration
    validate_config(&config)?;

    Ok(config)
}

/// Load configuration once into the global CONFIG static
pub fn load_config_once(path: &str) -> Result<(), ConfigError> {
    let config = load_config(path)?;
    CONFIG
        .set(RwLock::new(config))
        .map_err(|_| ConfigError::ConfigAlreadySet)?;
    Ok(())
}

/// Reload configuration from file path
pub fn reload_config(path: &str) -> Result<(), ConfigError> {
    let new_config = load_config(path)?;

    let config = CONFIG.get().ok_or(ConfigError::NotInitialized)?;
    let mut guard = config.write().map_err(|_| ConfigError::LockPoisoned)?;
    *guard = new_config;

    println!("🔄 Config reloaded successfully");
    Ok(())
}

/// Get a read guard to the global configuration
/// Returns an error if config is not initialized or lock is poisoned
pub fn get_config() -> Result<std::sync::RwLockReadGuard<'static, yetii::YetiiConfig>, ConfigError> {
    let config = CONFIG.get().ok_or(ConfigError::NotInitialized)?;
    config.read().map_err(|_| ConfigError::LockPoisoned)
}

/// Get a read guard to the global configuration (unsafe version for internal use)
/// Panics if config is not initialized or lock is poisoned

#[allow(unused)]
pub(crate) fn get_config_unchecked() -> std::sync::RwLockReadGuard<'static, yetii::YetiiConfig> {
    CONFIG
        .get()
        .expect("CONFIG not initialized")
        .read()
        .expect("CONFIG lock poisoned")
}

/// Check if the global configuration has been initialized
pub fn is_config_initialized() -> bool {
    CONFIG.get().is_some()
}

/// Validate a configuration struct
pub fn validate_config(config: &yetii::YetiiConfig) -> Result<(), ConfigError> {
    // Validate database configuration
    if config.databases.host.trim().is_empty() {
        return Err(ConfigError::MissingRequiredField("databases.host".to_string()));
    }

    if config.databases.database.trim().is_empty() {
        return Err(ConfigError::MissingRequiredField("databases.database".to_string()));
    }

    #[allow(unused_comparisons)]
    // Validate port range
    if config.databases.port == 0 || config.databases.port > 65535 {
        return Err(ConfigError::MissingRequiredField("databases.port must be between 1 and 65535".to_string()));
    }

    // Validate timeout values
    if let Some(timeout) = config.databases.pool.timeout_seconds {
        if timeout == 0 {
            return Err(ConfigError::InvalidTimeout(Some(timeout)));
        }
    }

    // Validate global settings
    if config.global_settings.environment.trim().is_empty() {
        return Err(ConfigError::MissingRequiredField("global_settings.environment".to_string()));
    }

    // Validate queries
    for (index, query) in config.queries.iter().enumerate() {
        if query.name.trim().is_empty() {
            return Err(ConfigError::MissingRequiredField(
                format!("queries[{}].name", index)
            ));
        }

        if query.query.sql.trim().is_empty() {
            return Err(ConfigError::MissingRequiredField(
                format!("queries[{}].query.sql", index)
            ));
        }

        // Validate schedule if present
        if let Some(schedule) = &query.schedule {
            if schedule.enabled && schedule.cron.trim().is_empty() {
                return Err(ConfigError::InvalidSchedule(
                    format!("Empty cron expression for query '{}'", query.name)
                ));
            }
        }

        // Validate endpoint URL
        if query.endpoint.url.trim().is_empty() {
            return Err(ConfigError::MissingRequiredField(
                format!("queries[{}].endpoint.url", index)
            ));
        }
    }

    // Validate execution settings
    if let Some(timeout) = config.execution.global_timeout_minutes {
        if timeout == 0 {
            return Err(ConfigError::InvalidTimeout(Some(timeout)));
        }
    }

    Ok(())
}