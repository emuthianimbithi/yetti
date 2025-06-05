pub(crate) mod yetii;
pub(crate) mod database;
pub(crate) mod global_settings;
pub(crate) mod connection_config;
pub(crate) mod error_handling;
pub(crate) mod query_config;
pub(crate) mod schedule_config;
mod utils;
pub(crate) mod security_settings;
pub(crate)mod logging;
pub(crate) mod sql_query;
pub(crate) mod transform_config;
pub(crate) mod endpoint_config;
pub(crate) mod request_config;
pub(crate) mod execution_config;
pub(crate) mod monitor_config;
mod environment_config;

use std::{fmt};
use once_cell::sync::OnceCell;
use std::sync::RwLock;

pub static CONFIG: OnceCell<RwLock<yetii::YetiiConfig>> = OnceCell::new();
pub fn load_config(path: &str) -> Result<yetii::YetiiConfig, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let config: yetii::YetiiConfig = serde_yaml::from_str(&content)?;
    Ok(config)
}
pub fn load_config_once(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config(path)?;
    CONFIG.set(RwLock::new(config)).map_err(|_| "Config already set!")?;
    Ok(())
}
pub fn reload_config(path: &str) {
    match load_config(path) {
        Ok(cfg) => {
            if let Some(lock) = CONFIG.get() {
                let mut guard = lock.write().unwrap();
                *guard = cfg;
                println!("🔄 Config reloaded successfully");
            }
        }
        Err(e) => eprintln!("❌ Failed to reload config: {}", e),
    }
}
pub fn get_config() -> std::sync::RwLockReadGuard<'static, yetii::YetiiConfig> {
    CONFIG.get()
        .expect("CONFIG not initialized")
        .read()
        .expect("CONFIG lock poisoned")
}
// Custom error type for configuration validation
#[derive(Debug)]
pub enum ConfigError {
    InvalidDatabaseType(String),
    InvalidSchedule(String),
    MissingRequiredField(String),
    InvalidTimeout(Option<u32>),
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::InvalidDatabaseType(db_type) => write!(f, "Invalid database type: {}", db_type),
            ConfigError::InvalidSchedule(schedule) => write!(f, "Invalid schedule format: {}", schedule),
            ConfigError::MissingRequiredField(field) => write!(f, "Missing required field: {}", field),
            ConfigError::InvalidTimeout(timeout) => write!(f, "Invalid timeout value: {:?}", timeout),
        }
    }
}
impl std::error::Error for ConfigError {}