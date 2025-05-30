use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::{env, fmt, fs};
use std::io::Error;
use std::path::PathBuf;
use std::sync::RwLock;
use once_cell::sync::Lazy;

pub static CONFIG: Lazy<RwLock<YetiiConfig>> = Lazy::new(|| {
    let config = YetiiConfig::from_file(None)
        .expect("Failed to load configuration file");
    RwLock::new(config)
});

// Custom error type for configuration validation
#[derive(Debug)]
pub enum ConfigError {
    InvalidDatabaseType(String),
    InvalidSchedule(String),
    MissingRequiredField(String),
    InvalidTimeout(Option<u32>),
    IoError(Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::InvalidDatabaseType(db_type) => write!(f, "Invalid database type: {}", db_type),
            ConfigError::InvalidSchedule(schedule) => write!(f, "Invalid schedule format: {}", schedule),
            ConfigError::MissingRequiredField(field) => write!(f, "Missing required field: {}", field),
            ConfigError::InvalidTimeout(timeout) => write!(f, "Invalid timeout value: {:?}", timeout),
            ConfigError::IoError(err) => write!(f, "I/O error: {}", err),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Root configuration structure for the ERP integration system.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YetiiConfig {
    #[serde(default = "default_version")]
    pub version: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub databases: DatabaseConfig,
    #[serde(default)]
    pub global_settings: GlobalSettings,
    pub queries: Vec<QueryConfig>,
    #[serde(default)]
    pub execution: ExecutionConfig,
    pub monitoring: Option<MonitoringConfig>,
    pub environments: Option<HashMap<String, EnvironmentOverride>>,
}

impl YetiiConfig {
    /// Validates the entire configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate version format
        if self.version.is_none() {
            return Err(ConfigError::MissingRequiredField("version".to_string()));
        }

        // Validate database configuration
        self.databases.validate()?;

        // Validate global settings
        self.global_settings.validate()?;

        // Validate all queries
        for query in &self.queries {
            query.validate()?;
        }

        // Validate execution config
        self.execution.validate()?;

        Ok(())
    }

    /// Gets the effective configuration for a specific environment
    #[allow(unused)]
    pub fn for_environment(&self, env: &str) -> Self {
        let mut config = self.clone();

        if let Some(overrides) = &self.environments {
            if let Some(env_override) = overrides.get(env) {
                if let Some(global_settings) = &env_override.global_settings {
                    config.global_settings = global_settings.clone();
                }
                if let Some(databases) = &env_override.databases {
                    config.databases = databases.clone();
                }
                if let Some(monitoring) = &env_override.monitoring {
                    config.monitoring = Some(monitoring.clone());
                }
            }
        }

        config
    }

    pub fn from_file(path: Option<PathBuf>) -> Result<Self, ConfigError> {
        let candidate_paths = match path {
            Some(p) => vec![p],
            None => {
                let mut paths = Vec::new();

                // ./yetii.yaml
                paths.push(PathBuf::from("yetii.yaml"));

                // $HOME/yetii.yaml
                if let Ok(home) = env::var("HOME") {
                    let mut home_path = PathBuf::from(home);
                    home_path.push("yetii.yaml");
                    paths.push(home_path);
                }

                paths
            }
        };

        for candidate in candidate_paths {
            if candidate.exists() {
                let contents = fs::read_to_string(&candidate)
                    .map_err(|e| ConfigError::IoError(Error::new(std::io::ErrorKind::NotFound, e)))?;
                let config: YetiiConfig = serde_yaml::from_str(&contents)
                    .map_err(|e| ConfigError::IoError(Error::new(std::io::ErrorKind::InvalidData, e)))?;
                return Ok(config);
            }
        }

        Err(ConfigError::IoError(Error::new(
            std::io::ErrorKind::NotFound,
            "No yetii.yaml configuration file found",
        )))
    }
}

/// Enhanced database configuration with validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub db_type: DatabaseType,
    pub connection_string: Option<String>,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub schema: Option<String>,
    pub auth: AuthConfig,
    #[serde(default)]
    pub pool: ConnectionConfig,
}

