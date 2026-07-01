pub(crate) mod connection_config;
pub(crate) mod database;
pub(crate) mod endpoint_config;
mod environment_config;
pub(crate) mod error_handling;
pub(crate) mod execution_config;
pub(crate) mod global_settings;
pub(crate) mod logging;
pub(crate) mod monitor_config;
pub(crate) mod query_config;
pub(crate) mod request_config;
pub(crate) mod schedule_config;
pub(crate) mod security_settings;
pub(crate) mod sql_query;
pub(crate) mod transform_config;
mod utils;
pub(crate) mod watermark_config;
pub(crate) mod yetii;

use once_cell::sync::OnceCell;
use std::sync::RwLock;

pub static CONFIG: OnceCell<RwLock<yetii::YetiiConfig>> = OnceCell::new();

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid database type: {0}")]
    InvalidDatabaseType(String),
    #[error("invalid schedule format: {0}")]
    InvalidSchedule(String),
    #[error("missing required field: {0}")]
    MissingRequiredField(String),
    #[error("invalid timeout value: {0:?}")]
    InvalidTimeout(Option<u32>),
    #[error("invalid port: {0}")]
    InvalidPort(u16),
    #[error("invalid HTTP method: {0}")]
    InvalidHttpMethod(String),
    #[error("invalid execution mode: {0}")]
    InvalidExecutionMode(String),
    #[error("invalid configuration value for {field}: {value}")]
    InvalidValue { field: String, value: String },
    #[error("configuration not initialized; call load_config_once() first")]
    NotInitialized,
    #[error("configuration lock is poisoned")]
    LockPoisoned,
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("configuration serialization error: {0}")]
    SerializationError(#[from] serde_yaml::Error),
    #[error("configuration has already been initialized")]
    ConfigAlreadySet,
    #[error("environment variable '{0}' referenced by configuration is not set")]
    MissingEnvironmentVariable(String),
}

/// Load configuration from a file path
pub fn load_config(path: &str) -> Result<yetii::YetiiConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let content = interpolate_env_vars(&content)?;
    let config: yetii::YetiiConfig = serde_yaml::from_str(&content)?;

    // Validate the configuration
    config.validate()?;

    Ok(config)
}

fn interpolate_env_vars(content: &str) -> Result<String, ConfigError> {
    let mut output = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            output.push_str(&rest[start..]);
            return Ok(output);
        };

        let name = &after_start[..end];
        let value = std::env::var(name)
            .map_err(|_| ConfigError::MissingEnvironmentVariable(name.to_string()))?;
        output.push_str(&value);
        rest = &after_start[end + 1..];
    }

    output.push_str(rest);
    Ok(output)
}

/// Load configuration once into the global CONFIG static
pub fn load_config_once(path: &str) -> Result<(), ConfigError> {
    let config = load_config(path)?;
    CONFIG
        .set(RwLock::new(config))
        .map_err(|_| ConfigError::ConfigAlreadySet)?;
    Ok(())
}

