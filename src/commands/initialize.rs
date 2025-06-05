/// The creation of this file was inspired by the `cargo init` command.
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Write};
use std::path::Path;
use crate::config::connection_config::ConnectionConfig;
use crate::config::database::{AuthConfig, DatabaseConfig, DatabaseType};
use crate::config::endpoint_config::{EndpointAuth, EndpointConfig, ResponseConfig};
use crate::config::error_handling::ErrorHandling;
use crate::config::execution_config::{ExecutionConfig, SchedulerConfig, StateManagement};
use crate::config::global_settings::{GlobalSettings, Logging};
use crate::config::logging::LogRotation;
use crate::config::monitor_config::{HealthCheckConfig, MetricsConfig, MonitoringConfig, NotificationChannel, NotificationSettings};
use crate::config::query_config::QueryConfig;
use crate::config::request_config::RequestConfig;
use crate::config::schedule_config::ScheduleConfig;
use crate::config::security_settings::SecuritySettings;
use crate::config::sql_query::{QueryParameter, QueryValidation, SqlQuery};
use crate::config::transform_config::{DataConversion, DataFilter, TransformConfig};
use crate::config::yetii::YetiiConfig;
/// Initializes the Yetii configuration file with default values and helpful comments.
/// # Arguments
/// * `config_name`: The name of the configuration file to be created.
/// * `path`: The path where the configuration file will be created.
/// # Returns
/// * `Ok(String)` with success message if the configuration file is created successfully.
/// * `Err(Box<dyn Error>)` if there is an error during the creation process.
/// # Example usage
/// ```rust
/// use yetii::initialize_yetii_config;
/// match initialize_yetii_config("yetii.yaml", &"./".to_string()) {
///     Ok(msg) => println!("{}", msg),
///     Err(e) => eprintln!("Error initializing Yetii configuration: {}", e),
/// }
/// ```
pub fn initialize_yetii_config(config_name: &str, path: &String) -> Result<String, Box<dyn Error>> {
    let config = create_default_config(config_name)?;

    // Generate YAML with comments
    let yaml_content = generate_commented_yaml(&config)?;

    // Create the full path for the configuration file
    let full_path = Path::new(path).join(config_name);
    let full_path_str = full_path.to_string_lossy();

    // Save the YAML string to the specified path
    save_yaml_file_simple(&full_path_str, &yaml_content)?;

    println!("Yetii configuration file created at: {}", full_path_str);
    Ok("Yetii configuration initialized successfully.".to_string())
}