impl DatabaseConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(ConfigError::MissingRequiredField("database.name".to_string()));
        }

        if self.host.is_empty() {
            return Err(ConfigError::MissingRequiredField("database.host".to_string()));
        }

        if self.database.is_empty() {
            return Err(ConfigError::MissingRequiredField("database.database".to_string()));
        }

        if self.db_type.validate().is_err() {
            return Err(ConfigError::InvalidDatabaseType("database.type not supported".to_string()));
        }

        self.pool.validate()?;

        Ok(())
    }
}

/// Enhanced global settings with defaults and validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalSettings {
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(default)]
    pub error_handling: ErrorHandling,
    #[serde(default)]
    pub logging: Logging,
    #[serde(default)]
    pub security: SecuritySettings,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            environment: default_environment(),
            error_handling: ErrorHandling::default(),
            logging: Logging::default(),
            security: SecuritySettings::default(),
        }
    }
}

impl GlobalSettings {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let valid_environments = ["development", "staging", "production"];
        if !valid_environments.contains(&self.environment.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.environment.clone()));
        }

        self.error_handling.validate()?;
        self.logging.validate()?;
        self.security.validate()?;

        Ok(())
    }
}

/// Enhanced connection config with validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionConfig {
    #[serde(default = "default_max_connections")]
    pub max_connections: Option<u32>,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: Option<u32>,
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: Option<u32>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: Option::from(default_max_connections()),
            timeout_seconds: Option::from(default_timeout_seconds()),
            retry_attempts: Option::from(default_retry_attempts()),
        }
    }
}

impl ConnectionConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_connections == Some(0) || self.max_connections > Some(1000) {
            return Err(ConfigError::InvalidTimeout(self.max_connections));
        }

        if self.timeout_seconds > Some(300) {
            return Err(ConfigError::InvalidTimeout(self.timeout_seconds));
        }

        Ok(())
    }
}

/// Enhanced error handling with validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorHandling {
    #[serde(default = "default_error_action")]
    pub on_query_error: String,
    #[serde(default = "default_error_action")]
    pub on_transform_error: String,
    #[serde(default = "default_error_action")]
    pub on_endpoint_error: String,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

impl Default for ErrorHandling {
    fn default() -> Self {
        Self {
            on_query_error: default_error_action(),
            on_transform_error: default_error_action(),
            on_endpoint_error: default_error_action(),
            max_retries: default_max_retries(),
        }
    }
}

impl ErrorHandling {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let valid_actions = ["stop", "log_and_continue", "retry"];

        if !valid_actions.contains(&self.on_query_error.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.on_query_error.clone()));
        }

        if self.max_retries > 10 {
            return Err(ConfigError::InvalidTimeout(Option::from(self.max_retries)));
        }

        Ok(())
    }
}

// Enhanced query config with validation
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

/// Enhanced schedule config with cron validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleConfig {
    pub cron: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl ScheduleConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Basic cron validation - you might want to use a proper cron parser
        let parts: Vec<&str> = self.cron.split_whitespace().collect();
        if parts.len() != 5 && parts.len() != 6 {
            return Err(ConfigError::InvalidSchedule(self.cron.clone()));
        }

        Ok(())
    }
}

// Default value functions
fn default_version() -> Option<String> { Some("1.0.0".to_string()) }
fn default_environment() -> String { "development".to_string() }
fn default_max_connections() -> Option<u32> { Some(10) }
fn default_timeout_seconds() -> Option<u32> { Some(30) }
fn default_retry_attempts() -> Option<u32> { Some(3) }
fn default_error_action() -> String { "stop".to_string() }
fn default_max_retries() -> u32 { 3 }
fn default_timezone() -> String { "UTC".to_string() }
fn default_true() -> bool { true }

// Include all the other structs from your original code...
// (DatabaseType, AuthConfig, SecuritySettings, Logging, etc.)
// I've only shown the enhanced versions of key structs for brevity

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseType {
    Postgres,
    Mysql,
    Mssql,
    Oracle,
}

