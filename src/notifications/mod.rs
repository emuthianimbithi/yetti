mod template;

use crate::config::endpoint_config::{EndpointConfig, ResponseConfig};
use crate::config::monitor_config::{
    MonitoringConfig, NotificationChannel, NotificationEventKind, NotificationServiceConfig,
    NotificationSettings,
};
use crate::config::request_config::RequestConfig;
use crate::http::HttpSender;
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use template::render_template;

#[derive(Debug, Clone, Serialize)]
pub struct NotificationEvent {
    pub event: NotificationEventKind,
    pub success: bool,
    pub status: String,
    pub query: String,
    pub query_name: String,
    pub rows_read: usize,
    pub pages_read: usize,
    pub batches_sent: usize,
    pub failures: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub environment: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl NotificationEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn query_outcome(
        query: impl Into<String>,
        success: bool,
        error: Option<String>,
        rows_read: usize,
        pages_read: usize,
        batches_sent: usize,
        duration: Duration,
    ) -> Self {
        let query = query.into();
        let duration_ms = duration.as_millis().min(u64::MAX as u128) as u64;
        Self {
            event: if success {
                NotificationEventKind::QuerySuccess
            } else {
                NotificationEventKind::QueryFailure
            },
            success,
            status: if success { "success" } else { "failure" }.to_string(),
            query_name: query.clone(),
            query,
            rows_read,
            pages_read,
            batches_sent,
            failures: usize::from(!success),
            duration_ms,
            error,
            environment: None,
            occurred_at: Utc::now(),
            started_at: None,
            finished_at: Some(Utc::now()),
        }
    }

    pub fn run_outcome(
        success: bool,
        rows_read: usize,
        pages_read: usize,
        batches_sent: usize,
        failures: usize,
        duration: Duration,
    ) -> Self {
        let duration_ms = duration.as_millis().min(u64::MAX as u128) as u64;
        Self {
            event: if success {
                NotificationEventKind::RunSuccess
            } else {
                NotificationEventKind::RunFailure
            },
            success,
            status: if success { "success" } else { "failure" }.to_string(),
            query_name: String::new(),
            query: String::new(),
            rows_read,
            pages_read,
            batches_sent,
            failures,
            duration_ms,
            error: (!success).then(|| format!("{failures} query failure(s)")),
            environment: None,
            occurred_at: Utc::now(),
            started_at: None,
            finished_at: Some(Utc::now()),
        }
    }

    pub fn daemon_lifecycle(event: NotificationEventKind) -> Self {
        let success = matches!(
            event,
            NotificationEventKind::DaemonStarted | NotificationEventKind::DaemonStopping
        );
        Self {
            status: event.as_str().to_string(),
            event,
            success,
            query_name: String::new(),
            query: String::new(),
            rows_read: 0,
            pages_read: 0,
            batches_sent: 0,
            failures: 0,
            duration_ms: 0,
            error: None,
            environment: None,
            occurred_at: Utc::now(),
            started_at: None,
            finished_at: None,
        }
    }
}

pub async fn notify(config: Option<&MonitoringConfig>, event: &NotificationEvent) -> Result<()> {
    let Some(settings) = config
        .filter(|config| config.enabled)
        .and_then(|config| config.notifications.as_ref())
        .filter(|settings| settings.enabled)
    else {
        return Ok(());
    };

    let mut errors = Vec::new();
    deliver_legacy_channels(settings, event, &mut errors).await;
    deliver_services(settings, event, &mut errors).await;

    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(errors.join("; ")))
    }
}

