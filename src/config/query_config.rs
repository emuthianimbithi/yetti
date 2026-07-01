use crate::config::ConfigError;
use crate::config::endpoint_config::EndpointConfig;
use crate::config::schedule_config::ScheduleConfig;
use crate::config::sql_query::SqlQuery;
use crate::config::transform_config::TransformConfig;
use crate::config::utils::default_true;
use crate::config::watermark_config::{WatermarkConfig, is_state_parameter};
use serde::{Deserialize, Serialize};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark: Option<WatermarkConfig>,
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
        if let Some(watermark) = &self.watermark {
            watermark.validate(&self.name, self.query.parameters.as_ref())?;
        }

        let state_parameters = self
            .query
            .parameters
            .iter()
            .flat_map(|parameters| parameters.iter())
            .filter(|(_, parameter)| is_state_parameter(parameter))
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        let configured_parameters = self
            .watermark
            .as_ref()
            .map(WatermarkConfig::cursor_parameters)
            .unwrap_or_default();
        for parameter in state_parameters {
            if !configured_parameters.contains(&parameter) {
                return Err(ConfigError::InvalidValue {
                    field: format!("query '{}'.query.parameters.{parameter}.source", self.name),
                    value:
                        "state_file parameters require a matching max/max_tuple watermark parameter"
                            .to_string(),
                });
            }
        }

        self.transform.validate()?;
        self.endpoint.validate()?;

        Ok(())
    }
}
