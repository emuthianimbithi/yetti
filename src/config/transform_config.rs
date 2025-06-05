use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::config::{ConfigError};
use crate::config::utils::default_true;
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransformConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub mappings: Option<HashMap<String, String>>,
    pub group_by: Option<String>,
    pub filters: Option<Vec<DataFilter>>,
    pub conversions: Option<HashMap<String, DataConversion>>,
}
impl Default for TransformConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mappings: None,
            group_by: None,
            filters: None,
            conversions: None,
        }
    }
}
impl TransformConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Add specific validation logic for transformations
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DataFilter {
    pub field: String,
    pub condition: String,
    pub value: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DataConversion {
    pub from: String,
    pub to: String,
    pub format: Option<String>,
}