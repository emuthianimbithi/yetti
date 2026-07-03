mod oauth2;
mod retry;

use crate::config::endpoint_config::{EndpointAuth, EndpointConfig};
use crate::config::request_config::RequestConfig;
use oauth2::OAuth2Client;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, Method, StatusCode};
use retry::{RetryPolicy, is_transient_status};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone)]
pub struct HttpSender {
    client: Client,
    oauth2: OAuth2Client,
    retry_policy: RetryPolicy,
}

#[derive(Debug)]
pub struct SendOutcome {
    pub status: StatusCode,
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("failed to build HTTP client: {0}")]
    BuildClient(reqwest::Error),
    #[error("invalid HTTP method: {0}")]
    InvalidMethod(String),
    #[error("invalid HTTP header name '{name}': {reason}")]
    InvalidHeaderName { name: String, reason: String },
    #[error("invalid value for HTTP header '{name}': {reason}")]
    InvalidHeaderValue { name: String, reason: String },
    #[error(transparent)]
    OAuth2(#[from] oauth2::OAuth2Error),
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("endpoint returned unexpected status {status}: {body}")]
    UnexpectedStatus { status: StatusCode, body: String },
}

impl HttpSender {
    pub fn new(request: &RequestConfig) -> Result<Self, HttpError> {
        let timeout = Duration::from_secs(request.timeout_seconds.unwrap_or(30) as u64);
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(HttpError::BuildClient)?;
        Ok(Self {
            oauth2: OAuth2Client::new(client.clone()),
            client,
            retry_policy: RetryPolicy::from_request(request),
        })
    }

    pub async fn send(
        &self,
        endpoint: &EndpointConfig,
        rows: &[Value],
    ) -> Result<SendOutcome, HttpError> {
        self.send_value(endpoint, &Value::Array(rows.to_vec()))
            .await
    }

