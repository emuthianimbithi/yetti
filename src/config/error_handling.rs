use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::utils::default_error_action;
use crate::config::utils::default_max_retries;
/// Enhanced error handling with validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorHandling {
    #[serde(default = "default_error_action")]
    pub on_query_error: String,
    #[serde(default = "default_error_action")]
    pub on_transform_error: String,
    #[serde(default = "default_error_action")]
    pub on_endpoint_error: String,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}
impl Default for ErrorHandling {
    fn default() -> Self {
        Self {
            on_query_error: default_error_action(),
            on_transform_error: default_error_action(),
            on_endpoint_error: default_error_action(),
            max_retries: default_max_retries(),
        }
    }
}
impl ErrorHandling {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let valid_actions = ["stop", "log_and_continue", "retry"];

        if !valid_actions.contains(&self.on_query_error.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.on_query_error.clone()));
        }

        if self.max_retries > 10 {
            return Err(ConfigError::InvalidTimeout(Option::from(self.max_retries)));
        }

        Ok(())
    }
}