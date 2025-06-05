use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::database::DatabaseConfig;
use crate::config::environment_config::EnvironmentOverride;
use crate::config::execution_config::ExecutionConfig;
use crate::config::global_settings::GlobalSettings;
use crate::config::monitor_config::MonitoringConfig;
use crate::config::query_config::QueryConfig;
use crate::config::utils::default_version;

/// Root configuration structure for the ERP integration system.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YetiiConfig {
    #[serde(default = "default_version")]
    pub version: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub databases: DatabaseConfig,
    #[serde(default)]
    pub global_settings: GlobalSettings,
    pub queries: Vec<QueryConfig>,
    #[serde(default)]
    pub execution: ExecutionConfig,
    pub monitoring: Option<MonitoringConfig>,
    pub environments: Option<HashMap<String, EnvironmentOverride>>,
}
impl YetiiConfig {
    /// Validates the entire configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate version format
        if self.version.is_none() {
            return Err(ConfigError::MissingRequiredField("version".to_string()));
        }

        // Validate database configuration
        self.databases.validate()?;

        // Validate global settings
        self.global_settings.validate()?;

        // Validate all queries
        for query in &self.queries {
            query.validate()?;
        }

        // Validate execution config
        self.execution.validate()?;

        Ok(())
    }

    /// Gets the effective configuration for a specific environment
    #[allow(unused)]
    pub fn for_environment(&self, env: &str) -> Self {
        let mut config = self.clone();

        if let Some(overrides) = &self.environments {
            if let Some(env_override) = overrides.get(env) {
                if let Some(global_settings) = &env_override.global_settings {
                    config.global_settings = global_settings.clone();
                }
                if let Some(databases) = &env_override.databases {
                    config.databases = databases.clone();
                }
                if let Some(monitoring) = &env_override.monitoring {
                    config.monitoring = Some(monitoring.clone());
                }
            }
        }

        config
    }

}