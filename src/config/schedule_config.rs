use crate::config::ConfigError;
use crate::config::utils::default_timezone;
use crate::config::utils::default_true;
use serde::{Deserialize, Serialize};
use tokio_cron_scheduler::Job;
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
        normalized_cron(&self.cron)?;

        Ok(())
    }
}

pub fn normalized_cron(cron: &str) -> Result<String, ConfigError> {
    let parts = cron.split_whitespace().collect::<Vec<_>>();
    let normalized = match parts.len() {
        5 => format!("0 {cron}"),
        6 => cron.to_string(),
        _ => return Err(ConfigError::InvalidSchedule(cron.to_string())),
    };

    Job::new_async(normalized.clone(), |_uuid, _lock| Box::pin(async {}))
        .map_err(|_| ConfigError::InvalidSchedule(cron.to_string()))?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_five_field_cron_to_seconds_required_format() {
        assert_eq!("0 */5 * * * *", normalized_cron("*/5 * * * *").unwrap());
    }

    #[test]
    fn accepts_six_field_cron() {
        assert_eq!("*/10 * * * * *", normalized_cron("*/10 * * * * *").unwrap());
    }

    #[test]
    fn rejects_invalid_cron() {
        assert!(normalized_cron("not a cron").is_err());
    }
}