async fn deliver_legacy_channels(
    settings: &NotificationSettings,
    event: &NotificationEvent,
    errors: &mut Vec<String>,
) {
    if !matches!(
        event.event,
        NotificationEventKind::QuerySuccess | NotificationEventKind::QueryFailure
    ) {
        return;
    }
    if (event.success && !settings.on_success) || (!event.success && !settings.on_failure) {
        return;
    }

    let client = reqwest::Client::new();
    for channel in &settings.channels {
        match channel {
            NotificationChannel::Webhook { url } => match client.post(url).json(event).send().await
            {
                Ok(response) if response.status().is_success() => {}
                Ok(response) => errors.push(format!(
                    "notification webhook '{url}' returned {}",
                    response.status()
                )),
                Err(error) => errors.push(format!("notification webhook '{url}' failed: {error}")),
            },
            NotificationChannel::Email { .. } => errors.push(
                "email notifications require SMTP authentication/TLS settings and are not implemented"
                    .to_string(),
            ),
        }
    }
}

async fn deliver_services(
    settings: &NotificationSettings,
    event: &NotificationEvent,
    errors: &mut Vec<String>,
) {
    for service in &settings.services {
        if !service.enabled || !service.events.contains(&event.event) {
            continue;
        }
        if let Err(error) = deliver_service(service, event).await {
            errors.push(format!(
                "notification service '{}' failed: {error:#}",
                service.name
            ));
        }
    }
}

async fn deliver_service(
    service: &NotificationServiceConfig,
    event: &NotificationEvent,
) -> Result<()> {
    let payload = render_payload(service, event)
        .with_context(|| format!("failed to render payload for '{}'", service.name))?;
    let endpoint = service_endpoint(service);
    let sender = HttpSender::new(&endpoint.request)?;
    sender.send_value(&endpoint, &payload).await?;
    Ok(())
}

fn render_payload(service: &NotificationServiceConfig, event: &NotificationEvent) -> Result<Value> {
    let Some(payload) = &service.payload else {
        return serde_json::to_value(event).context("failed to serialize notification event");
    };

    if payload.template.is_null() {
        return serde_json::to_value(event).context("failed to serialize notification event");
    }

    render_template(&payload.template, &event_fields(event))
}

fn event_fields(event: &NotificationEvent) -> HashMap<&'static str, Value> {
    let mut fields = HashMap::new();
    fields.insert("event", Value::from(event.event.as_str()));
    fields.insert("success", Value::from(event.success));
    fields.insert("status", Value::from(event.status.clone()));
    fields.insert("query", Value::from(event.query.clone()));
    fields.insert("query_name", Value::from(event.query_name.clone()));
    fields.insert("rows_read", Value::from(event.rows_read as u64));
    fields.insert("pages_read", Value::from(event.pages_read as u64));
    fields.insert("batches_sent", Value::from(event.batches_sent as u64));
    fields.insert("failures", Value::from(event.failures as u64));
    fields.insert("duration_ms", Value::from(event.duration_ms));
    fields.insert(
        "error",
        event
            .error
            .as_ref()
            .map_or(Value::Null, |error| Value::from(error.clone())),
    );
    fields.insert(
        "environment",
        event
            .environment
            .as_ref()
            .map_or(Value::Null, |environment| Value::from(environment.clone())),
    );
    fields.insert("occurred_at", Value::from(event.occurred_at.to_rfc3339()));
    fields.insert(
        "started_at",
        event.started_at.map_or(Value::Null, |started_at| {
            Value::from(started_at.to_rfc3339())
        }),
    );
    fields.insert(
        "finished_at",
        event.finished_at.map_or(Value::Null, |finished_at| {
            Value::from(finished_at.to_rfc3339())
        }),
    );
    fields
}

