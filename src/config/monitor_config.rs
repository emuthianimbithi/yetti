use crate::config::ConfigError;
use crate::config::endpoint_config::EndpointAuth;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub on_failure: bool,
    #[serde(default)]
    pub on_success: bool,
    #[serde(default)]
    pub channels: Vec<NotificationChannel>,
    #[serde(default)]
    pub services: Vec<NotificationServiceConfig>,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationServiceConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub service_type: NotificationServiceType,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub events: Vec<NotificationEventKind>,
    pub endpoint: NotificationEndpointConfig,
    pub auth: Option<EndpointAuth>,
    pub headers: Option<HashMap<String, String>>,
    pub payload: Option<NotificationPayloadConfig>,
    pub response: Option<NotificationResponseConfig>,
    pub retry: Option<NotificationRetryConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationServiceType {
    Http,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NotificationEventKind {
    RunSuccess,
    RunFailure,
    QuerySuccess,
    QueryFailure,
    DaemonStarted,
    DaemonStopping,
}

impl NotificationEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationEventKind::RunSuccess => "run_success",
            NotificationEventKind::RunFailure => "run_failure",
            NotificationEventKind::QuerySuccess => "query_success",
            NotificationEventKind::QueryFailure => "query_failure",
            NotificationEventKind::DaemonStarted => "daemon_started",
            NotificationEventKind::DaemonStopping => "daemon_stopping",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationEndpointConfig {
    pub url: String,
    #[serde(default = "default_http_method")]
    pub method: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationPayloadConfig {
    #[serde(default = "default_payload_format")]
    pub format: String,
    #[serde(default)]
    pub template: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotificationResponseConfig {
    #[serde(default = "default_success_codes")]
    pub success_codes: Vec<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NotificationRetryConfig {
    pub attempts: Option<u32>,
    pub delay_seconds: Option<u32>,
    pub backoff: Option<String>,
    pub timeout_seconds: Option<u32>,
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
            notifications.validate()?;
        }
        Ok(())
    }
}

impl NotificationSettings {
    fn validate(&self) -> Result<(), ConfigError> {
        if !self.enabled {
            return Ok(());
        }

        for channel in &self.channels {
            match channel {
                NotificationChannel::Webhook { url } => {
                    validate_http_url("monitoring.notifications.webhook.url", url)?;
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

        let mut service_names = HashSet::new();
        for service in &self.services {
            if !service.enabled {
                continue;
            }
            if service.name.trim().is_empty() {
                return Err(invalid(
                    "monitoring.notifications.services.name",
                    "service name is required",
                ));
            }
            if !service_names.insert(service.name.as_str()) {
                return Err(invalid(
                    "monitoring.notifications.services.name",
                    &service.name,
                ));
            }
            if service.events.is_empty() {
                return Err(invalid(
                    "monitoring.notifications.services.events",
                    "at least one event is required",
                ));
            }
            validate_http_method(
                "monitoring.notifications.services.endpoint.method",
                &service.endpoint.method,
            )?;
            validate_http_url(
                "monitoring.notifications.services.endpoint.url",
                &service.endpoint.url,
            )?;
            validate_notification_auth(service.auth.as_ref())?;
            if let Some(payload) = &service.payload
                && payload.format != "json"
            {
                return Err(invalid(
                    "monitoring.notifications.services.payload.format",
                    &payload.format,
                ));
            }
            if let Some(response) = &service.response
                && (response.success_codes.is_empty()
                    || response
                        .success_codes
                        .iter()
                        .any(|code| !(100..=599).contains(code)))
            {
                return Err(invalid(
                    "monitoring.notifications.services.response.success_codes",
                    "must contain HTTP status codes from 100 to 599",
                ));
            }
            if let Some(retry) = &service.retry
                && retry
                    .backoff
                    .as_deref()
                    .is_some_and(|backoff| !matches!(backoff, "fixed" | "exponential"))
            {
                return Err(invalid(
                    "monitoring.notifications.services.retry.backoff",
                    retry.backoff.as_deref().unwrap_or_default(),
                ));
            }
        }

        Ok(())
    }
}

fn validate_http_method(field: &str, method: &str) -> Result<(), ConfigError> {
    let method = method.to_ascii_uppercase();
    let valid_methods = ["GET", "POST", "PUT", "PATCH", "DELETE"];
    if valid_methods.contains(&method.as_str()) {
        Ok(())
    } else {
        Err(ConfigError::InvalidValue {
            field: field.to_string(),
            value: method,
        })
    }
}

fn validate_http_url(field: &str, value: &str) -> Result<(), ConfigError> {
    let parsed = url::Url::parse(value).map_err(|_| invalid(field, value))?;
    if matches!(parsed.scheme(), "http" | "https") {
        Ok(())
    } else {
        Err(invalid(field, value))
    }
}

fn validate_notification_auth(auth: Option<&EndpointAuth>) -> Result<(), ConfigError> {
    match auth {
        Some(EndpointAuth::Bearer { token, .. }) if token.trim().is_empty() => Err(invalid(
            "monitoring.notifications.services.auth.token",
            "token is required",
        )),
        Some(EndpointAuth::ApiKey { header_name, token })
            if header_name.trim().is_empty() || token.trim().is_empty() =>
        {
            Err(invalid(
                "monitoring.notifications.services.auth",
                "header_name and token are required",
            ))
        }
        Some(EndpointAuth::Basic { username, password })
            if username.trim().is_empty() || password.trim().is_empty() =>
        {
            Err(invalid(
                "monitoring.notifications.services.auth",
                "username and password are required",
            ))
        }
        Some(EndpointAuth::OAuth2 {
            client_id,
            client_secret,
            token_url,
            ..
        }) if client_id.trim().is_empty()
            || client_secret.trim().is_empty()
            || token_url.trim().is_empty() =>
        {
            Err(invalid(
                "monitoring.notifications.services.auth",
                "client_id, client_secret, and token_url are required",
            ))
        }
        Some(EndpointAuth::OAuth2 { token_url, .. }) => validate_http_url(
            "monitoring.notifications.services.auth.token_url",
            token_url,
        ),
        _ => Ok(()),
    }
}

fn invalid(field: &str, value: &str) -> ConfigError {
    ConfigError::InvalidValue {
        field: field.to_string(),
        value: value.to_string(),
    }
}

fn default_true() -> bool {
    true
}

fn default_http_method() -> String {
    "POST".to_string()
}

fn default_payload_format() -> String {
    "json".to_string()
}

fn default_success_codes() -> Vec<u16> {
    vec![200, 201, 202, 204]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_monitoring_listener_webhook_and_notification_services() {
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
                enabled: true,
                on_failure: true,
                on_success: false,
                channels: vec![NotificationChannel::Webhook {
                    url: "https://example.test/alert".to_string(),
                }],
                services: vec![NotificationServiceConfig {
                    name: "ops".to_string(),
                    service_type: NotificationServiceType::Http,
                    enabled: true,
                    events: vec![NotificationEventKind::QueryFailure],
                    endpoint: NotificationEndpointConfig {
                        url: "https://example.test/events".to_string(),
                        method: "POST".to_string(),
                    },
                    auth: None,
                    headers: None,
                    payload: Some(NotificationPayloadConfig {
                        format: "json".to_string(),
                        template: serde_json::json!({
                            "event": "{{event}}",
                            "query": "{{query_name}}",
                            "rows_read": "{{rows_read}}"
                        }),
                    }),
                    response: Some(NotificationResponseConfig {
                        success_codes: vec![202],
                    }),
                    retry: Some(NotificationRetryConfig {
                        attempts: Some(3),
                        delay_seconds: Some(1),
                        backoff: Some("exponential".to_string()),
                        timeout_seconds: Some(10),
                    }),
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

    #[test]
    fn rejects_invalid_notification_service_config() {
        let config = MonitoringConfig {
            enabled: true,
            metrics: None,
            health_check: None,
            notifications: Some(NotificationSettings {
                enabled: true,
                on_failure: false,
                on_success: false,
                channels: vec![],
                services: vec![NotificationServiceConfig {
                    name: "bad".to_string(),
                    service_type: NotificationServiceType::Http,
                    enabled: true,
                    events: vec![],
                    endpoint: NotificationEndpointConfig {
                        url: "ftp://example.test/events".to_string(),
                        method: "POST".to_string(),
                    },
                    auth: None,
                    headers: None,
                    payload: None,
                    response: None,
                    retry: None,
                }],
            }),
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn service_yaml_can_omit_legacy_success_failure_flags() {
        let config: MonitoringConfig = serde_yaml::from_str(
            r#"
enabled: true
notifications:
  enabled: true
  services:
    - name: ops
      type: http
      events: [query_failure]
      endpoint:
        url: https://example.test/events
"#,
        )
        .unwrap();

        config.validate().unwrap();
        let notifications = config.notifications.unwrap();
        assert!(!notifications.on_failure);
        assert!(notifications.channels.is_empty());
        assert_eq!(1, notifications.services.len());
    }
}