    pub async fn send_value(
        &self,
        endpoint: &EndpointConfig,
        body: &Value,
    ) -> Result<SendOutcome, HttpError> {
        let mut retry_index = 0;

        loop {
            match self.send_once(endpoint, body).await {
                Ok(outcome) => return Ok(outcome),
                Err(error)
                    if retry_index < self.retry_policy.max_retries && error.is_retryable() =>
                {
                    retry_index += 1;
                    crate::monitoring::record_http_retry();
                    let delay = self.retry_policy.delay_for_retry(retry_index);
                    tracing::warn!(
                        retry = retry_index,
                        max_retries = self.retry_policy.max_retries,
                        delay_ms = delay.as_millis(),
                        error = %error,
                        "HTTP delivery failed; retrying"
                    );
                    tokio::time::sleep(delay).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn send_once(
        &self,
        endpoint: &EndpointConfig,
        body: &Value,
    ) -> Result<SendOutcome, HttpError> {
        let response = self.execute_request(endpoint, body, false).await?;
        if response.status() == StatusCode::UNAUTHORIZED
            && matches!(endpoint.auth, Some(EndpointAuth::OAuth2 { .. }))
        {
            tracing::warn!("endpoint returned 401; refreshing OAuth2 token and retrying once");
            return self
                .validate_response(endpoint, self.execute_request(endpoint, body, true).await?)
                .await;
        }

        self.validate_response(endpoint, response).await
    }

    async fn execute_request(
        &self,
        endpoint: &EndpointConfig,
        body: &Value,
        refresh_oauth2: bool,
    ) -> Result<reqwest::Response, HttpError> {
        let method = parse_method(&endpoint.method)?;
        let mut request = self.client.request(method, &endpoint.url);

        if let Some(headers) = &endpoint.headers {
            for (name, value) in headers {
                request = add_header(request, name, value)?;
            }
        }

        if let Some(auth) = &endpoint.auth {
            request = match auth {
                EndpointAuth::Bearer { token, header_name } => {
                    let name = header_name.as_deref().unwrap_or("Authorization");
                    add_header(request, name, &format!("Bearer {token}"))?
                }
                EndpointAuth::ApiKey { header_name, token } => {
                    add_header(request, header_name, token)?
                }
                EndpointAuth::Basic { username, password } => {
                    request.basic_auth(username, Some(password))
                }
                EndpointAuth::OAuth2 { .. } => {
                    let token = if refresh_oauth2 {
                        self.oauth2.refresh_token_for(auth).await?
                    } else {
                        self.oauth2.token_for(auth).await?
                    };
                    request.bearer_auth(token)
                }
            };
        }

        request.json(body).send().await.map_err(HttpError::Request)
    }

    async fn validate_response(
        &self,
        endpoint: &EndpointConfig,
        response: reqwest::Response,
    ) -> Result<SendOutcome, HttpError> {
        let status = response.status();
        let success_codes = endpoint
            .response
            .as_ref()
            .map(|response| response.success_codes.as_slice())
            .filter(|codes| !codes.is_empty())
            .unwrap_or(&[200, 201, 202, 204]);

        if !success_codes.contains(&status.as_u16()) {
            let body = response.text().await.unwrap_or_default();
            return Err(HttpError::UnexpectedStatus {
                status,
                body: truncate(&body, 1024),
            });
        }

        Ok(SendOutcome { status })
    }
}

impl HttpError {
    fn is_retryable(&self) -> bool {
        match self {
            HttpError::Request(error) => {
                error.is_connect() || error.is_timeout() || error.status().is_none()
            }
            HttpError::UnexpectedStatus { status, .. } => is_transient_status(*status),
            HttpError::BuildClient(_)
            | HttpError::InvalidMethod(_)
            | HttpError::InvalidHeaderName { .. }
            | HttpError::InvalidHeaderValue { .. }
            | HttpError::OAuth2(_) => false,
        }
    }
}

fn parse_method(method: &str) -> Result<Method, HttpError> {
    method
        .to_ascii_uppercase()
        .parse::<Method>()
        .map_err(|error| HttpError::InvalidMethod(error.to_string()))
}

fn add_header(
    request: reqwest::RequestBuilder,
    name: &str,
    value: &str,
) -> Result<reqwest::RequestBuilder, HttpError> {
    let header_name =
        HeaderName::from_bytes(name.as_bytes()).map_err(|error| HttpError::InvalidHeaderName {
            name: name.to_string(),
            reason: error.to_string(),
        })?;
    let header_value =
        HeaderValue::from_str(value).map_err(|error| HttpError::InvalidHeaderValue {
            name: name.to_string(),
            reason: error.to_string(),
        })?;
    Ok(request.header(header_name, header_value))
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::endpoint_config::{EndpointAuth, ResponseConfig};
    use std::collections::HashMap;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn sends_configured_method_headers_auth_and_json_body() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
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

            stream
                .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
            String::from_utf8(bytes).unwrap()
        });

        let mut headers = HashMap::new();
        headers.insert("X-Source".to_string(), "yetii".to_string());
        let endpoint = EndpointConfig {
            url: format!("http://{address}/sync"),
            method: "POST".to_string(),
            auth: Some(EndpointAuth::Bearer {
                token: "secret".to_string(),
                header_name: None,
            }),
            headers: Some(headers),
            request: RequestConfig::default(),
            response: Some(ResponseConfig {
                success_codes: vec![202],
                handle_duplicates: "skip".to_string(),
            }),
        };
        let sender = HttpSender::new(&endpoint.request).unwrap();

        let outcome = sender
            .send(&endpoint, &[serde_json::json!({"id": "42"})])
            .await
            .unwrap();
        let request = server.await.unwrap();
        let request_lower = request.to_ascii_lowercase();

        assert_eq!(StatusCode::ACCEPTED, outcome.status);
        assert!(request.starts_with("POST /sync HTTP/1.1"));
        assert!(request_lower.contains("x-source: yetii"));
        assert!(request_lower.contains("authorization: bearer secret"));
        assert!(request.contains(r#"[{"id":"42"}]"#));
    }

    #[tokio::test]
    async fn retries_transient_status_then_succeeds() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = attempts.clone();

        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                read_request(&mut stream).await;
                let attempt = server_attempts.fetch_add(1, Ordering::SeqCst);
                let response = if attempt < 2 {
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n".as_slice()
                } else {
                    b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n".as_slice()
                };
                stream.write_all(response).await.unwrap();
            }
        });

        let endpoint = EndpointConfig {
            url: format!("http://{address}/sync"),
            method: "POST".to_string(),
            auth: None,
            headers: None,
            request: RequestConfig {
                retry_attempts: Some(2),
                retry_delay_seconds: Some(0),
                retry_backoff: Some("fixed".to_string()),
                ..RequestConfig::default()
            },
            response: Some(ResponseConfig {
                success_codes: vec![202],
                handle_duplicates: "skip".to_string(),
            }),
        };
        let sender = HttpSender::new(&endpoint.request).unwrap();