fn service_endpoint(service: &NotificationServiceConfig) -> EndpointConfig {
    EndpointConfig {
        url: service.endpoint.url.clone(),
        method: service.endpoint.method.clone(),
        auth: service.auth.clone(),
        headers: service.headers.clone(),
        request: RequestConfig {
            timeout_seconds: service
                .retry
                .as_ref()
                .and_then(|retry| retry.timeout_seconds),
            retry_attempts: service.retry.as_ref().and_then(|retry| retry.attempts),
            retry_delay_seconds: service.retry.as_ref().and_then(|retry| retry.delay_seconds),
            retry_backoff: service
                .retry
                .as_ref()
                .and_then(|retry| retry.backoff.clone()),
            ..RequestConfig::default()
        },
        response: Some(ResponseConfig {
            success_codes: service
                .response
                .as_ref()
                .map(|response| response.success_codes.clone())
                .filter(|codes| !codes.is_empty())
                .unwrap_or_else(|| vec![200, 201, 202, 204]),
            handle_duplicates: "skip".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::endpoint_config::EndpointAuth;
    use crate::config::monitor_config::{
        NotificationEndpointConfig, NotificationPayloadConfig, NotificationResponseConfig,
        NotificationRetryConfig, NotificationServiceType,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn service_notification_posts_templated_payload_with_auth() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            stream
                .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8(request).unwrap()
        });

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
                    name: "ops".to_string(),
                    service_type: NotificationServiceType::Http,
                    enabled: true,
                    events: vec![NotificationEventKind::QueryFailure],
                    endpoint: NotificationEndpointConfig {
                        url: format!("http://{address}/events"),
                        method: "POST".to_string(),
                    },
                    auth: Some(EndpointAuth::Bearer {
                        token: "token".to_string(),
                        header_name: None,
                    }),
                    headers: Some(HashMap::from([(
                        "X-Source".to_string(),
                        "yetii".to_string(),
                    )])),
                    payload: Some(NotificationPayloadConfig {
                        format: "json".to_string(),
                        template: serde_json::json!({
                            "event": "{{event}}",
                            "query": "{{query_name}}",
                            "rows_read": "{{rows_read}}",
                            "message": "query {{query_name}} failed: {{error}}"
                        }),
                    }),
                    response: Some(NotificationResponseConfig {
                        success_codes: vec![202],
                    }),
                    retry: Some(NotificationRetryConfig {
                        attempts: Some(0),
                        delay_seconds: Some(0),
                        backoff: Some("fixed".to_string()),
                        timeout_seconds: Some(5),
                    }),
                }],
            }),
        };
        let event = NotificationEvent::query_outcome(
            "orders",
            false,
            Some("database unavailable".to_string()),
            42,
            2,
            3,
            Duration::from_millis(15),
        );

        notify(Some(&config), &event).await.unwrap();
        let request = server.await.unwrap();
        let lower = request.to_ascii_lowercase();

        assert!(request.starts_with("POST /events HTTP/1.1"));
        assert!(lower.contains("authorization: bearer token"));
        assert!(lower.contains("x-source: yetii"));
        assert!(request.contains(r#""event":"query_failure""#));
        assert!(request.contains(r#""query":"orders""#));
        assert!(request.contains(r#""rows_read":42"#));
        assert!(request.contains(r#""message":"query orders failed: database unavailable""#));
    }

    #[tokio::test]
    async fn legacy_webhook_shape_still_posts_success_and_query() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8(request).unwrap()
        });
        let config = MonitoringConfig {
            enabled: true,
            metrics: None,
            health_check: None,
            notifications: Some(NotificationSettings {
                enabled: true,
                on_failure: true,
                on_success: false,
                channels: vec![NotificationChannel::Webhook {
                    url: format!("http://{address}/alert"),
                }],
                services: vec![],
            }),
        };
        let event = NotificationEvent::query_outcome(
            "orders",
            false,
            Some("failed".to_string()),
            0,
            0,
            0,
            Duration::from_millis(10),
        );

        notify(Some(&config), &event).await.unwrap();

        let request = server.await.unwrap();
        assert!(request.starts_with("POST /alert HTTP/1.1"));
        assert!(request.contains("\"success\":false"));
        assert!(request.contains("\"query\":\"orders\""));
        assert!(request.contains("\"event\":\"query_failure\""));
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 2048];

        loop {
            let read = stream.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);

            if let Some(header_end) = find_header_end(&bytes) {
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= header_end + 4 + content_length {
                    break;
                }
            }
        }

        bytes
    }

    fn find_header_end(bytes: &[u8]) -> Option<usize> {
        bytes.windows(4).position(|window| window == b"\r\n\r\n")
    }
}
