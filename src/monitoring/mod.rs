use crate::config::monitor_config::{MonitoringConfig, NotificationChannel};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

static METRICS: LazyLock<Mutex<MetricsState>> =
    LazyLock::new(|| Mutex::new(MetricsState::default()));

#[derive(Debug, Default)]
struct MetricsState {
    ready: bool,
    shutting_down: bool,
    started_at: Option<DateTime<Utc>>,
    active_queries: u64,
    total_runs: u64,
    total_failures: u64,
    total_rows: u64,
    total_pages: u64,
    total_batches: u64,
    http_retries: u64,
    overlap_skips: u64,
    queries: BTreeMap<String, QueryMetrics>,
}

#[derive(Debug, Default, Serialize)]
struct QueryMetrics {
    active: bool,
    runs: u64,
    failures: u64,
    rows: u64,
    pages: u64,
    batches: u64,
    last_duration_ms: u64,
    last_success_at: Option<DateTime<Utc>>,
    last_failure_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationEvent {
    pub success: bool,
    pub query: String,
    pub rows_read: usize,
    pub pages_read: usize,
    pub batches_sent: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Default)]
struct Routes {
    health_path: Option<String>,
    metrics_path: Option<String>,
}

pub struct MonitoringServer {
    shutdown: watch::Sender<bool>,
    handles: Vec<tokio::task::JoinHandle<()>>,
}

pub fn initialize() {
    let mut metrics = lock_metrics();
    if metrics.started_at.is_none() {
        metrics.started_at = Some(Utc::now());
    }
}

pub fn set_ready(ready: bool) {
    let mut metrics = lock_metrics();
    metrics.ready = ready;
}

pub fn set_shutting_down() {
    let mut metrics = lock_metrics();
    metrics.shutting_down = true;
    metrics.ready = false;
}

pub fn query_started(query: &str) {
    let mut metrics = lock_metrics();
    metrics.total_runs += 1;
    metrics.active_queries += 1;
    let query = metrics.queries.entry(query.to_string()).or_default();
    query.active = true;
    query.runs += 1;
}

pub fn query_succeeded(query: &str, rows: usize, pages: usize, batches: usize, duration: Duration) {
    let mut metrics = lock_metrics();
    metrics.active_queries = metrics.active_queries.saturating_sub(1);
    metrics.total_rows += rows as u64;
    metrics.total_pages += pages as u64;
    metrics.total_batches += batches as u64;
    let query = metrics.queries.entry(query.to_string()).or_default();
    query.active = false;
    query.rows += rows as u64;
    query.pages += pages as u64;
    query.batches += batches as u64;
    query.last_duration_ms = duration.as_millis().min(u64::MAX as u128) as u64;
    query.last_success_at = Some(Utc::now());
    query.last_error = None;
}

pub fn query_failed(
    query: &str,
    error: &str,
    rows: usize,
    pages: usize,
    batches: usize,
    duration: Duration,
) {
    let mut metrics = lock_metrics();
    metrics.active_queries = metrics.active_queries.saturating_sub(1);
    metrics.total_failures += 1;
    metrics.total_rows += rows as u64;
    metrics.total_pages += pages as u64;
    metrics.total_batches += batches as u64;
    let query = metrics.queries.entry(query.to_string()).or_default();
    query.active = false;
    query.failures += 1;
    query.rows += rows as u64;
    query.pages += pages as u64;
    query.batches += batches as u64;
    query.last_duration_ms = duration.as_millis().min(u64::MAX as u128) as u64;
    query.last_failure_at = Some(Utc::now());
    query.last_error = Some(error.to_string());
}

pub fn record_http_retry() {
    lock_metrics().http_retries += 1;
}

pub fn record_overlap_skip(query: &str) {
    let mut metrics = lock_metrics();
    metrics.overlap_skips += 1;
    metrics
        .queries
        .entry(query.to_string())
        .or_default()
        .last_error = Some("overlapping scheduled execution skipped".to_string());
}

