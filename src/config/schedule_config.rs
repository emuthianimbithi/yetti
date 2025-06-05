use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::utils:: default_true;
use crate::config::utils::default_timezone;
/// Enhanced schedule config with cron validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleConfig {
    pub cron: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
impl ScheduleConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Basic cron validation - you might want to use a proper cron parser
        let parts: Vec<&str> = self.cron.split_whitespace().collect();
        if parts.len() != 5 && parts.len() != 6 {
            return Err(ConfigError::InvalidSchedule(self.cron.clone()));
        }

        Ok(())
    }
}