use crate::config::execution_config::StateManagement;
use crate::config::query_config::QueryConfig;
use crate::config::sql_query::QueryParameter;
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone)]
pub struct QueryRunState {
    pub started_at: DateTime<Utc>,
    pub state_parameters: Vec<StateParameter>,
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

        self.rotate_backups()?;
        std::fs::write(&self.path, format!("{content}\n"))
            .with_context(|| format!("failed to write state file '{}'", self.path.display()))?;
        Ok(())
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
        state_parameters: &[StateParameter],
    ) {
        let query_state = self.queries.entry(query_name.to_string()).or_default();
        query_state.last_started_at = Some(started_at);
        query_state.last_success_at = Some(completed_at);
        query_state.last_rows_read = Some(rows_read);
        query_state.last_batches_sent = Some(batches_sent);

        let completed = completed_at.to_rfc3339();
        for parameter in state_parameters {
            query_state
                .watermarks
                .insert(parameter.watermark_name.clone(), completed.clone());
        }
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
    fn record_success_updates_metadata_and_watermarks() {
        let started = "2026-07-01T09:59:58Z".parse().unwrap();
        let completed = "2026-07-01T10:00:00Z".parse().unwrap();
        let mut state = YetiiState::default();

        state.record_success(
            "orders_sync",
            started,
            completed,
            500,
            5,
            &[StateParameter {
                parameter_name: "last_run_time".to_string(),
                watermark_name: "last_run_time".to_string(),
            }],
        );

        let query_state = state.query("orders_sync").unwrap();
        assert_eq!(Some(started), query_state.last_started_at);
        assert_eq!(Some(completed), query_state.last_success_at);
        assert_eq!(Some(500), query_state.last_rows_read);
        assert_eq!(Some(5), query_state.last_batches_sent);
        assert_eq!(
            Some(&"2026-07-01T10:00:00+00:00".to_string()),
            query_state.watermarks.get("last_run_time")
        );
    }

    #[test]
    fn save_rotates_backups() {
        let path = temp_state_path("rotation");
        let store = StateStore::new(&path, 2);
        let mut state = YetiiState::default();

        state.record_success("first", Utc::now(), Utc::now(), 1, 1, &[]);
        store.save(&state).unwrap();
        state.record_success("second", Utc::now(), Utc::now(), 2, 1, &[]);
        store.save(&state).unwrap();
        state.record_success("third", Utc::now(), Utc::now(), 3, 1, &[]);
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
}
