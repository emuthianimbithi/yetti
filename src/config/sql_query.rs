use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::config::{ConfigError};
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SqlQuery {
    pub sql: String,
    pub parameters: Option<HashMap<String, QueryParameter>>,
    pub validation: Option<QueryValidation>,
}
impl SqlQuery {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.sql.trim().is_empty() {
            return Err(ConfigError::MissingRequiredField("query.sql".to_string()));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryParameter {
    #[serde(rename = "type")]
    pub param_type: String,
    pub default: Option<String>,
    pub source: Option<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryValidation {
    pub strict_mapping: Option<bool>,
    pub warn_unmapped_columns: Option<bool>,
    pub validate_filter_fields: Option<bool>,
}