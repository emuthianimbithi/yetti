use crate::config::ConfigError;
use crate::config::sql_query::QueryParameter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WatermarkStrategy {
    Max,
    MaxTuple,
    None,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WatermarkConfig {
    pub strategy: WatermarkStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<usize>,
}

impl WatermarkConfig {
    pub fn validate(
        &self,
        query_name: &str,
        parameters: Option<&HashMap<String, QueryParameter>>,
    ) -> Result<(), ConfigError> {
        match self.strategy {
            WatermarkStrategy::None => {
                if self.column.is_some()
                    || self.parameter.is_some()
                    || self.columns.is_some()
                    || self.parameters.is_some()
                    || self.page_size.is_some()
                {
                    return Err(invalid(
                        query_name,
                        "strategy=none cannot set cursor fields or page_size",
                    ));
                }
            }
            WatermarkStrategy::Max => {
                if self.columns.is_some() || self.parameters.is_some() {
                    return Err(invalid(
                        query_name,
                        "strategy=max uses column and parameter; use max_tuple for lists",
                    ));
                }
                let column = required_value(query_name, "column", self.column.as_deref())?;
                let parameter_name =
                    required_value(query_name, "parameter", self.parameter.as_deref())?;
                validate_parameter(query_name, parameter_name, parameters)?;
                if column.trim().is_empty() {
                    return Err(invalid(query_name, "column cannot be empty"));
                }
            }
            WatermarkStrategy::MaxTuple => {
                if self.column.is_some() || self.parameter.is_some() {
                    return Err(invalid(
                        query_name,
                        "strategy=max_tuple uses columns and parameters lists",
                    ));
                }
                let columns = self
                    .columns
                    .as_deref()
                    .ok_or_else(|| invalid(query_name, "strategy=max_tuple requires columns"))?;
                let parameter_names = self
                    .parameters
                    .as_deref()
                    .ok_or_else(|| invalid(query_name, "strategy=max_tuple requires parameters"))?;
                if columns.len() < 2 {
                    return Err(invalid(
                        query_name,
                        "strategy=max_tuple requires at least two columns",
                    ));
                }
                if columns.len() != parameter_names.len() {
                    return Err(invalid(
                        query_name,
                        "columns and parameters must have equal lengths",
                    ));
                }
                if columns.iter().any(|column| column.trim().is_empty()) {
                    return Err(invalid(query_name, "columns cannot contain empty names"));
                }
                if has_duplicates(columns) || has_duplicates(parameter_names) {
                    return Err(invalid(
                        query_name,
                        "columns and parameters cannot contain duplicates",
                    ));
                }
                for parameter_name in parameter_names {
                    validate_parameter(query_name, parameter_name, parameters)?;
                }
            }
        }

        if self.page_size == Some(0) {
            return Err(invalid(query_name, "page_size must be greater than zero"));
        }

        Ok(())
    }

    pub fn is_incremental(&self) -> bool {
        self.strategy != WatermarkStrategy::None
    }

    pub fn cursor_columns(&self) -> Vec<&str> {
        match self.strategy {
            WatermarkStrategy::Max => self.column.iter().map(String::as_str).collect(),
            WatermarkStrategy::MaxTuple => {
                self.columns.iter().flatten().map(String::as_str).collect()
            }
            WatermarkStrategy::None => Vec::new(),
        }
    }

    pub fn cursor_parameters(&self) -> Vec<&str> {
        match self.strategy {
            WatermarkStrategy::Max => self.parameter.iter().map(String::as_str).collect(),
            WatermarkStrategy::MaxTuple => self
                .parameters
                .iter()
                .flatten()
                .map(String::as_str)
                .collect(),
            WatermarkStrategy::None => Vec::new(),
        }
    }
}

pub fn is_state_parameter(parameter: &QueryParameter) -> bool {
    parameter
        .source
        .as_deref()
        .is_some_and(|source| source == "state_file" || source.starts_with("state_file:"))
}

pub fn is_supported_cursor_type(param_type: &str) -> bool {
    matches!(
        param_type.to_ascii_lowercase().as_str(),
        "int"
            | "integer"
            | "bigint"
            | "long"
            | "float"
            | "double"
            | "decimal"
            | "numeric"
            | "date"
            | "time"
            | "timestamp"
            | "datetime"
    )
}

fn required_value<'a>(
    query_name: &str,
    field: &str,
    value: Option<&'a str>,
) -> Result<&'a str, ConfigError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid(query_name, &format!("strategy=max requires {field}")))
}

