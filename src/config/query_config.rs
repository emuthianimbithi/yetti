use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::endpoint_config::EndpointConfig;
use crate::config::schedule_config::ScheduleConfig;
use crate::config::sql_query::SqlQuery;
use crate::config::transform_config::TransformConfig;
use crate::config::utils::default_true;
/// Enhanced query config with validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryConfig {
    pub name: String,
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub database: Option<String>,
    pub schedule: Option<ScheduleConfig>,
    pub query: SqlQuery,
    #[serde(default)]
    pub transform: TransformConfig,
    pub endpoint: EndpointConfig,
}
impl QueryConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(ConfigError::MissingRequiredField("query.name".to_string()));
        }

        if let Some(schedule) = &self.schedule {
            schedule.validate()?;
        }

        self.query.validate()?;
        self.transform.validate()?;
        self.endpoint.validate()?;

        Ok(())
    }
}