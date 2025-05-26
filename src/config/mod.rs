mod validation;

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Root configuration structure for the ERP integration system.
/// Defines database connections, queries, and endpoints.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct YetiiConfig {
    /// Configuration version for compatibility tracking
    pub version: String,
    /// Human-readable name for this configuration
    pub name: String,
    /// Description of what this configuration does
    pub description: String,
    /// Database connection settings
    pub database: DatabaseConfig,
    /// Global settings that apply to all queries
    pub global_settings: GlobalSettings,
    /// List of query configurations to execute
    pub queries: Vec<QueryConfig>,
    /// Execution behavior settings
    pub execution: ExecutionConfig,
    /// Optional monitoring and notification settings
    pub monitoring: Option<MonitoringConfig>,
}

/// Database connection configuration supporting multiple database types.
/// Includes authentication, connection pooling, and timeout settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    /// Type of database (postgres, mysql, sap_hana, etc.)
    #[serde(rename = "type")]
    pub db_type: DatabaseType,
    /// Database server hostname or IP address
    pub host: String,
    /// Database server port number
    pub port: u16,
    /// Database/catalog name to connect to
    pub database: String,
    /// Optional schema name (for databases that support schemas)
    pub schema: Option<String>,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Connection pooling and timeout settings
    pub connection: ConnectionConfig,
}

/// Authentication configuration for database connections.
/// Supports multiple authentication methods including basic auth, tokens, and certificates.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    /// Authentication type (basic, windows, token, certificate)
    #[serde(rename = "type")]
    pub auth_type: String,
    /// Username for basic authentication
    pub username: Option<String>,
    /// Password for basic authentication (should use environment variables)
    pub password: Option<String>,
    /// Token for token-based authentication
    pub token: Option<String>,
    /// Certificate path for certificate-based authentication
    pub certificate: Option<String>,
}

/// Connection pooling and timeout configuration for database connections.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConnectionConfig {
    /// Maximum number of concurrent database connections
    pub max_connections: Option<u32>,
    /// Connection timeout in seconds
    pub timeout_seconds: Option<u32>,
    /// Number of retry attempts for failed connections
    pub retry_attempts: Option<u32>,
}

/// Supported database types for the integration system.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseType {
    /// PostgreSQL database
    Postgres,
    /// MySQL database
    Mysql,
    /// SAP HANA in-memory database
    SapHana,
    /// Microsoft SQL Server
    Mssql,
    /// Oracle database
    Oracle,
}

/// Global configuration settings that apply to all queries in the system.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GlobalSettings {
    /// Error handling behavior configuration
    pub error_handling: ErrorHandling,
    /// Logging configuration
    pub logging: Logging,
}

/// Defines how the system should handle different types of errors.
/// Allows fine-grained control over error behavior at different stages.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorHandling {
    /// What to do when a database query fails (stop, continue, retry)
    pub on_query_error: String,
    /// What to do when data transformation fails (skip_record, stop, log_and_continue)
    pub on_transform_error: String,
    /// What to do when API endpoint calls fail (retry, stop, continue)
    pub on_endpoint_error: String,
}

/// Logging configuration for the integration system.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Logging {
    /// Log level (debug, info, warn, error)
    pub level: String,
    /// Log format (json, text)
    pub format: String,
    /// Log output destination (stdout, file, both)
    pub output: String,
    /// Optional file path when logging to file
    pub file_path: Option<String>,
}

/// Configuration for a single query including SQL, transformations, and endpoint.
/// Each query represents a complete data pipeline from database to API.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryConfig {
    /// Unique name for this query
    pub name: String,
    /// Human-readable description of what this query does
    pub description: String,
    /// Whether this query should be executed
    pub enabled: bool,
    /// SQL query configuration
    pub query: SqlQuery,
    /// Data transformation rules
    pub transform: TransformConfig,
    /// API endpoint configuration
    pub endpoint: EndpointConfig,
}

/// SQL query definition with optional parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SqlQuery {
    /// The SQL query string (can contain parameter placeholders)
    pub sql: String,
    /// Optional parameters for the SQL query
    pub parameters: Option<Vec<QueryParameter>>,
}

/// Parameter definition for parameterized SQL queries.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QueryParameter {
    /// Parameter name
    pub name: String,
    /// Parameter data type (string, integer, date, etc.)
    #[serde(rename = "type")]
    pub param_type: String,
    /// Parameter value (can include environment variable references)
    pub value: String,
}

/// Data transformation configuration that defines how to reshape query results.
/// Supports complex nested objects, arrays, and data type conversions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransformConfig {
    /// Field mapping definitions from source to target format
    pub mappings: HashMap<String, Mapping>,
    /// Optional field to group results by (creates nested arrays)
    pub group_by: Option<String>,
}

