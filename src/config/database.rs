use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
use crate::config::connection_config::ConnectionConfig;

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