pub async fn start(config: Option<&MonitoringConfig>) -> Result<Option<MonitoringServer>> {
    let Some(config) = config.filter(|config| config.enabled) else {
        return Ok(None);
    };
    initialize();

    let mut listeners = HashMap::<String, Routes>::new();
    if let Some(health) = config.health_check.as_ref().filter(|health| health.enabled) {
        listeners
            .entry(format!("127.0.0.1:{}", health.port))
            .or_default()
            .health_path = Some(health.endpoint.clone());
    }
    if let Some(metrics) = config.metrics.as_ref().filter(|metrics| metrics.enabled) {
        let url = url::Url::parse(&metrics.endpoint)
            .with_context(|| format!("invalid metrics endpoint '{}'", metrics.endpoint))?;
        let host = match url.host_str().unwrap_or("127.0.0.1") {
            "localhost" => "127.0.0.1",
            host => host,
        };
        let address = format!(
            "{host}:{}",
            url.port_or_known_default()
                .ok_or_else(|| anyhow!("metrics endpoint has no port"))?
        );
        let path = if url.path().is_empty() {
            "/metrics".to_string()
        } else {
            url.path().to_string()
        };
        listeners.entry(address).or_default().metrics_path = Some(path);
    }
    if listeners.is_empty() {
        return Ok(None);
    }

    let (shutdown, _) = watch::channel(false);
    let mut handles = Vec::new();
    for (address, routes) in listeners {
        let listener = TcpListener::bind(&address)
            .await
            .with_context(|| format!("failed to bind monitoring server to {address}"))?;
        let receiver = shutdown.subscribe();
        tracing::info!(address, "monitoring server listening");
        handles.push(tokio::spawn(serve(listener, routes, receiver)));
    }
    Ok(Some(MonitoringServer { shutdown, handles }))
}

impl MonitoringServer {
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        for handle in self.handles {
            let _ = handle.await;
        }
    }
}

async fn serve(listener: TcpListener, routes: Routes, mut shutdown: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let health_path = routes.health_path.clone();
                        let metrics_path = routes.metrics_path.clone();
                        tokio::spawn(async move {
                            if let Err(error) = respond(stream, health_path.as_deref(), metrics_path.as_deref()).await {
                                tracing::debug!(error = %error, "monitoring request failed");
                            }
                        });
                    }
                    Err(error) => tracing::warn!(error = %error, "monitoring listener accept failed"),
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}

