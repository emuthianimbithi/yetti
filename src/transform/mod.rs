use crate::config::transform_config::{DataConversion, DataFilter, TransformConfig};
use serde_json::{Map, Number, Value};

#[derive(Debug, thiserror::Error)]
pub enum TransformError {
    #[error("unsupported transform filter condition '{0}'")]
    UnsupportedFilter(String),
    #[error("unsupported transform conversion target '{0}'")]
    UnsupportedConversion(String),
    #[error("failed to convert field '{field}' to {target}: {reason}")]
    Conversion {
        field: String,
        target: String,
        reason: String,
    },
    #[error("group_by transforms are not implemented yet")]
    GroupByUnsupported,
}

pub fn apply(
    rows: Vec<Map<String, Value>>,
    transform: &TransformConfig,
) -> Result<Vec<Map<String, Value>>, TransformError> {
    if !transform.enabled {
        return Ok(rows);
    }
    if transform.group_by.is_some() {
        return Err(TransformError::GroupByUnsupported);
    }

    let mut rows = apply_filters(rows, transform.filters.as_deref())?;
    apply_conversions(&mut rows, transform.conversions.as_ref())?;
    apply_mappings(&mut rows, transform.mappings.as_ref());
    Ok(rows)
}

fn apply_filters(
    rows: Vec<Map<String, Value>>,
    filters: Option<&[DataFilter]>,
) -> Result<Vec<Map<String, Value>>, TransformError> {
    let Some(filters) = filters else {
        return Ok(rows);
    };

    rows.into_iter()
        .filter_map(|row| {
            match filters.iter().try_fold(true, |keep, filter| {
                Ok(keep && row_matches_filter(&row, filter)?)
            }) {
                Ok(true) => Some(Ok(row)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect()
}

fn row_matches_filter(
    row: &Map<String, Value>,
    filter: &DataFilter,
) -> Result<bool, TransformError> {
    let value = row.get(&filter.field).unwrap_or(&Value::Null);
    match filter.condition.as_str() {
        "not_null" => Ok(!value.is_null()),
        "is_null" => Ok(value.is_null()),
        "eq" | "equals" => Ok(filter.value.as_ref() == Some(value)),
        "ne" | "not_equals" => Ok(filter
            .value
            .as_ref()
            .is_some_and(|expected| expected != value)),
        other => Err(TransformError::UnsupportedFilter(other.to_string())),
    }
}

fn apply_conversions(
    rows: &mut [Map<String, Value>],
    conversions: Option<&std::collections::HashMap<String, DataConversion>>,
) -> Result<(), TransformError> {
    let Some(conversions) = conversions else {
        return Ok(());
    };

    for row in rows {
        for (field, conversion) in conversions {
            let Some(value) = row.get(field).cloned() else {
                continue;
            };
            row.insert(
                field.clone(),
                convert_value(field, value, conversion.to.as_str())?,
            );
        }
    }
    Ok(())
}

fn convert_value(field: &str, value: Value, target: &str) -> Result<Value, TransformError> {
    if value.is_null() {
        return Ok(Value::Null);
    }

    match target {
        "string" | "text" | "iso8601_string" => Ok(match value {
            Value::String(value) => Value::String(value),
            Value::Number(value) => Value::String(value.to_string()),
            Value::Bool(value) => Value::String(value.to_string()),
            other => Value::String(other.to_string()),
        }),
        "integer" | "int" => {
            let value = as_i64(field, &value, target)?;
            Ok(Value::Number(Number::from(value)))
        }
        "number" | "float" | "double" | "decimal" => {
            let value = as_f64(field, &value, target)?;
            Number::from_f64(value)
                .map(Value::Number)
                .ok_or_else(|| TransformError::Conversion {
                    field: field.to_string(),
                    target: target.to_string(),
                    reason: "number is not finite".to_string(),
                })
        }
        "bool" | "boolean" => Ok(Value::Bool(as_bool(field, &value, target)?)),
        other => Err(TransformError::UnsupportedConversion(other.to_string())),
    }
}

fn as_i64(field: &str, value: &Value, target: &str) -> Result<i64, TransformError> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(value) => value.parse::<i64>().ok(),
        Value::Bool(value) => Some(i64::from(*value)),
        _ => None,
    }
    .ok_or_else(|| TransformError::Conversion {
        field: field.to_string(),
        target: target.to_string(),
        reason: format!("cannot convert {value} to integer"),
    })
}

fn as_f64(field: &str, value: &Value, target: &str) -> Result<f64, TransformError> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(value) => value.parse::<f64>().ok(),
        Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        _ => None,
    }
    .ok_or_else(|| TransformError::Conversion {
        field: field.to_string(),
        target: target.to_string(),
        reason: format!("cannot convert {value} to number"),
    })
}

fn as_bool(field: &str, value: &Value, target: &str) -> Result<bool, TransformError> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_i64().map(|value| value != 0),
        Value::String(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "t" | "yes" | "y" | "1" => Some(true),
            "false" | "f" | "no" | "n" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
    .ok_or_else(|| TransformError::Conversion {
        field: field.to_string(),
        target: target.to_string(),
        reason: format!("cannot convert {value} to bool"),
    })
}

fn apply_mappings(
    rows: &mut [Map<String, Value>],
    mappings: Option<&std::collections::HashMap<String, String>>,
) {
    let Some(mappings) = mappings else {
        return;
    };

    for row in rows {
        let mut replacements = Vec::new();
        for (from, to) in mappings {
            if let Some(value) = row.remove(from) {
                replacements.push((to.clone(), value));
            }
        }
        for (to, value) in replacements {
            row.insert(to, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::transform_config::DataConversion;
    use std::collections::HashMap;

    #[test]
    fn filters_converts_and_maps_rows() {
        let mut mappings = HashMap::new();
        mappings.insert("amount".to_string(), "total_amount".to_string());
        let mut conversions = HashMap::new();
        conversions.insert(
            "amount".to_string(),
            DataConversion {
                from: "string".to_string(),
                to: "number".to_string(),
                format: None,
            },
        );
        conversions.insert(
            "active".to_string(),
            DataConversion {
                from: "string".to_string(),
                to: "bool".to_string(),
                format: None,
            },
        );
        let transform = TransformConfig {
            enabled: true,
            mappings: Some(mappings),
            group_by: None,
            filters: Some(vec![DataFilter {
                field: "email".to_string(),
                condition: "not_null".to_string(),
                value: None,
            }]),
            conversions: Some(conversions),
        };
        let rows = vec![
            serde_json::json!({"email": "a@example.test", "amount": "42.5", "active": "true"})
                .as_object()
                .unwrap()
                .clone(),
            serde_json::json!({"email": null, "amount": "99", "active": "false"})
                .as_object()
                .unwrap()
                .clone(),
        ];

        let rows = apply(rows, &transform).unwrap();

        assert_eq!(1, rows.len());
        assert_eq!(serde_json::json!(42.5), rows[0]["total_amount"]);
        assert_eq!(serde_json::json!(true), rows[0]["active"]);
        assert!(rows[0].get("amount").is_none());
    }

    #[test]
    fn disabled_transform_is_passthrough() {
        let rows = vec![serde_json::json!({"a": 1}).as_object().unwrap().clone()];
        let transform = TransformConfig {
            enabled: false,
            ..TransformConfig::default()
        };

        assert_eq!(rows, apply(rows.clone(), &transform).unwrap());
    }
}
