/// The creation of this file was inspired by the `cargo init` command.
use crate::config::{YetiiConfig, DatabaseConfig, DatabaseType, GlobalSettings, ErrorHandling, ExecutionConfig, Logging, QueryConfig, SqlQuery, TransformConfig, EndpointConfig, RequestConfig, EndpointAuth};
use std::error::Error;


/// Initializes the Yetii configuration file with default values.
/// # Arguments
/// * `config_name`: The name of the configuration file to be created.
/// * `path`: The path where the configuration file will be created.
/// # Returns
/// * `Ok(())` if the configuration file is created successfully.
/// * `Err(String)` if there is an error during the creation process.
/// # Example usage
/// ```rust
/// use yetii::initialize_yetii_config("yetii.config".to_string(), "./".to_string());
/// match initialize_yetii_config("yetii.config".to_string(), "./".to_string()) {
///     Ok(_) => println!("Yetii configuration initialized successfully."),
///     Err(e) => eprintln!("Error initializing Yetii configuration: {}", e),
///}
pub fn initialize_yetii_config(config_name: &String, path : &String) -> Result<String,Box<dyn Error>> {
    let config = YetiiConfig {
        version: "0.0.1".to_string(),
        name: config_name.clone(),
        description: "Yetii is a tool for managing and executing SQL queries across multiple databases.".to_string(),
        database: DatabaseConfig {
            db_type: DatabaseType::Postgres,
            host: "localhost".to_string(),
            port: 5432,
            database: "yetii_db".to_string(),
            schema: None,
            auth: crate::config::AuthConfig {
                auth_type: "password".to_string(),
                username: Some("yetii_user".to_string()),
                password: Some("yetii_password".to_string()),
                token: None,
                certificate: None,
            },
            connection: crate::config::ConnectionConfig {
                max_connections: Some(10),
                timeout_seconds: Some(90),
                retry_attempts: Some(3),
            },
        },
        global_settings: GlobalSettings {
            error_handling: ErrorHandling {
                on_query_error: "".to_string(),
                on_transform_error: "".to_string(),
                on_endpoint_error: "".to_string(),
            }, logging: Logging {
                level: "".to_string(),
                format: "".to_string(),
                output: "".to_string(),
                file_path: None,
            } },
        queries: vec![
            QueryConfig {
                name: "example_query".to_string(),
                description: "An example query to demonstrate Yetii configuration.".to_string(),
                enabled: false,
                query: SqlQuery { sql: "".to_string(), parameters: None },
                transform: TransformConfig {
                    mappings: Default::default(),
                    group_by: None,
                },
                endpoint: EndpointConfig {
                    url: "".to_string(),
                    method: "".to_string(),
                    auth: EndpointAuth::Bearer{
                        token: "your_token_here".to_string(),},
                    headers: None,
                    request: RequestConfig {
                        batch_size: 0,
                        timeout_seconds: 30,
                        retry_attempts: 3,
                        retry_delay_seconds: 1,
                    }
                },
            }
        ],
        execution: ExecutionConfig {
            parallel: false,
            order: None,
            stop_on_error: false,
            global_timeout_minutes: None,
        },
        monitoring: None,
    };

    // Convert the YetiiConfig to a YAML string
    let yaml_string = serde_yaml::to_string(&config).map_err(|e| format!("Failed to serialize YetiiConfig: {}", e))?;
    // Create the full path for the configuration file
    let full_path = format!("{}/{}", path, config_name);
    // save the YAML string to the specified path
    save_yaml_file(path, full_path.as_str(), &yaml_string)?;

    println!("Yetii configuration file created at: {}", full_path);
    Ok("Yetii configuration initialized successfully.".to_string())
}

use std::io::{self, Write};
use std::path::Path;

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
