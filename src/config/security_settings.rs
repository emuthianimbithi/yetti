use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::utils::default_timeout_seconds;
use crate::config::utils::default_false;
use crate::config::utils::default_true;
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecuritySettings {
    #[serde(default = "default_false")]
    pub encrypt_config: bool,
    #[serde(default = "default_true")]
    pub validate_ssl: bool,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: Option<u32>,
}
impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            encrypt_config: false,
            validate_ssl: false,
            timeout_seconds: default_timeout_seconds(),
        }
    }
}
impl SecuritySettings {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.timeout_seconds > Some(600) {
            return Err(ConfigError::InvalidTimeout(Option::from(self.timeout_seconds)));
        }
        Ok(())
    }
}