fn save_yaml_file_simple(full_path: &str, yaml_string: &str) -> Result<(), String> {
    let file_path = Path::new(full_path);

    // Create parent directory if it doesn't exist
    if let Some(parent_dir) = file_path.parent() {
        if !parent_dir.exists() {
            std::fs::create_dir_all(parent_dir)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }
    }

    // Check if file exists and prompt for overwrite
    if file_path.exists() {
        print!("File '{}' already exists. Overwrite? (y/N): ", full_path);
        io::stdout().flush().map_err(|e| format!("Failed to flush stdout: {}", e))?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read user input: {}", e))?;

        let input = input.trim().to_lowercase();
        if input != "y" && input != "yes" {
            return Err("Aborted by user.".into());
        }
    }

    // Write YAML to file
    std::fs::write(full_path, yaml_string)
        .map_err(|e| format!("Failed to write configuration file: {}", e))?;

    Ok(())
}
fn create_default_config(config_name: &str) -> Result<YetiiConfig, Box<dyn Error>> {
    let mut query_parameters = HashMap::new();
    query_parameters.insert("last_run_time".to_string(), QueryParameter {
        param_type: "timestamp".to_string(),
        default: Some("1970-01-01T00:00:00Z".to_string()),
        source: Some("state_file".to_string()),
    });

    let mut field_mappings = HashMap::new();
    field_mappings.insert("id".to_string(), "customer_id".to_string());
    field_mappings.insert("name".to_string(), "customer_name".to_string());
    field_mappings.insert("email".to_string(), "email_address".to_string());

    let mut data_conversions = HashMap::new();
    data_conversions.insert("created_at".to_string(), DataConversion {
        from: "timestamp".to_string(),
        to: "iso8601_string".to_string(),
        format: None,
    });

    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("X-Source".to_string(), "yetii-erp-sync".to_string());

    let config = YetiiConfig {
        version: Some("1.0.0".to_string()),
        name: Some(config_name.to_string()),
        description: Some("Yetii configuration for ERP data integration and transformation".to_string()),
        databases: DatabaseConfig {
            name: "main_erp".to_string(),
            db_type: DatabaseType::Postgres,
            connection_string: None,
            host: "localhost".to_string(),
            port: 5432,
            database: "erp_db".to_string(),
            schema: Some("public".to_string()),
            auth: AuthConfig {
                username: Some("erp_user".to_string()),
                password: Some("${ERP_PASSWORD}".to_string()),
            },
            pool: ConnectionConfig {
                max_connections: Some(10),
                timeout_seconds: Some(30),
                retry_attempts: Some(3),
            },
        },
        global_settings: GlobalSettings {
            environment: "development".to_string(),
            error_handling: ErrorHandling {
                on_query_error: "log_and_continue".to_string(),
                on_transform_error: "stop".to_string(),
                on_endpoint_error: "retry".to_string(),
                max_retries: 3,
            },
            logging: Logging {
                level: "info".to_string(),
                format: "json".to_string(),
                output: "file".to_string(),
                file_path: Some("./logs/yetii.log".to_string()),
                rotation: Some(LogRotation {
                    max_size_mb: 100,
                    max_files: 10,
                }),
            },
            security: SecuritySettings {
                encrypt_config: false,
                validate_ssl: true,
                timeout_seconds: Some(300),
            },
        },
        queries: vec![
            QueryConfig {
                name: "customer_data_sync".to_string(),
                description: "Sync customer data from ERP to external system".to_string(),
                enabled: true,
                database: Some("main_erp".to_string()),
                schedule: Some(ScheduleConfig {
                    cron: "0 */6 * * *".to_string(),
                    timezone: "UTC".to_string(),
                    enabled: true,
                }),
                query: SqlQuery {
                    sql: "SELECT \n  customer_id,\n  customer_name,\n  email,\n  created_at\nFROM customers \nWHERE updated_at > $last_run_time\nORDER BY updated_at".to_string(),
                    parameters: Some(query_parameters),
                    validation: Some(QueryValidation {
                        strict_mapping: Some(true),
                        warn_unmapped_columns: Some(true),
                        validate_filter_fields: Some(true),
                    }),
                },
                transform: TransformConfig {
                    enabled: true,
                    mappings: Some(field_mappings),
                    group_by: None,
                    filters: Some(vec![
                        DataFilter {
                            field: "email".to_string(),
                            condition: "not_null".to_string(),
                            value: None,
                        }
                    ]),
                    conversions: Some(data_conversions),
                },
                endpoint: EndpointConfig {
                    url: "https://api.example.com/customers".to_string(),
                    method: "POST".to_string(),
                    auth: Some(EndpointAuth::Bearer {
                        token: "${API_TOKEN}".to_string(),
                        header_name: Some("Authorization".to_string()),
                    }),
                    headers: Some(headers),
                    request: RequestConfig {
                        format: "json".to_string(),
                        batch_size: Some(100),
                        timeout_seconds: Some(30),
                        retry_attempts: Some(3),
                        retry_delay_seconds: Some(5),
                        retry_backoff: Some("exponential".to_string()),
                    },
                    response: Some(ResponseConfig {
                        success_codes: vec![200, 201, 202],
                        handle_duplicates: "skip".to_string(),
                    }),
                },
            }
        ],
        execution: ExecutionConfig {
            mode: "parallel".to_string(),
            global_timeout_minutes: Some(60),
            state_management: Some(StateManagement {
                enabled: true,
                state_file: "./state/yetii_state.json".to_string(),
                backup_states: 5,
            }),
            scheduler: Some(SchedulerConfig {
                enabled: true,
                max_concurrent_jobs: 5,
                job_timeout_minutes: 30,
                missed_job_policy: "skip".to_string(),
            }),
        },
        monitoring: Some(MonitoringConfig {
            enabled: true,
            metrics: Some(MetricsConfig {
                enabled: true,
                endpoint: "http://localhost:9090/metrics".to_string(),
                interval_seconds: 30,
            }),
            health_check: Some(HealthCheckConfig {
                enabled: true,
                endpoint: "/health".to_string(),
                port: 8080,
            }),
            notifications: Some(NotificationSettings {
                on_failure: true,
                on_success: false,
                channels: vec![
                    NotificationChannel::Webhook {
                        url: "https://hooks.slack.com/services/...".to_string(),
                    }
                ],
            }),
        }),
        environments: None,
    };

    Ok(config)
}
fn generate_commented_yaml(config: &YetiiConfig) -> Result<String, Box<dyn Error>> {
    let yaml = serde_yaml::to_string(config)?;

    let commented_yaml = format!(r#"# Yetii Configuration File
# Version: {}
# Description: {}
#
# This configuration file defines database connections, queries, transformations,
# and API endpoints for the Yetii ERP integration system.

{}

# Configuration Notes:
#
# Database Configuration:
# - Supports PostgreSQL, MySQL, MSSQL, Oracle, and ODBC connections
# - Use environment variables for sensitive data like passwords (${{VAR_NAME}})
# - Connection pooling helps manage database resources efficiently
#
# Query Configuration:
# - Each query can have its own schedule using cron expressions
# - Transformations allow field mapping, filtering, and data type conversions
# - Validation settings help catch configuration errors early
#
# Scheduling:
# - Cron format: "second minute hour day_of_month month day_of_week"
# - Common examples:
#   * "0 */6 * * *"   - Every 6 hours
#   * "0 0 * * *"     - Daily at midnight
#   * "0 0 * * 1"     - Weekly on Mondays
#   * "0 9-17 * * 1-5" - Weekdays 9 AM to 5 PM
#
# Error Handling Options:
# - stop: Stop execution on error
# - log_and_continue: Log error but continue processing
# - retry: Attempt to retry the operation
#
# Environment Variables:
# - Use ${{VARIABLE_NAME}} syntax for environment variable substitution
# - Recommended for passwords, API keys, and environment-specific values
#
# For more information, visit: https://docs.yetii.io
"#, config.version.as_deref().unwrap_or("0.0.1"),
                                 config.description.as_ref().unwrap_or(&"Yetii ERP Integration".to_string()), yaml
    );

    Ok(commented_yaml)
}
fn save_yaml_file(path: &str, full_path: &str, yaml_string: &str) -> Result<(), String> {
    // Create directory if it doesn't exist
    if !Path::new(path).exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    // Check if file exists
    if Path::new(full_path).exists() {
        // Prompt for overwrite
        print!("File '{}' already exists. Overwrite? (y/N): ", full_path);
        io::stdout().flush().map_err(|e| format!("Failed to flush stdout: {}", e))?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read user input: {}", e))?;

        let input = input.trim().to_lowercase();
        if input != "y" && input != "yes" {
            return Err("Aborted by user.".into());
        }
    }

    // Write YAML to file
    std::fs::write(full_path, yaml_string)
        .map_err(|e| format!("Failed to write configuration file: {}", e))?;

    Ok(())
}