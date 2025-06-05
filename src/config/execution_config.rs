use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::utils::default_execution_mode;
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionConfig {
    #[serde(default = "default_execution_mode")]
    pub mode: String,
    pub global_timeout_minutes: Option<u32>,
    pub state_management: Option<StateManagement>,
    pub scheduler: Option<SchedulerConfig>,
}
impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            mode: default_execution_mode(),
            global_timeout_minutes: Some(60),
            state_management: None,
            scheduler: None,
        }
    }
}
impl ExecutionConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let valid_modes = ["parallel", "sequential"];
        if !valid_modes.contains(&self.mode.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.mode.clone()));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateManagement {
    pub enabled: bool,
    pub state_file: String,
    pub backup_states: u32,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub max_concurrent_jobs: u32,
    pub job_timeout_minutes: u32,
    pub missed_job_policy: String,
}