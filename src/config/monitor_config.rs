use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitoringConfig {
    pub enabled: bool,
    pub metrics: Option<MetricsConfig>,
    pub health_check: Option<HealthCheckConfig>,
    pub notifications: Option<NotificationSettings>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub interval_seconds: u32,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthCheckConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub port: u16,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationSettings {
    pub on_failure: bool,
    pub on_success: bool,
    pub channels: Vec<NotificationChannel>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum NotificationChannel {
    #[serde(rename = "webhook")]
    Webhook { url: String },
    #[serde(rename = "email")]
    Email {
        smtp_host: String,
        recipients: Vec<String>,
    },
}