use serde::{Deserialize, Serialize};
use crate::config::utils::default_request_format;
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestConfig {
    #[serde(default = "default_request_format")]
    pub format: String,
    pub batch_size: Option<u32>,
    pub timeout_seconds: Option<u32>,
    pub retry_attempts: Option<u32>,
    pub retry_delay_seconds: Option<u32>,
    pub retry_backoff: Option<String>,
}
impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            format: default_request_format(),
            batch_size: Some(100),
            timeout_seconds: Some(30),
            retry_attempts: Some(3),
            retry_delay_seconds: Some(1),
            retry_backoff: Some("exponential".to_string()),
        }
    }
}