/// Defines how to map and transform a single field from the query result.
/// Supports simple field mapping, nested objects, and arrays.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Mapping {
    /// Simple field mapping with optional transformation
    Simple {
        /// Source field name from query result
        source: String,
        /// Target data type
        #[serde(rename = "type")]
        mapping_type: String,
        /// Whether this field is required
        #[serde(default)]
        required: Option<bool>,
        /// Optional transformation function (trim, uppercase, etc.)
        #[serde(default)]
        transform: Option<String>,
        /// Optional format specification (for dates, numbers, etc.)
        #[serde(default)]
        format: Option<String>,
    },
    /// Nested object mapping
    Object {
        /// Always "object" for object mappings
        #[serde(rename = "type")]
        mapping_type: String,
        /// Field mappings within this object
        fields: HashMap<String, Mapping>,
    },
    /// Array mapping for grouped data
    Array {
        /// Always "array" for array mappings
        #[serde(rename = "type")]
        mapping_type: String,
        /// Whether to group source data for this array
        #[serde(default)]
        group_source: Option<bool>,
        /// Field mappings for array elements
        fields: HashMap<String, Mapping>,
    },
}

/// API endpoint configuration including authentication, headers, and request settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EndpointConfig {
    /// Target API endpoint URL
    pub url: String,
    /// HTTP method (GET, POST, PUT, etc.)
    pub method: String,
    /// Authentication configuration for the endpoint
    pub auth: EndpointAuth,
    /// Optional HTTP headers to include
    pub headers: Option<HashMap<String, String>>,
    /// Request behavior settings
    pub request: RequestConfig,
}

/// Authentication methods supported for API endpoints.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum EndpointAuth {
    /// Bearer token authentication
    #[serde(rename = "bearer")]
    Bearer {
        /// Bearer token value
        token: String
    },

    /// API key authentication via header
    #[serde(rename = "api_key")]
    ApiKey {
        /// Header name for the API key
        header: String,
        /// API key value
        value: String
    },

    /// HTTP Basic authentication
    #[serde(rename = "basic")]
    Basic {
        /// Username for basic auth
        username: String,
        /// Password for basic auth
        password: String
    },

    /// OAuth2 client credentials flow
    #[serde(rename = "oauth2")]
    OAuth2 {
        /// OAuth2 client ID
        client_id: String,
        /// OAuth2 client secret
        client_secret: String,
        /// Token endpoint URL
        token_url: String,
    },
}

/// HTTP request configuration including batching, timeouts, and retry logic.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestConfig {
    /// Number of records to send per API call
    pub batch_size: u32,
    /// Request timeout in seconds
    pub timeout_seconds: u32,
    /// Number of retry attempts for failed requests
    pub retry_attempts: u32,
    /// Delay between retry attempts in seconds
    pub retry_delay_seconds: u32,
}

/// Template for generating request payloads.
/// Supports variable substitution for dynamic content.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PayloadTemplate {
    /// JSON template string with variable placeholders
    pub template: String,
}

/// Global execution settings that control how queries are run.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionConfig {
    /// Whether to run queries in parallel or sequentially
    pub parallel: bool,
    /// Execution order for sequential runs (query names)
    pub order: Option<Vec<String>>,
    /// Whether to stop execution on first error
    pub stop_on_error: bool,
    /// Maximum time for entire execution in minutes
    pub global_timeout_minutes: Option<u32>,
}

/// Monitoring and notification configuration for operational visibility.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitoringConfig {
    /// Metrics to collect during execution
    pub metrics: Vec<Metric>,
    /// Optional notification settings
    pub notifications: Option<NotificationConfig>,
}

/// Definition of a metric to collect during execution.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metric {
    /// Metric name
    pub name: String,
    /// Metric type (counter, histogram, gauge)
    #[serde(rename = "type")]
    pub metric_type: String,
    /// Labels to apply to this metric
    pub labels: Vec<String>,
}

/// Notification configuration for success and error scenarios.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationConfig {
    /// Notification settings for successful execution
    pub on_success: Option<NotificationChannel>,
    /// Notification settings for errors
    pub on_error: Option<ErrorNotificationChannel>,
}

/// Basic notification channel configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationChannel {
    /// Webhook URL to send notifications to
    pub webhook: String,
    /// Message template for the notification
    pub template: String,
}

/// Error notification channel with additional email support.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorNotificationChannel {
    /// Webhook URL for error notifications
    pub webhook: String,
    /// Optional email address for error notifications
    pub email: Option<String>,
    /// Message template for error notifications
    pub template: String,
}