fn validate_parameter(
    query_name: &str,
    parameter_name: &str,
    parameters: Option<&HashMap<String, QueryParameter>>,
) -> Result<(), ConfigError> {
    let parameter = parameters
        .and_then(|parameters| parameters.get(parameter_name))
        .ok_or_else(|| {
            invalid(
                query_name,
                &format!("parameter '{parameter_name}' is not defined"),
            )
        })?;
    if !is_state_parameter(parameter) {
        return Err(invalid(
            query_name,
            &format!("parameter '{parameter_name}' must use source: state_file"),
        ));
    }
    if !is_supported_cursor_type(&parameter.param_type) {
        return Err(invalid(
            query_name,
            &format!(
                "parameter '{parameter_name}' has unsupported watermark type '{}'",
                parameter.param_type
            ),
        ));
    }
    Ok(())
}

fn has_duplicates(values: &[String]) -> bool {
    let mut seen = std::collections::HashSet::new();
    values.iter().any(|value| !seen.insert(value))
}

fn invalid(query_name: &str, reason: &str) -> ConfigError {
    ConfigError::InvalidValue {
        field: format!("query '{query_name}'.watermark"),
        value: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_parameter(param_type: &str) -> QueryParameter {
        QueryParameter {
            param_type: param_type.to_string(),
            default: Some("0".to_string()),
            source: Some("state_file".to_string()),
        }
    }

    #[test]
    fn max_requires_a_known_state_parameter() {
        let watermark = WatermarkConfig {
            strategy: WatermarkStrategy::Max,
            column: Some("id".to_string()),
            parameter: Some("last_id".to_string()),
            columns: None,
            parameters: None,
            page_size: None,
        };
        let mut parameters = HashMap::new();
        parameters.insert("last_id".to_string(), state_parameter("bigint"));

        watermark.validate("orders", Some(&parameters)).unwrap();

        parameters.get_mut("last_id").unwrap().source = None;
        assert!(watermark.validate("orders", Some(&parameters)).is_err());
        assert!(watermark.validate("orders", None).is_err());
    }

    #[test]
    fn none_rejects_cursor_fields() {
        let watermark = WatermarkConfig {
            strategy: WatermarkStrategy::None,
            column: Some("updated_at".to_string()),
            parameter: None,
            columns: None,
            parameters: None,
            page_size: None,
        };

        assert!(watermark.validate("orders", None).is_err());
    }

    #[test]
    fn max_rejects_unsupported_parameter_types() {
        let watermark = WatermarkConfig {
            strategy: WatermarkStrategy::Max,
            column: Some("cursor".to_string()),
            parameter: Some("cursor".to_string()),
            columns: None,
            parameters: None,
            page_size: None,
        };
        let mut parameters = HashMap::new();
        parameters.insert("cursor".to_string(), state_parameter("boolean"));

        assert!(watermark.validate("orders", Some(&parameters)).is_err());
    }

    #[test]
    fn max_tuple_accepts_matching_arbitrary_length_lists() {
        let watermark = WatermarkConfig {
            strategy: WatermarkStrategy::MaxTuple,
            column: None,
            parameter: None,
            columns: Some(vec![
                "tenant_id".to_string(),
                "updated_at".to_string(),
                "id".to_string(),
            ]),
            parameters: Some(vec![
                "last_tenant".to_string(),
                "last_updated".to_string(),
                "last_id".to_string(),
            ]),
            page_size: Some(1000),
        };
        let mut parameters = HashMap::new();
        parameters.insert("last_tenant".to_string(), state_parameter("bigint"));
        parameters.insert("last_updated".to_string(), state_parameter("timestamp"));
        parameters.insert("last_id".to_string(), state_parameter("bigint"));

        watermark.validate("orders", Some(&parameters)).unwrap();
        assert_eq!(
            vec!["tenant_id", "updated_at", "id"],
            watermark.cursor_columns()
        );
    }

    #[test]
    fn max_tuple_rejects_mismatched_or_duplicate_lists() {
        let mut watermark = WatermarkConfig {
            strategy: WatermarkStrategy::MaxTuple,
            column: None,
            parameter: None,
            columns: Some(vec!["updated_at".to_string(), "id".to_string()]),
            parameters: Some(vec!["last_updated".to_string()]),
            page_size: Some(100),
        };
        assert!(watermark.validate("orders", None).is_err());

        watermark.parameters = Some(vec!["cursor".to_string(), "cursor".to_string()]);
        assert!(watermark.validate("orders", None).is_err());
    }
}
