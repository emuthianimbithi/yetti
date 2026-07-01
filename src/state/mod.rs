use crate::config::execution_config::StateManagement;
use crate::config::query_config::QueryConfig;
use crate::config::sql_query::QueryParameter;
use crate::config::watermark_config::WatermarkStrategy;
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

static STATE_WRITE_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Clone)]
pub struct StateStore {
    path: PathBuf,
    backup_states: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct YetiiState {
    #[serde(default)]
    pub queries: BTreeMap<String, QueryState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rows_read: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_batches_sent: Option<usize>,
    #[serde(default)]
    pub watermarks: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateParameter {
    pub parameter_name: String,
    pub watermark_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatermarkComponent {
    pub watermark_name: String,
    pub value: String,
    pub cursor_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatermarkUpdate {
    pub components: Vec<WatermarkComponent>,
}

impl StateStore {
    pub fn from_config(config: &StateManagement) -> Self {
        Self {
            path: PathBuf::from(&config.state_file),
            backup_states: config.backup_states,
        }
    }

    #[cfg(test)]
    pub fn new(path: impl Into<PathBuf>, backup_states: u32) -> Self {
        Self {
            path: path.into(),
            backup_states,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_or_default(&self) -> Result<YetiiState> {
        if !self.path.exists() {
            return Ok(YetiiState::default());
        }

        let content = std::fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read state file '{}'", self.path.display()))?;
        if content.trim().is_empty() {
            return Ok(YetiiState::default());
        }

        serde_json::from_str(&content)
            .with_context(|| format!("failed to parse state file '{}'", self.path.display()))
    }

    pub fn save(&self, state: &YetiiState) -> Result<()> {
        let content = serde_json::to_string_pretty(state).context("failed to serialize state")?;
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create state directory '{}'", parent.display())
            })?;
        }

        let temp_path = temporary_path(&self.path);
        std::fs::write(&temp_path, format!("{content}\n")).with_context(|| {
            format!("failed to write temporary state '{}'", temp_path.display())
        })?;
        self.rotate_backups()?;
        if self.backup_states == 0 && self.path.exists() {
            std::fs::remove_file(&self.path).with_context(|| {
                format!("failed to replace state file '{}'", self.path.display())
            })?;
        }
        std::fs::rename(&temp_path, &self.path).with_context(|| {
            format!(
                "failed to move temporary state '{}' to '{}'",
                temp_path.display(),
                self.path.display()
            )
        })?;
        Ok(())
    }

    pub async fn record_success(
        &self,
        query_name: &str,
        started_at: DateTime<Utc>,
        rows_read: usize,
        batches_sent: usize,
        watermark: Option<WatermarkUpdate>,
    ) -> Result<YetiiState> {
        let _guard = STATE_WRITE_LOCK.lock().await;
        let store = self.clone();
        let query_name = query_name.to_string();

        tokio::task::spawn_blocking(move || {
            let mut state = store.load_or_default()?;
            state.record_success(
                &query_name,
                started_at,
                Utc::now(),
                rows_read,
                batches_sent,
                watermark.as_ref(),
            )?;
            store.save(&state)?;
            Ok(state)
        })
        .await
        .context("state persistence task failed")?
    }

    fn rotate_backups(&self) -> Result<()> {
        if self.backup_states == 0 || !self.path.exists() {
            return Ok(());
        }

        for index in (1..=self.backup_states).rev() {
            let from = if index == 1 {
                self.path.clone()
            } else {
                backup_path(&self.path, index - 1)
            };
            let to = backup_path(&self.path, index);

            if from.exists() {
                if to.exists() {
                    std::fs::remove_file(&to)
                        .with_context(|| format!("failed to remove backup '{}'", to.display()))?;
                }
                std::fs::rename(&from, &to).with_context(|| {
                    format!(
                        "failed to rotate state backup '{}' to '{}'",
                        from.display(),
                        to.display()
                    )
                })?;
            }
        }

        Ok(())
    }
}

impl YetiiState {
    pub fn query(&self, query_name: &str) -> Option<&QueryState> {
        self.queries.get(query_name)
    }

    pub fn record_success(
        &mut self,
        query_name: &str,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        rows_read: usize,
        batches_sent: usize,
        watermark: Option<&WatermarkUpdate>,
    ) -> Result<()> {
        let query_state = self.queries.entry(query_name.to_string()).or_default();
        query_state.last_started_at = Some(started_at);
        query_state.last_success_at = Some(completed_at);
        query_state.last_rows_read = Some(rows_read);
        query_state.last_batches_sent = Some(batches_sent);

        if let Some(watermark) = watermark {
            let existing = watermark
                .components
                .iter()
                .map(|component| {
                    query_state
                        .watermarks
                        .get(&component.watermark_name)
                        .map(|value| WatermarkComponent {
                            watermark_name: component.watermark_name.clone(),
                            value: value.clone(),
                            cursor_type: component.cursor_type.clone(),
                        })
                })
                .collect::<Vec<_>>();
            let populated = existing.iter().filter(|value| value.is_some()).count();
            let should_advance = match populated {
                0 => true,
                count if count == existing.len() => {
                    let current = WatermarkUpdate {
                        components: existing.into_iter().flatten().collect(),
                    };
                    compare_watermarks(watermark, &current)? == Ordering::Greater
                }
                _ => {
                    return Err(anyhow!(
                        "query '{query_name}' has a partially populated tuple watermark"
                    ));
                }
            };
            if should_advance {
                for component in &watermark.components {
                    query_state
                        .watermarks
                        .insert(component.watermark_name.clone(), component.value.clone());
                }
            }
        }
        Ok(())
    }
}

pub fn extract_watermark(
    query: &QueryConfig,
    rows: &[Map<String, Value>],
) -> Result<Option<WatermarkUpdate>> {
    let Some(watermark) = &query.watermark else {
        return Ok(None);
    };
    if watermark.strategy == WatermarkStrategy::None || rows.is_empty() {
        return Ok(None);
    }

    let columns = watermark.cursor_columns();
    let parameter_names = watermark.cursor_parameters();
    let configured_parameters = query
        .query
        .parameters
        .as_ref()
        .expect("validated incremental watermark has parameters");
    let parameters = parameter_names
        .iter()
        .map(|name| {
            configured_parameters
                .get(*name)
                .expect("validated watermark references a parameter")
        })
        .collect::<Vec<_>>();

    let mut maximum: Option<&Map<String, Value>> = None;
    for (index, row) in rows.iter().enumerate() {
        for (column, parameter) in columns.iter().zip(&parameters) {
            let value = row.get(*column).ok_or_else(|| {
                anyhow!(
                    "query '{}' row {} is missing watermark column '{}'",
                    query.name,
                    index + 1,
                    column
                )
            })?;
            if value.is_null() {
                return Err(anyhow!(
                    "query '{}' row {} has a null watermark column '{}'",
                    query.name,
                    index + 1,
                    column
                ));
            }
            validate_cursor_value(&parameter.param_type, value).with_context(|| {
                format!(
                    "query '{}' row {} has an invalid watermark value in '{}'",
                    query.name,
                    index + 1,
                    column
                )
            })?;
        }

        let is_new_maximum = match maximum {
            None => true,
            Some(current) => {
                compare_rows(&columns, &parameters, row, current)? == Ordering::Greater
            }
        };
        if is_new_maximum {
            maximum = Some(row);
        }
    }

    let maximum = maximum.expect("non-empty rows set a maximum");
    let components = columns
        .iter()
        .zip(parameter_names)
        .zip(parameters)
        .map(|((column, parameter_name), parameter)| {
            Ok(WatermarkComponent {
                watermark_name: state_watermark_name(parameter_name, parameter)
                    .expect("validated watermark references a state parameter"),
                value: cursor_to_string(
                    maximum
                        .get(*column)
                        .expect("validated maximum row has cursor column"),
                )?,
                cursor_type: parameter.param_type.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(WatermarkUpdate { components }))
}

pub fn current_watermark(
    query: &QueryConfig,
    parameters: &BTreeCompatibleParameters,
) -> Result<Option<WatermarkUpdate>> {
    let Some(watermark) = query
        .watermark
        .as_ref()
        .filter(|watermark| watermark.is_incremental())
    else {
        return Ok(None);
    };
    let components = watermark
        .cursor_parameters()
        .into_iter()
        .map(|parameter_name| {
            let parameter = parameters.get(parameter_name).ok_or_else(|| {
                anyhow!("watermark parameter '{parameter_name}' is not configured")
            })?;
            let value = parameter.default.clone().ok_or_else(|| {
                anyhow!("watermark parameter '{parameter_name}' has no resolved value")
            })?;
            Ok(WatermarkComponent {
                watermark_name: parameter_name.to_string(),
                value,
                cursor_type: parameter.param_type.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(WatermarkUpdate { components }))
}

pub fn compare_watermarks(left: &WatermarkUpdate, right: &WatermarkUpdate) -> Result<Ordering> {
    if left.components.len() != right.components.len() {
        return Err(anyhow!("watermark tuples have different lengths"));
    }
    for (left, right) in left.components.iter().zip(&right.components) {
        if !left.cursor_type.eq_ignore_ascii_case(&right.cursor_type) {
            return Err(anyhow!("watermark tuple component types do not match"));
        }
        let ordering = compare_cursor_values(
            &left.cursor_type,
            &Value::String(left.value.clone()),
            &Value::String(right.value.clone()),
        )?;
        if ordering != Ordering::Equal {
            return Ok(ordering);
        }
    }
    Ok(Ordering::Equal)
}

fn compare_rows(
    columns: &[&str],
    parameters: &[&QueryParameter],
    left: &Map<String, Value>,
    right: &Map<String, Value>,
) -> Result<Ordering> {
    for (column, parameter) in columns.iter().zip(parameters) {
        let ordering = compare_cursor_values(
            &parameter.param_type,
            left.get(*column).expect("validated row has cursor column"),
            right.get(*column).expect("validated row has cursor column"),
        )?;
        if ordering != Ordering::Equal {
            return Ok(ordering);
        }
    }
    Ok(Ordering::Equal)
}

fn compare_cursor_values(param_type: &str, left: &Value, right: &Value) -> Result<Ordering> {
    let param_type = param_type.to_ascii_lowercase();
    match param_type.as_str() {
        "int" | "integer" | "bigint" | "long" => {
            Ok(parse_integer(left)?.cmp(&parse_integer(right)?))
        }
        "float" | "double" | "decimal" | "numeric" => parse_float(left)?
            .partial_cmp(&parse_float(right)?)
            .ok_or_else(|| anyhow!("numeric watermark is not finite")),
        "timestamp" | "datetime" => Ok(parse_timestamp(left)?.cmp(&parse_timestamp(right)?)),
        "date" => Ok(parse_date(left)?.cmp(&parse_date(right)?)),
        "time" => Ok(parse_time(left)?.cmp(&parse_time(right)?)),
        _ => Err(anyhow!("unsupported watermark type '{param_type}'")),
    }
}

fn validate_cursor_value(param_type: &str, value: &Value) -> Result<()> {
    compare_cursor_values(param_type, value, value).map(|_| ())
}

fn parse_integer(value: &Value) -> Result<i128> {
    cursor_to_string(value)?
        .parse::<i128>()
        .with_context(|| format!("'{value}' is not an integer"))
}

fn parse_float(value: &Value) -> Result<f64> {
    let parsed = cursor_to_string(value)?
        .parse::<f64>()
        .with_context(|| format!("'{value}' is not numeric"))?;
    if parsed.is_finite() {
        Ok(parsed)
    } else {
        Err(anyhow!("'{value}' is not a finite number"))
    }
}

fn parse_timestamp(value: &Value) -> Result<NaiveDateTime> {
    let value = cursor_to_string(value)?;
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(&value) {
        return Ok(timestamp.naive_utc());
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(timestamp) = NaiveDateTime::parse_from_str(&value, format) {
            return Ok(timestamp);
        }
    }
    Err(anyhow!("'{value}' is not a supported timestamp"))
}

fn parse_date(value: &Value) -> Result<NaiveDate> {
    let value = cursor_to_string(value)?;
    NaiveDate::parse_from_str(&value, "%Y-%m-%d")
        .with_context(|| format!("'{value}' is not a date"))
}

fn parse_time(value: &Value) -> Result<NaiveTime> {
    let value = cursor_to_string(value)?;
    NaiveTime::parse_from_str(&value, "%H:%M:%S%.f")
        .with_context(|| format!("'{value}' is not a time"))
}

fn cursor_to_string(value: &Value) -> Result<String> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        other => Err(anyhow!(
            "watermark values must be strings or numbers, got {other}"
        )),
    }
}

pub fn resolve_query_parameters(
    query: &QueryConfig,
    parameters: &mut Option<BTreeCompatibleParameters>,
    state: &YetiiState,
) -> Result<Vec<StateParameter>> {
    let Some(parameters) = parameters.as_mut() else {
        return Ok(Vec::new());
    };

    let mut state_parameters = Vec::new();
    for (parameter_name, parameter) in parameters.iter_mut() {
        let Some(watermark_name) = state_watermark_name(parameter_name, parameter) else {
            continue;
        };

        let value = state
            .query(&query.name)
            .and_then(|query_state| query_state.watermarks.get(&watermark_name))
            .cloned()
            .or_else(|| parameter.default.clone())
            .ok_or_else(|| {
                anyhow!(
                    "query '{}' parameter '{}' uses source=state_file but has no stored value and no default",
                    query.name,
                    parameter_name
                )
            })?;

        parameter.default = Some(value);
        parameter.source = None;
        state_parameters.push(StateParameter {
            parameter_name: parameter_name.clone(),
            watermark_name,
        });
    }

    Ok(state_parameters)
}

fn state_watermark_name(parameter_name: &str, parameter: &QueryParameter) -> Option<String> {
    match parameter.source.as_deref() {
        Some("state_file") => Some(parameter_name.to_string()),
        Some(source) if source.starts_with("state_file:") => {
            let name = source.trim_start_matches("state_file:").trim();
            if name.is_empty() {
                Some(parameter_name.to_string())
            } else {
                Some(name.to_string())
            }
        }
        _ => None,
    }
}

fn backup_path(path: &Path, index: u32) -> PathBuf {
    let mut backup = OsString::from(path.as_os_str());
    backup.push(format!(".{index}"));
    PathBuf::from(backup)
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut temporary = OsString::from(path.as_os_str());
    temporary.push(format!(".tmp-{}", std::process::id()));
    PathBuf::from(temporary)
}

pub type BTreeCompatibleParameters = std::collections::HashMap<String, QueryParameter>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::endpoint_config::EndpointConfig;
    use crate::config::sql_query::SqlQuery;
    use crate::config::transform_config::TransformConfig;
    use std::collections::HashMap;

    fn temp_state_path(name: &str) -> PathBuf {
        let unique = format!(
            "yetii-state-test-{name}-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        );
        std::env::temp_dir().join(unique)
    }

    fn query_with_state_parameter(default: Option<&str>) -> QueryConfig {
        let mut parameters = HashMap::new();
        parameters.insert(
            "last_run_time".to_string(),
            QueryParameter {
                param_type: "timestamp".to_string(),
                default: default.map(str::to_string),
                source: Some("state_file".to_string()),
            },
        );

        QueryConfig {
            name: "orders_sync".to_string(),
            description: String::new(),
            enabled: true,
            database: None,
            schedule: None,
            query: SqlQuery {
                sql: "SELECT * FROM orders WHERE updated_at > $last_run_time".to_string(),
                parameters: Some(parameters),
                validation: None,
            },
            watermark: Some(crate::config::watermark_config::WatermarkConfig {
                strategy: WatermarkStrategy::Max,
                column: Some("updated_at".to_string()),
                parameter: Some("last_run_time".to_string()),
                columns: None,
                parameters: None,
                page_size: None,
            }),
            transform: TransformConfig::default(),
            endpoint: EndpointConfig {
                url: "https://example.test".to_string(),
                method: "POST".to_string(),
                auth: None,
                headers: None,
                request: Default::default(),
                response: None,
            },
        }
    }

    #[test]
    fn missing_state_uses_parameter_default() {
        let query = query_with_state_parameter(Some("1970-01-01T00:00:00Z"));
        let mut parameters = query.query.parameters.clone();

        let state_parameters =
            resolve_query_parameters(&query, &mut parameters, &YetiiState::default()).unwrap();

        assert_eq!(
            Some("1970-01-01T00:00:00Z"),
            parameters
                .as_ref()
                .unwrap()
                .get("last_run_time")
                .unwrap()
                .default
                .as_deref()
        );
        assert_eq!(
            vec![StateParameter {
                parameter_name: "last_run_time".to_string(),
                watermark_name: "last_run_time".to_string()
            }],
            state_parameters
        );
    }

    #[test]
    fn existing_state_value_overrides_default() {
        let query = query_with_state_parameter(Some("1970-01-01T00:00:00Z"));
        let mut parameters = query.query.parameters.clone();
        let mut state = YetiiState::default();
        state
            .queries
            .entry("orders_sync".to_string())
            .or_default()
            .watermarks
            .insert(
                "last_run_time".to_string(),
                "2026-07-01T10:00:00Z".to_string(),
            );

        resolve_query_parameters(&query, &mut parameters, &state).unwrap();

        assert_eq!(
            Some("2026-07-01T10:00:00Z"),
            parameters
                .as_ref()
                .unwrap()
                .get("last_run_time")
                .unwrap()
                .default
                .as_deref()
        );
    }

    #[test]
    fn missing_state_and_default_is_error() {
        let query = query_with_state_parameter(None);
        let mut parameters = query.query.parameters.clone();

        assert!(resolve_query_parameters(&query, &mut parameters, &YetiiState::default()).is_err());
    }

    #[test]
    fn state_parameters_require_an_explicit_matching_watermark() {
        let mut query = query_with_state_parameter(Some("1970-01-01T00:00:00Z"));
        query.validate().unwrap();

        query.watermark = None;

        assert!(query.validate().is_err());
    }

    #[test]
    fn record_success_updates_metadata_and_watermarks() {
        let started = "2026-07-01T09:59:58Z".parse().unwrap();
        let completed = "2026-07-01T10:00:00Z".parse().unwrap();
        let mut state = YetiiState::default();

        state
            .record_success(
                "orders_sync",
                started,
                completed,
                500,
                5,
                Some(&WatermarkUpdate {
                    components: vec![WatermarkComponent {
                        watermark_name: "last_run_time".to_string(),
                        value: "2026-07-01T09:59:59Z".to_string(),
                        cursor_type: "timestamp".to_string(),
                    }],
                }),
            )
            .unwrap();

        let query_state = state.query("orders_sync").unwrap();
        assert_eq!(Some(started), query_state.last_started_at);
        assert_eq!(Some(completed), query_state.last_success_at);
        assert_eq!(Some(500), query_state.last_rows_read);
        assert_eq!(Some(5), query_state.last_batches_sent);
        assert_eq!(
            Some(&"2026-07-01T09:59:59Z".to_string()),
            query_state.watermarks.get("last_run_time")
        );
    }

    #[test]
    fn save_rotates_backups() {
        let path = temp_state_path("rotation");
        let store = StateStore::new(&path, 2);
        let mut state = YetiiState::default();

        state
            .record_success("first", Utc::now(), Utc::now(), 1, 1, None)
            .unwrap();
        store.save(&state).unwrap();
        state
            .record_success("second", Utc::now(), Utc::now(), 2, 1, None)
            .unwrap();
        store.save(&state).unwrap();
        state
            .record_success("third", Utc::now(), Utc::now(), 3, 1, None)
            .unwrap();
        store.save(&state).unwrap();

        assert!(path.exists());
        assert!(backup_path(&path, 1).exists());
        assert!(backup_path(&path, 2).exists());

        let loaded = store.load_or_default().unwrap();
        assert!(loaded.query("third").is_some());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path(&path, 1));
        let _ = std::fs::remove_file(backup_path(&path, 2));
    }

    #[test]
    fn extracts_maximum_timestamp_from_query_rows() {
        let query = query_with_state_parameter(Some("1970-01-01T00:00:00Z"));
        let rows = vec![
            serde_json::json!({"id": 1, "updated_at": "2026-07-01T10:00:00Z"})
                .as_object()
                .unwrap()
                .clone(),
            serde_json::json!({"id": 2, "updated_at": "2026-07-01T12:30:00+03:00"})
                .as_object()
                .unwrap()
                .clone(),
            serde_json::json!({"id": 3, "updated_at": "2026-07-01T10:30:00Z"})
                .as_object()
                .unwrap()
                .clone(),
        ];

        let update = extract_watermark(&query, &rows).unwrap().unwrap();

        assert_eq!("last_run_time", update.components[0].watermark_name);
        assert_eq!("2026-07-01T10:30:00Z", update.components[0].value);
        assert_eq!("timestamp", update.components[0].cursor_type);
    }

    #[test]
    fn extracts_maximum_integer_from_query_rows() {
        let mut query = query_with_state_parameter(Some("0"));
        let parameter = query
            .query
            .parameters
            .as_mut()
            .unwrap()
            .get_mut("last_run_time")
            .unwrap();
        parameter.param_type = "bigint".to_string();
        query.watermark.as_mut().unwrap().column = Some("id".to_string());
        let rows = vec![
            serde_json::json!({"id": 9}).as_object().unwrap().clone(),
            serde_json::json!({"id": 105}).as_object().unwrap().clone(),
            serde_json::json!({"id": 42}).as_object().unwrap().clone(),
        ];

        let update = extract_watermark(&query, &rows).unwrap().unwrap();

        assert_eq!("105", update.components[0].value);
    }

    #[test]
    fn empty_results_do_not_advance_watermark() {
        let query = query_with_state_parameter(Some("1970-01-01T00:00:00Z"));

        assert_eq!(None, extract_watermark(&query, &[]).unwrap());
    }

    #[test]
    fn missing_or_null_watermark_is_an_error() {
        let query = query_with_state_parameter(Some("1970-01-01T00:00:00Z"));
        let missing = vec![serde_json::json!({"id": 1}).as_object().unwrap().clone()];
        let null = vec![
            serde_json::json!({"id": 1, "updated_at": null})
                .as_object()
                .unwrap()
                .clone(),
        ];

        assert!(extract_watermark(&query, &missing).is_err());
        assert!(extract_watermark(&query, &null).is_err());
    }

    #[test]
    fn concurrent_or_stale_success_cannot_regress_watermark() {
        let mut state = YetiiState::default();
        let newer = WatermarkUpdate {
            components: vec![WatermarkComponent {
                watermark_name: "last_run_time".to_string(),
                value: "2026-07-01T11:00:00Z".to_string(),
                cursor_type: "timestamp".to_string(),
            }],
        };
        let older = WatermarkUpdate {
            components: vec![WatermarkComponent {
                watermark_name: "last_run_time".to_string(),
                value: "2026-07-01T10:00:00Z".to_string(),
                cursor_type: "timestamp".to_string(),
            }],
        };

        state
            .record_success("orders_sync", Utc::now(), Utc::now(), 1, 1, Some(&newer))
            .unwrap();
        state
            .record_success("orders_sync", Utc::now(), Utc::now(), 1, 1, Some(&older))
            .unwrap();

        assert_eq!(
            Some(&newer.components[0].value),
            state
                .query("orders_sync")
                .unwrap()
                .watermarks
                .get("last_run_time")
        );
    }

    #[test]
    fn extracts_lexicographic_maximum_from_arbitrary_tuple() {
        let mut query = query_with_state_parameter(Some("0"));
        let parameters = query.query.parameters.as_mut().unwrap();
        parameters.clear();
        for (name, param_type, default) in [
            ("last_tenant", "bigint", "0"),
            ("last_updated", "timestamp", "1970-01-01T00:00:00Z"),
            ("last_id", "bigint", "0"),
        ] {
            parameters.insert(
                name.to_string(),
                QueryParameter {
                    param_type: param_type.to_string(),
                    default: Some(default.to_string()),
                    source: Some("state_file".to_string()),
                },
            );
        }
        query.watermark = Some(crate::config::watermark_config::WatermarkConfig {
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
            page_size: Some(100),
        });
        query.validate().unwrap();
        let rows = vec![
            serde_json::json!({"tenant_id": 1, "updated_at": "2026-07-01T11:00:00Z", "id": 9})
                .as_object()
                .unwrap()
                .clone(),
            serde_json::json!({"tenant_id": 2, "updated_at": "2026-07-01T09:00:00Z", "id": 1})
                .as_object()
                .unwrap()
                .clone(),
            serde_json::json!({"tenant_id": 2, "updated_at": "2026-07-01T09:00:00Z", "id": 8})
                .as_object()
                .unwrap()
                .clone(),
        ];

        let update = extract_watermark(&query, &rows).unwrap().unwrap();

        assert_eq!(
            vec!["2", "2026-07-01T09:00:00Z", "8"],
            update
                .components
                .iter()
                .map(|component| component.value.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn concurrent_state_writes_merge_query_results() {
        let path = temp_state_path("concurrent");
        let first_store = StateStore::new(&path, 0);
        let second_store = first_store.clone();

        let (first, second) = tokio::join!(
            first_store.record_success("orders", Utc::now(), 10, 1, None),
            second_store.record_success("customers", Utc::now(), 20, 2, None),
        );
        first.unwrap();
        second.unwrap();

        let state = StateStore::new(&path, 0).load_or_default().unwrap();
        assert!(state.query("orders").is_some());
        assert!(state.query("customers").is_some());

        let _ = std::fs::remove_file(&path);
    }
}
