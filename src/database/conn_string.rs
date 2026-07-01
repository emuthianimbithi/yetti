use crate::config::database::{DatabaseConfig, DatabaseType};
use odbc_api::escape_attribute_value;

pub fn build_connection_string(db: &DatabaseConfig) -> String {
    if let Some(connection_string) = &db.connection_string {
        return connection_string.clone();
    }

    let driver = db
        .driver
        .as_deref()
        .unwrap_or_else(|| db.db_type.default_odbc_driver());
    let mut attributes = vec![format!("Driver={}", brace_value(driver))];

    match db.db_type {
        DatabaseType::Postgres | DatabaseType::Mysql => {
            attributes.push(format!("Server={}", escape_value(&db.host)));
            attributes.push(format!("Port={}", db.port));
            attributes.push(format!("Database={}", escape_value(&db.database)));
            if matches!(db.db_type, DatabaseType::Postgres) && !has_option(db, "BoolsAsChar") {
                attributes.push("BoolsAsChar=0".to_string());
            }
        }
        DatabaseType::Mssql => {
            attributes.push(format!(
                "Server={}",
                escape_value(&format!("{},{}", db.host, db.port))
            ));
            attributes.push(format!("Database={}", escape_value(&db.database)));
            if !has_option(db, "TrustServerCertificate") {
                attributes.push("TrustServerCertificate=yes".to_string());
            }
        }
        DatabaseType::Oracle => {
            attributes.push(format!(
                "Dbq={}",
                escape_value(&format!("//{}:{}/{}", db.host, db.port, db.database))
            ));
        }
    }

    if let Some(username) = &db.auth.username {
        attributes.push(format!("Uid={}", escape_value(username)));
    }
    if let Some(password) = &db.auth.password {
        attributes.push(format!("Pwd={}", escape_value(password)));
    }

    let mut options = db.connection_options.iter().collect::<Vec<_>>();
    options.sort_by_key(|(name, _)| *name);
    for (name, value) in options {
        attributes.push(format!("{name}={}", escape_value(value)));
    }

    format!("{};", attributes.join(";"))
}

pub fn redacted_connection_description(db: &DatabaseConfig) -> String {
    if db.connection_string.is_some() {
        return "<configured connection string redacted>".to_string();
    }

    let driver = db
        .driver
        .as_deref()
        .unwrap_or_else(|| db.db_type.default_odbc_driver());
    let mut description = format!(
        "Driver={};Server={};Port={};Database={};Uid={};Pwd=***",
        driver,
        db.host,
        db.port,
        db.database,
        db.auth.username.as_deref().unwrap_or("<none>")
    );
    let mut options = db.connection_options.iter().collect::<Vec<_>>();
    options.sort_by_key(|(name, _)| *name);
    for (name, value) in options {
        let rendered = if is_sensitive_option(name) {
            "***"
        } else {
            value.as_str()
        };
        description.push_str(&format!(";{name}={rendered}"));
    }
    description
}

fn escape_value(value: &str) -> String {
    escape_attribute_value(value).into_owned()
}

fn has_option(db: &DatabaseConfig, name: &str) -> bool {
    db.connection_options
        .keys()
        .any(|option| option.eq_ignore_ascii_case(name))
}

fn is_sensitive_option(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("password")
        || name.contains("pwd")
        || name.contains("token")
        || name.contains("secret")
        || name.contains("key")
}

fn brace_value(value: &str) -> String {
    format!("{{{}}}", value.replace('}', "}}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::connection_config::ConnectionConfig;
    use crate::config::database::{AuthConfig, DatabaseType};

    fn database(db_type: DatabaseType) -> DatabaseConfig {
        DatabaseConfig {
            name: "main".to_string(),
            db_type,
            driver: None,
            connection_string: None,
            connection_options: Default::default(),
            host: "db.internal".to_string(),
            port: 5432,
            database: "erp".to_string(),
            schema: None,
            auth: AuthConfig {
                username: Some("sync".to_string()),
                password: Some("p;ass}word".to_string()),
            },
            pool: ConnectionConfig::default(),
        }
    }

    #[test]
    fn preserves_explicit_connection_string() {
        let mut db = database(DatabaseType::Postgres);
        db.connection_string = Some("DSN=erp".to_string());

        assert_eq!("DSN=erp", build_connection_string(&db));
        assert_eq!(
            "<configured connection string redacted>",
            redacted_connection_description(&db)
        );
    }

    #[test]
    fn builds_postgres_connection_string_and_escapes_values() {
        let value = build_connection_string(&database(DatabaseType::Postgres));

        assert_eq!(
            "Driver={PostgreSQL Unicode};Server=db.internal;Port=5432;Database=erp;BoolsAsChar=0;Uid=sync;Pwd={p;ass}}word};",
            value
        );
    }

    #[test]
    fn uses_sql_server_port_syntax_and_certificate_option() {
        let mut db = database(DatabaseType::Mssql);
        db.port = 1433;

        let value = build_connection_string(&db);

        assert!(value.contains("Server=db.internal,1433"));
        assert!(value.contains("TrustServerCertificate=yes"));
    }

    #[test]
    fn appends_connection_options_and_allows_sql_server_certificate_override() {
        let mut db = database(DatabaseType::Mssql);
        db.connection_options
            .insert("Encrypt".to_string(), "yes".to_string());
        db.connection_options
            .insert("TrustServerCertificate".to_string(), "no".to_string());

        let value = build_connection_string(&db);

        assert!(value.contains("Encrypt=yes"));
        assert!(value.contains("TrustServerCertificate=no"));
        assert!(!value.contains("TrustServerCertificate=yes"));
    }

    #[test]
    fn redacts_sensitive_connection_options() {
        let mut db = database(DatabaseType::Postgres);
        db.connection_options
            .insert("ApiToken".to_string(), "secret-token".to_string());

        let value = redacted_connection_description(&db);

        assert!(value.contains("ApiToken=***"));
        assert!(!value.contains("secret-token"));
    }

    #[test]
    fn redacted_description_never_contains_password() {
        let value = redacted_connection_description(&database(DatabaseType::Postgres));

        assert!(value.contains("Pwd=***"));
        assert!(!value.contains("p;ass"));
    }
}
