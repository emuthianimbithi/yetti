use crate::config::ConfigError;
use crate::config::connection_config::ConnectionConfig;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct DatabaseConfigs(Vec<DatabaseConfig>);

impl DatabaseConfigs {
    pub fn as_slice(&self) -> &[DatabaseConfig] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, name: &str) -> Option<&DatabaseConfig> {
        self.0.iter().find(|database| database.name == name)
    }

    pub fn resolve_for_query(&self, query_database: Option<&str>) -> Option<&DatabaseConfig> {
        match query_database {
            Some(name) => self.get(name),
            None if self.0.len() == 1 => self.0.first(),
            None => None,
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.0.is_empty() {
            return Err(ConfigError::MissingRequiredField("databases".to_string()));
        }

        let mut names = HashSet::new();
        for database in &self.0 {
            database.validate()?;
            if !names.insert(database.name.clone()) {
                return Err(ConfigError::InvalidValue {
                    field: "databases.name".to_string(),
                    value: format!("duplicate database name '{}'", database.name),
                });
            }
        }

        Ok(())
    }
}

impl From<DatabaseConfig> for DatabaseConfigs {
    fn from(database: DatabaseConfig) -> Self {
        Self(vec![database])
    }
}

impl From<Vec<DatabaseConfig>> for DatabaseConfigs {
    fn from(databases: Vec<DatabaseConfig>) -> Self {
        Self(databases)
    }
}

impl Serialize for DatabaseConfigs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DatabaseConfigs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOrMany {
            One(Box<DatabaseConfig>),
            Many(Vec<DatabaseConfig>),
        }

        match OneOrMany::deserialize(deserializer)? {
            OneOrMany::One(database) => Ok(Self(vec![*database])),
            OneOrMany::Many(databases) => Ok(Self(databases)),
        }
    }
}

/// Enhanced database configuration with validation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub db_type: DatabaseType,
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub connection_string: Option<String>,
    #[serde(default)]
    pub connection_options: HashMap<String, String>,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub database: String,
    pub schema: Option<String>,
    pub auth: AuthConfig,
    #[serde(default)]
    pub pool: ConnectionConfig,
}
impl DatabaseConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(ConfigError::MissingRequiredField(
                "database.name".to_string(),
            ));
        }

        if self.connection_string.is_none() {
            if self.host.trim().is_empty() {
                return Err(ConfigError::MissingRequiredField(
                    "database.host".to_string(),
                ));
            }
            if self.port == 0 {
                return Err(ConfigError::InvalidPort(self.port));
            }
            if self.database.trim().is_empty() {
                return Err(ConfigError::MissingRequiredField(
                    "database.database".to_string(),
                ));
            }
        }

        if self.db_type.validate().is_err() {
            return Err(ConfigError::InvalidDatabaseType(
                "database.type not supported".to_string(),
            ));
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
impl DatabaseType {
    pub fn default_odbc_driver(&self) -> &'static str {
        match self {
            DatabaseType::Postgres => "PostgreSQL Unicode",
            DatabaseType::Mysql => "MySQL ODBC 8.0 Unicode Driver",
            DatabaseType::Mssql => "ODBC Driver 18 for SQL Server",
            DatabaseType::Oracle => "Oracle in instantclient",
        }
    }

    #[allow(unused)]
    pub fn validate(&self) -> Result<(), ConfigError> {
        match self {
            DatabaseType::Postgres
            | DatabaseType::Mysql
            | DatabaseType::Mssql
            | DatabaseType::Oracle => Ok(()),
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub username: Option<String>,
    pub password: Option<String>,
}
