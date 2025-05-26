use crate::config::DatabaseConfig;

/// Database trait to be used for all configured databases on Yetii
trait Database {
    ///
    fn connect(database_config: DatabaseConfig);

    fn close(&self);

}