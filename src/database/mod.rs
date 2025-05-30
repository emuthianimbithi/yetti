mod postgres;

use crate::config::DatabaseConfig;

/// Database trait to be used for all configured databases on Yetii
#[allow(unused)]
trait Database {
    fn connect(database_config: &DatabaseConfig);
    fn close(&self);
}