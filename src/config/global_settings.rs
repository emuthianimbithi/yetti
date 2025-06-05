use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::utils::default_environment;
pub use crate::config::error_handling::ErrorHandling;
pub use crate::config::logging::Logging;
pub use crate::config::security_settings::SecuritySettings;
/// Enhanced global settings with defaults and validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalSettings {
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(default)]
    pub error_handling: ErrorHandling,
    #[serde(default)]
    pub logging: Logging,
    #[serde(default)]
    pub security: SecuritySettings,
}
impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            environment: default_environment(),
            error_handling: ErrorHandling::default(),
            logging: Logging::default(),
            security: SecuritySettings::default(),
        }
    }
}
impl GlobalSettings {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let valid_environments = ["development", "staging", "production"];
        if !valid_environments.contains(&self.environment.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.environment.clone()));
        }

        self.error_handling.validate()?;
        self.logging.validate()?;
        self.security.validate()?;

        Ok(())
    }
}