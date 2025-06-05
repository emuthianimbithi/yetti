// Default value functions
pub fn default_version() -> Option<String> { Some("1.0.0".to_string()) }
pub fn default_environment() -> String { "development".to_string() }
pub fn default_max_connections() -> Option<u32> { Some(10) }
pub fn default_timeout_seconds() -> Option<u32> { Some(30) }
pub fn default_retry_attempts() -> Option<u32> { Some(3) }
pub fn default_error_action() -> String { "stop".to_string() }
pub fn default_max_retries() -> u32 { 3 }
pub fn default_timezone() -> String { "UTC".to_string() }
pub fn default_true() -> bool { true }
pub fn default_false() -> bool { false }
pub fn default_log_level() -> String { "info".to_string() }
pub fn default_log_format() -> String { "json".to_string() }
pub fn default_log_output() -> String { "console".to_string() }
pub fn default_request_format() -> String { "json".to_string() }
pub fn default_execution_mode() -> String { "sequential".to_string() }
