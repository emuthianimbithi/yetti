use crate::config::ConfigError;
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

impl MonitoringConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.enabled {
            return Ok(());
        }
        if let Some(metrics) = &self.metrics
            && metrics.enabled
        {
            let url = url::Url::parse(&metrics.endpoint)
                .map_err(|_| invalid("monitoring.metrics.endpoint", &metrics.endpoint))?;
            if !matches!(url.scheme(), "http")
                || url.host_str().is_none()
                || url.port_or_known_default().is_none()
            {
                return Err(invalid("monitoring.metrics.endpoint", &metrics.endpoint));
            }
            if metrics.interval_seconds == 0 {
                return Err(invalid(
                    "monitoring.metrics.interval_seconds",
                    "must be greater than zero",
                ));
            }
        }
        if let Some(health) = &self.health_check
            && health.enabled
            && (!health.endpoint.starts_with('/') || health.port == 0)
        {
            return Err(invalid(
                "monitoring.health_check",
                "endpoint must start with '/' and port must be non-zero",
            ));
        }
        if let Some(notifications) = &self.notifications {
            for channel in &notifications.channels {
                match channel {
                    NotificationChannel::Webhook { url } => {
                        let parsed = url::Url::parse(url)
                            .map_err(|_| invalid("monitoring.notifications.webhook.url", url))?;
                        if !matches!(parsed.scheme(), "http" | "https") {
                            return Err(invalid("monitoring.notifications.webhook.url", url));
                        }
                    }
                    NotificationChannel::Email {
                        smtp_host,
                        recipients,
                    } if smtp_host.trim().is_empty() || recipients.is_empty() => {
                        return Err(invalid(
                            "monitoring.notifications.email",
                            "smtp_host and at least one recipient are required",
                        ));
                    }
                    NotificationChannel::Email { .. } => {}
                }
            }
        }
        Ok(())
    }
}

fn invalid(field: &str, value: &str) -> ConfigError {
    ConfigError::InvalidValue {
        field: field.to_string(),
        value: value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_monitoring_listener_and_webhook_urls() {
        let config = MonitoringConfig {
            enabled: true,
            metrics: Some(MetricsConfig {
                enabled: true,
                endpoint: "http://127.0.0.1:9090/metrics".to_string(),
                interval_seconds: 30,
            }),
            health_check: Some(HealthCheckConfig {
                enabled: true,
                endpoint: "/health".to_string(),
                port: 8080,
            }),
            notifications: Some(NotificationSettings {
                on_failure: true,
                on_success: false,
                channels: vec![NotificationChannel::Webhook {
                    url: "https://example.test/alert".to_string(),
                }],
            }),
        };

        config.validate().unwrap();
    }

    #[test]
    fn rejects_invalid_monitoring_paths_and_intervals() {
        let config = MonitoringConfig {
            enabled: true,
            metrics: Some(MetricsConfig {
                enabled: true,
                endpoint: "http://127.0.0.1:9090/metrics".to_string(),
                interval_seconds: 0,
            }),
            health_check: Some(HealthCheckConfig {
                enabled: true,
                endpoint: "health".to_string(),
                port: 0,
            }),
            notifications: None,
        };

        assert!(config.validate().is_err());
    }
}
