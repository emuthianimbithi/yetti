use crate::config::ConfigError;
use crate::config::utils::default_execution_mode;
use serde::{Deserialize, Serialize};
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
            return Err(ConfigError::InvalidExecutionMode(self.mode.clone()));
        }
        if let Some(state_management) = &self.state_management {
            state_management.validate()?;
        }
        if let Some(scheduler) = &self.scheduler {
            scheduler.validate()?;
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

impl StateManagement {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.enabled && self.state_file.trim().is_empty() {
            return Err(ConfigError::MissingRequiredField(
                "execution.state_management.state_file".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub max_concurrent_jobs: u32,
    pub job_timeout_minutes: u32,
    pub missed_job_policy: String,
}

impl SchedulerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_concurrent_jobs == 0 {
            return Err(ConfigError::InvalidValue {
                field: "execution.scheduler.max_concurrent_jobs".to_string(),
                value: "0".to_string(),
            });
        }
        if self.missed_job_policy != "skip" {
            return Err(ConfigError::InvalidValue {
                field: "execution.scheduler.missed_job_policy".to_string(),
                value: self.missed_job_policy.clone(),
            });
        }
        Ok(())
    }
}
