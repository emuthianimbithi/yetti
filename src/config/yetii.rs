use crate::config::ConfigError;
use crate::config::database::DatabaseConfigs;
use crate::config::environment_config::EnvironmentOverride;
use crate::config::execution_config::ExecutionConfig;
use crate::config::global_settings::GlobalSettings;
use crate::config::monitor_config::MonitoringConfig;
use crate::config::query_config::QueryConfig;
use crate::config::utils::default_version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root configuration structure for the ERP integration system.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YetiiConfig {
    #[serde(default = "default_version")]
    pub version: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub databases: DatabaseConfigs,
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
            match self.databases.resolve_for_query(query.database.as_deref()) {
                Some(_) => {}
                None if self.databases.len() > 1 && query.database.is_none() => {
                    return Err(ConfigError::MissingRequiredField(format!(
                        "query '{}'.database",
                        query.name
                    )));
                }
                None => {
                    return Err(ConfigError::InvalidValue {
                        field: format!("query '{}'.database", query.name),
                        value: query
                            .database
                            .clone()
                            .unwrap_or_else(|| "<missing>".to_string()),
                    });
                }
            }
        }

        // Validate execution config
        self.execution.validate()?;

        Ok(())
    }

    /// Gets the effective configuration for a specific environment
    #[allow(unused)]
    pub fn for_environment(&self, env: &str) -> Self {
        let mut config = self.clone();

        if let Some(overrides) = &self.environments
            && let Some(env_override) = overrides.get(env)
        {
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

        config
    }
}
