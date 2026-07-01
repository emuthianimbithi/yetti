use crate::config::endpoint_config::EndpointAuth;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use url::form_urlencoded;

#[derive(Clone)]
pub struct OAuth2Client {
    client: Client,
    cache: Arc<Mutex<HashMap<TokenCacheKey, CachedToken>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TokenCacheKey {
    token_url: String,
    client_id: String,
    scopes: Vec<String>,
    audience: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum OAuth2Error {
    #[error("OAuth2 token request failed: {0}")]
    Request(reqwest::Error),
    #[error("OAuth2 token endpoint returned unexpected status {status}: {body}")]
    UnexpectedStatus {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("OAuth2 token response could not be decoded: {0}")]
    Decode(reqwest::Error),
    #[error("OAuth2 token response used unsupported token_type '{0}'")]
    UnsupportedTokenType(String),
}

impl OAuth2Client {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn token_for(&self, auth: &EndpointAuth) -> Result<String, OAuth2Error> {
        self.token_for_inner(auth, false).await
    }

    pub async fn refresh_token_for(&self, auth: &EndpointAuth) -> Result<String, OAuth2Error> {
        self.token_for_inner(auth, true).await
    }

    async fn token_for_inner(
        &self,
        auth: &EndpointAuth,
        force_refresh: bool,
    ) -> Result<String, OAuth2Error> {
        let EndpointAuth::OAuth2 {
            client_id,
            client_secret,
            token_url,
            scopes,
            audience,
        } = auth
        else {
            unreachable!("token_for only accepts OAuth2 auth")
        };
        let key = TokenCacheKey {
            token_url: token_url.clone(),
            client_id: client_id.clone(),
            scopes: scopes.clone().unwrap_or_default(),
            audience: audience.clone(),
        };

        if !force_refresh
            && let Some(token) = self.cache.lock().await.get(&key)
            && token.expires_at > Instant::now() + Duration::from_secs(60)
        {
            return Ok(token.access_token.clone());
        }

        let token = self
            .fetch_token(
                client_id,
                client_secret,
                token_url,
                scopes.as_deref(),
                audience.as_deref(),
            )
            .await?;

        self.cache.lock().await.insert(key, token.clone());
        Ok(token.access_token)
    }

    async fn fetch_token(
        &self,
        client_id: &str,
        client_secret: &str,
        token_url: &str,
        scopes: Option<&[String]>,
        audience: Option<&str>,
    ) -> Result<CachedToken, OAuth2Error> {
        let body = {
            let mut serializer = form_urlencoded::Serializer::new(String::new());
            serializer.append_pair("grant_type", "client_credentials");
            serializer.append_pair("client_id", client_id);
            serializer.append_pair("client_secret", client_secret);
            if let Some(scopes) = scopes
                && !scopes.is_empty()
            {
                serializer.append_pair("scope", &scopes.join(" "));
            }
            if let Some(audience) = audience {
                serializer.append_pair("audience", audience);
            }
            serializer.finish()
        };

        let response = self
            .client
            .post(token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(OAuth2Error::Request)?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(OAuth2Error::UnexpectedStatus {
                status,
                body: truncate(&body, 1024),
            });
        }

        let token = response
            .json::<TokenResponse>()
            .await
            .map_err(OAuth2Error::Decode)?;
        if let Some(token_type) = &token.token_type
            && !token_type.eq_ignore_ascii_case("bearer")
        {
            return Err(OAuth2Error::UnsupportedTokenType(token_type.clone()));
        }
        let expires_in = token.expires_in.unwrap_or(3600).max(1);

        Ok(CachedToken {
            access_token: token.access_token,
            expires_at: Instant::now() + Duration::from_secs(expires_in),
        })
    }
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