/// Get a read guard to the global configuration
/// Returns an error if config is not initialized or lock is poisoned
pub fn get_config() -> Result<std::sync::RwLockReadGuard<'static, yetii::YetiiConfig>, ConfigError>
{
    let config = CONFIG.get().ok_or(ConfigError::NotInitialized)?;
    config.read().map_err(|_| ConfigError::LockPoisoned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_environment_variables_in_yaml_content() {
        unsafe {
            std::env::set_var("YETII_TEST_SECRET", "resolved-secret");
        }

        let content = "password: ${YETII_TEST_SECRET}\n";

        assert_eq!(
            "password: resolved-secret\n",
            interpolate_env_vars(content).unwrap()
        );
    }

    #[test]
    fn missing_environment_variable_is_an_error() {
        unsafe {
            std::env::remove_var("YETII_TEST_MISSING");
        }

        assert!(matches!(
            interpolate_env_vars("${YETII_TEST_MISSING}"),
            Err(ConfigError::MissingEnvironmentVariable(name)) if name == "YETII_TEST_MISSING"
        ));
    }

    #[test]
    fn single_database_object_yaml_still_loads() {
        let config: yetii::YetiiConfig = serde_yaml::from_str(
            r#"
version: "1.0.0"
databases:
  name: main
  type: postgres
  host: localhost
  port: 5432
  database: postgres
  auth:
    username: null
    password: null
queries: []
"#,
        )
        .unwrap();

        config.validate().unwrap();
        assert_eq!(1, config.databases.len());
        assert!(config.databases.get("main").is_some());
    }

    #[test]
    fn database_list_yaml_loads() {
        let config: yetii::YetiiConfig = serde_yaml::from_str(
            r#"
version: "1.0.0"
databases:
  - name: erp
    type: postgres
    host: localhost
    port: 5432
    database: postgres
    auth:
      username: null
      password: null
  - name: billing
    type: mssql
    host: sql.example.test
    port: 1433
    database: billing
    auth:
      username: user
      password: pass
queries: []
"#,
        )
        .unwrap();

        config.validate().unwrap();
        assert_eq!(2, config.databases.len());
        assert!(config.databases.get("erp").is_some());
        assert!(config.databases.get("billing").is_some());
    }

    #[test]
    fn duplicate_database_names_fail_validation() {
        let config: yetii::YetiiConfig = serde_yaml::from_str(
            r#"
version: "1.0.0"
databases:
  - name: main
    type: postgres
    host: localhost
    port: 5432
    database: postgres
    auth:
      username: null
      password: null
  - name: main
    type: postgres
    host: localhost
    port: 5432
    database: postgres
    auth:
      username: null
      password: null
queries: []
"#,
        )
        .unwrap();

        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidValue { field, .. }) if field == "databases.name"
        ));
    }

    #[test]
    fn multiple_databases_require_query_database() {
        let config: yetii::YetiiConfig =
            serde_yaml::from_str(&multi_database_query_yaml(None)).unwrap();

        assert!(matches!(
            config.validate(),
            Err(ConfigError::MissingRequiredField(field)) if field == "query 'sync'.database"
        ));
    }

    #[test]
    fn unknown_query_database_fails_validation() {
        let config: yetii::YetiiConfig =
            serde_yaml::from_str(&multi_database_query_yaml(Some("missing"))).unwrap();

        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidValue { field, value })
                if field == "query 'sync'.database" && value == "missing"
        ));
    }

    #[test]
    fn max_watermark_requires_enabled_state_management() {
        let config: yetii::YetiiConfig =
            serde_yaml::from_str(&watermark_query_yaml(false)).unwrap();

        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidValue { field, .. })
                if field == "query 'sync'.watermark"
        ));

        let config: yetii::YetiiConfig = serde_yaml::from_str(&watermark_query_yaml(true)).unwrap();
        config.validate().unwrap();
    }

    fn multi_database_query_yaml(database: Option<&str>) -> String {
        let database_line = database
            .map(|name| format!("    database: {name}\n"))
            .unwrap_or_default();
        format!(
            r#"
version: "1.0.0"
databases:
  - name: erp
    type: postgres
    host: localhost
    port: 5432
    database: postgres
    auth:
      username: null
      password: null
  - name: billing
    type: postgres
    host: localhost
    port: 5432
    database: postgres
    auth:
      username: null
      password: null
queries:
  - name: sync
    description: sync
    enabled: true
{database_line}    query:
      sql: SELECT 1
    endpoint:
      url: http://127.0.0.1/sync
      method: POST
"#
        )
    }

    fn watermark_query_yaml(state_enabled: bool) -> String {
        format!(
            r#"
version: "1.0.0"
databases:
  name: main
  type: postgres
  host: localhost
  port: 5432
  database: postgres
  auth:
    username: null
    password: null
queries:
  - name: sync
    description: sync
    enabled: true
    query:
      sql: SELECT id FROM orders WHERE id > $last_id
      parameters:
        last_id:
          type: bigint
          default: "0"
          source: state_file
    watermark:
      strategy: max
      column: id
      parameter: last_id
    endpoint:
      url: http://127.0.0.1/sync
      method: POST
execution:
  state_management:
    enabled: {state_enabled}
    state_file: ./state/yetii_state.json
    backup_states: 2
"#
        )
    }
}
