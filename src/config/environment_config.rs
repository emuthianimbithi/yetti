use crate::config::database::DatabaseConfigs;
use crate::config::global_settings::GlobalSettings;
use crate::config::monitor_config::MonitoringConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnvironmentOverride {
    pub global_settings: Option<GlobalSettings>,
    pub databases: Option<DatabaseConfigs>,
    pub monitoring: Option<MonitoringConfig>,
}
