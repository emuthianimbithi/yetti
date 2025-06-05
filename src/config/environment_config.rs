use serde::{Deserialize, Serialize};
use crate::config::database::DatabaseConfig;
use crate::config::global_settings::GlobalSettings;
use crate::config::monitor_config::MonitoringConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnvironmentOverride {
    pub global_settings: Option<GlobalSettings>,
    pub databases: Option<DatabaseConfig>,
    pub monitoring: Option<MonitoringConfig>,
}