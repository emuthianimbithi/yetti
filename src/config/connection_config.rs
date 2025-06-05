use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::utils::default_max_connections;
use crate::config::utils::default_retry_attempts;
use crate::config::utils::default_timeout_seconds;
/// Enhanced connection config with validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionConfig {
    #[serde(default = "default_max_connections")]
    pub max_connections: Option<u32>,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: Option<u32>,
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: Option<u32>,
}
impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: Option::from(default_max_connections()),
            timeout_seconds: Option::from(default_timeout_seconds()),
            retry_attempts: Option::from(default_retry_attempts()),
        }
    }
}
impl ConnectionConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_connections == Some(0) || self.max_connections > Some(1000) {
            return Err(ConfigError::InvalidTimeout(self.max_connections));
        }

        if self.timeout_seconds > Some(300) {
            return Err(ConfigError::InvalidTimeout(self.timeout_seconds));
        }

        Ok(())
    }
}