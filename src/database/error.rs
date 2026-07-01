#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("failed to initialize the ODBC environment: {0}")]
    Environment(odbc_api::Error),
    #[error("failed to connect through ODBC: {0}")]
    Connect(odbc_api::Error),
    #[error("failed to execute the ODBC query: {0}")]
    Execute(odbc_api::Error),
    #[error("failed to read the ODBC result set: {0}")]
    Fetch(odbc_api::Error),
    #[error("invalid query parameter configuration: {0}")]
    Parameter(String),
    #[error("ODBC worker task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}