        let outcome = sender
            .send(&endpoint, &[serde_json::json!({"id": 1})])
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(StatusCode::ACCEPTED, outcome.status);
        assert_eq!(3, attempts.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn does_not_retry_permanent_client_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let server_attempts = attempts.clone();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_request(&mut stream).await;
            server_attempts.fetch_add(1, Ordering::SeqCst);
            stream
                .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });

        let endpoint = EndpointConfig {
            url: format!("http://{address}/sync"),
            method: "POST".to_string(),
            auth: None,
            headers: None,
            request: RequestConfig {
                retry_attempts: Some(3),
                retry_delay_seconds: Some(0),
                ..RequestConfig::default()
            },
            response: Some(ResponseConfig {
                success_codes: vec![202],
                handle_duplicates: "skip".to_string(),
            }),
        };
        let sender = HttpSender::new(&endpoint.request).unwrap();

        assert!(
            sender
                .send(&endpoint, &[serde_json::json!({"id": 1})])
                .await
                .is_err()
        );
        server.await.unwrap();

        assert_eq!(1, attempts.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn oauth2_fetches_and_reuses_bearer_token() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let token_requests = Arc::new(AtomicUsize::new(0));
        let server_token_requests = token_requests.clone();

        let server = tokio::spawn(async move {
            let mut captured = Vec::new();
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = String::from_utf8(read_request(&mut stream).await).unwrap();
                if request.starts_with("POST /token ") {
                    server_token_requests.fetch_add(1, Ordering::SeqCst);
                    assert!(request.contains("grant_type=client_credentials"));
                    assert!(request.contains("client_id=client"));
                    assert!(request.contains("scope=rows.write"));
                    let body = "{\"access_token\":\"cached-token\",\"token_type\":\"Bearer\",\"expires_in\":3600}";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                } else {
                    captured.push(request);
                    stream
                        .write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n")
                        .await
                        .unwrap();
                }
            }
            captured
        });

        let endpoint = EndpointConfig {
            url: format!("http://{address}/sync"),
            method: "POST".to_string(),
            auth: Some(EndpointAuth::OAuth2 {
                client_id: "client".to_string(),
                client_secret: "secret".to_string(),
                token_url: format!("http://{address}/token"),
                scopes: Some(vec!["rows.write".to_string()]),
                audience: None,
            }),
            headers: None,
            request: RequestConfig {
                retry_attempts: Some(0),
                ..RequestConfig::default()
            },
            response: Some(ResponseConfig {
                success_codes: vec![202],
                handle_duplicates: "skip".to_string(),
            }),
        };
        let sender = HttpSender::new(&endpoint.request).unwrap();

        sender
            .send(&endpoint, &[serde_json::json!({"id": 1})])
            .await
            .unwrap();
        sender
            .send(&endpoint, &[serde_json::json!({"id": 2})])
            .await
            .unwrap();
        let captured = server.await.unwrap();

        assert_eq!(1, token_requests.load(Ordering::SeqCst));
        assert_eq!(2, captured.len());
        assert!(captured.iter().all(|request| {
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer cached-token")
        }));
    }

    #[tokio::test]
    async fn oauth2_refreshes_token_once_after_unauthorized_response() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let token_requests = Arc::new(AtomicUsize::new(0));
        let server_token_requests = token_requests.clone();

        let server = tokio::spawn(async move {
            let mut captured = Vec::new();
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = String::from_utf8(read_request(&mut stream).await).unwrap();
                if request.starts_with("POST /token ") {
                    let token_number = server_token_requests.fetch_add(1, Ordering::SeqCst);
                    let body = if token_number == 0 {
                        "{\"access_token\":\"stale-token\",\"token_type\":\"Bearer\",\"expires_in\":3600}"
                    } else {
                        "{\"access_token\":\"fresh-token\",\"token_type\":\"Bearer\",\"expires_in\":3600}"
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                } else {
                    let lower = request.to_ascii_lowercase();
                    let stale = lower.contains("authorization: bearer stale-token");
                    captured.push(request);
                    let response = if stale {
                        b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".as_slice()
                    } else {
                        b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n".as_slice()
                    };
                    stream.write_all(response).await.unwrap();
                }
            }
            captured
        });

        let endpoint = EndpointConfig {
            url: format!("http://{address}/sync"),
            method: "POST".to_string(),
            auth: Some(EndpointAuth::OAuth2 {
                client_id: "client".to_string(),
                client_secret: "secret".to_string(),
                token_url: format!("http://{address}/token"),
                scopes: None,
                audience: None,
            }),
            headers: None,
            request: RequestConfig {
                retry_attempts: Some(0),
                ..RequestConfig::default()
            },
            response: Some(ResponseConfig {
                success_codes: vec![202],
                handle_duplicates: "skip".to_string(),
            }),
        };
        let sender = HttpSender::new(&endpoint.request).unwrap();

        let outcome = sender
            .send(&endpoint, &[serde_json::json!({"id": 1})])
            .await
            .unwrap();
        let captured = server.await.unwrap();

        assert_eq!(StatusCode::ACCEPTED, outcome.status);
        assert_eq!(2, token_requests.load(Ordering::SeqCst));
        assert_eq!(2, captured.len());
        assert!(
            captured[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer stale-token")
        );
        assert!(
            captured[1]
                .to_ascii_lowercase()
                .contains("authorization: bearer fresh-token")
        );
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
