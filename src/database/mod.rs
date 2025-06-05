use crate::config::database::DatabaseConfig;
mod postgres;
/// Database trait to be used for all configured databases on Yetii
#[allow(unused)]
trait Database {
    fn connect(database_config: &DatabaseConfig);
    fn close(&self);
}