async fn respond(
    mut stream: tokio::net::TcpStream,
    health_path: Option<&str>,
    metrics_path: Option<&str>,
) -> Result<()> {
    let mut buffer = [0_u8; 4096];
    let read = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let (status, content_type, body) = if health_path == Some(path) {
        let (healthy, body) = health_body();
        (
            if healthy {
                "200 OK"
            } else {
                "503 Service Unavailable"
            },
            "application/json",
            body,
        )
    } else if metrics_path == Some(path) {
        ("200 OK", "text/plain; version=0.0.4", metrics_body())
    } else {
        (
            "404 Not Found",
            "application/json",
            "{\"error\":\"not found\"}\n".to_string(),
        )
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

fn health_body() -> (bool, String) {
    let metrics = lock_metrics();
    let healthy = metrics.ready && !metrics.shutting_down;
    let body = json!({
        "status": if healthy { "healthy" } else { "unavailable" },
        "ready": metrics.ready,
        "shutting_down": metrics.shutting_down,
        "started_at": metrics.started_at,
        "active_queries": metrics.active_queries,
        "queries": metrics.queries,
    });
    (healthy, format!("{body}\n"))
}

fn metrics_body() -> String {
    let metrics = lock_metrics();
    let mut output = format!(
        "# TYPE yetii_ready gauge\nyetii_ready {}\n\
# TYPE yetii_active_queries gauge\nyetii_active_queries {}\n\
# TYPE yetii_runs_total counter\nyetii_runs_total {}\n\
# TYPE yetii_failures_total counter\nyetii_failures_total {}\n\
# TYPE yetii_rows_total counter\nyetii_rows_total {}\n\
# TYPE yetii_pages_total counter\nyetii_pages_total {}\n\
# TYPE yetii_batches_total counter\nyetii_batches_total {}\n\
# TYPE yetii_http_retries_total counter\nyetii_http_retries_total {}\n\
# TYPE yetii_overlap_skips_total counter\nyetii_overlap_skips_total {}\n",
        u8::from(metrics.ready && !metrics.shutting_down),
        metrics.active_queries,
        metrics.total_runs,
        metrics.total_failures,
        metrics.total_rows,
        metrics.total_pages,
        metrics.total_batches,
        metrics.http_retries,
        metrics.overlap_skips,
    );
    for (name, query) in &metrics.queries {
        let name = escape_label(name);
        output.push_str(&format!(
            "yetii_query_runs_total{{query=\"{name}\"}} {}\n\
yetii_query_failures_total{{query=\"{name}\"}} {}\n\
yetii_query_last_duration_ms{{query=\"{name}\"}} {}\n",
            query.runs, query.failures, query.last_duration_ms
        ));
    }
    output
}

pub async fn notify(config: Option<&MonitoringConfig>, event: &NotificationEvent) -> Result<()> {
    let Some(settings) = config
        .filter(|config| config.enabled)
        .and_then(|config| config.notifications.as_ref())
    else {
        return Ok(());
    };
    if (event.success && !settings.on_success) || (!event.success && !settings.on_failure) {
        return Ok(());
    }

    let client = reqwest::Client::new();
    let mut errors = Vec::new();
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
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(errors.join("; ")))
    }
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn lock_metrics() -> std::sync::MutexGuard<'static, MetricsState> {
    METRICS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::monitor_config::NotificationSettings;

    #[test]
    fn metrics_include_query_and_retry_counters() {
        initialize();
        set_ready(true);
        query_started("orders");
        record_http_retry();
        query_succeeded("orders", 25, 2, 3, Duration::from_millis(40));

        let body = metrics_body();

        assert!(body.contains("yetii_ready 1"));
        assert!(body.contains("yetii_http_retries_total"));
        assert!(body.contains("yetii_query_runs_total{query=\"orders\"}"));
    }

    #[tokio::test]
    async fn health_and_metrics_routes_respond() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let routes = Routes {
            health_path: Some("/health".to_string()),
            metrics_path: Some("/metrics".to_string()),
        };
        let (_shutdown, receiver) = watch::channel(false);
        let server = tokio::spawn(serve(listener, routes, receiver));
        set_ready(true);

        let health = reqwest::get(format!("http://{address}/health"))
            .await
            .unwrap();
        assert!(health.status().is_success());
        assert_eq!(
            "healthy",
            health.json::<serde_json::Value>().await.unwrap()["status"]
        );

        let metrics = reqwest::get(format!("http://{address}/metrics"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(metrics.contains("yetii_ready"));

        server.abort();
    }

    #[tokio::test]
    async fn failure_notification_posts_to_webhook() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = stream.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n")
                    && String::from_utf8_lossy(&request).contains("\"query\":\"orders\"")
                {
                    break;
                }
            }
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
                on_failure: true,
                on_success: false,
                channels: vec![NotificationChannel::Webhook {
                    url: format!("http://{address}/alert"),
                }],
            }),
        };
        let event = NotificationEvent {
            success: false,
            query: "orders".to_string(),
            rows_read: 0,
            pages_read: 0,
            batches_sent: 0,
            duration_ms: 10,
            error: Some("failed".to_string()),
            occurred_at: Utc::now(),
        };

        notify(Some(&config), &event).await.unwrap();

        let request = server.await.unwrap();
        assert!(request.starts_with("POST /alert HTTP/1.1"));
        assert!(request.contains("\"success\":false"));
    }
}