impl DatabaseType{
    #[allow(unused)]
    pub fn validate(&self) -> Result<(), ConfigError> {
        match self {
            DatabaseType::Postgres | DatabaseType::Mysql | DatabaseType::Mssql | DatabaseType::Oracle => Ok(()),
            _ => Err(ConfigError::InvalidDatabaseType(format!("{:?}", self))),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecuritySettings {
    #[serde(default = "default_false")]
    pub encrypt_config: bool,
    #[serde(default = "default_true")]
    pub validate_ssl: bool,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: Option<u32>,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            encrypt_config: false,
            validate_ssl: false,
            timeout_seconds: default_timeout_seconds(),
        }
    }
}

impl SecuritySettings {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.timeout_seconds > Some(600) {
            return Err(ConfigError::InvalidTimeout(Option::from(self.timeout_seconds)));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Logging {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default = "default_log_output")]
    pub output: String,
    pub file_path: Option<String>,
    pub rotation: Option<LogRotation>,
}

impl Default for Logging {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            output: default_log_output(),
            file_path: None,
            rotation: None,
        }
    }
}

impl Logging {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.level.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.level.clone()));
        }

        let valid_formats = ["json", "plain", "structured"];
        if !valid_formats.contains(&self.format.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.format.clone()));
        }

        Ok(())
    }
}

fn default_false() -> bool { false }
fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> String { "json".to_string() }
fn default_log_output() -> String { "console".to_string() }

// Placeholder implementations for remaining structs
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LogRotation {
    pub max_size_mb: u32,
    pub max_files: u32,
}

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EndpointConfig {
    pub url: String,
    pub method: String,
    pub auth: Option<EndpointAuth>,
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub request: RequestConfig,
    pub response: Option<ResponseConfig>,
}

impl EndpointConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.url.is_empty() {
            return Err(ConfigError::MissingRequiredField("endpoint.url".to_string()));
        }

        let valid_methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "WRITE"];
        if !valid_methods.contains(&self.method.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.method.clone()));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum EndpointAuth {
    #[serde(rename = "bearer")]
    Bearer {
        token: String,
        #[serde(default)]
        header_name: Option<String>,
    },
    #[serde(rename = "api_key")]
    ApiKey {
        header_name: String,
        token: String,
    },
    #[serde(rename = "basic")]
    Basic {
        username: String,
        password: String,
    },
    #[serde(rename = "oauth2")]
    OAuth2 {
        client_id: String,
        client_secret: String,
        token_url: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestConfig {
    #[serde(default = "default_request_format")]
    pub format: String,
    pub batch_size: Option<u32>,
    pub timeout_seconds: Option<u32>,
    pub retry_attempts: Option<u32>,
    pub retry_delay_seconds: Option<u32>,
    pub retry_backoff: Option<String>,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            format: default_request_format(),
            batch_size: Some(100),
            timeout_seconds: Some(30),
            retry_attempts: Some(3),
            retry_delay_seconds: Some(1),
            retry_backoff: Some("exponential".to_string()),
        }
    }
}

fn default_request_format() -> String { "json".to_string() }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseConfig {
    pub success_codes: Vec<u16>,
    pub handle_duplicates: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionConfig {
    #[serde(default = "default_execution_mode")]
    pub mode: String,
    pub global_timeout_minutes: Option<u32>,
    pub state_management: Option<StateManagement>,
    pub scheduler: Option<SchedulerConfig>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            mode: default_execution_mode(),
            global_timeout_minutes: Some(60),
            state_management: None,
            scheduler: None,
        }
    }
}

impl ExecutionConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let valid_modes = ["parallel", "sequential"];
        if !valid_modes.contains(&self.mode.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.mode.clone()));
        }
        Ok(())
    }
}

fn default_execution_mode() -> String { "sequential".to_string() }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateManagement {
    pub enabled: bool,
    pub state_file: String,
    pub backup_states: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub max_concurrent_jobs: u32,
    pub job_timeout_minutes: u32,
    pub missed_job_policy: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitoringConfig {
    pub enabled: bool,
    pub metrics: Option<MetricsConfig>,
    pub health_check: Option<HealthCheckConfig>,
    pub notifications: Option<NotificationSettings>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub interval_seconds: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthCheckConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationSettings {
    pub on_failure: bool,
    pub on_success: bool,
    pub channels: Vec<NotificationChannel>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum NotificationChannel {
    #[serde(rename = "webhook")]
    Webhook { url: String },
    #[serde(rename = "email")]
    Email {
        smtp_host: String,
        recipients: Vec<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnvironmentOverride {
    pub global_settings: Option<GlobalSettings>,
    pub databases: Option<DatabaseConfig>,
    pub monitoring: Option<MonitoringConfig>,
}