use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::config::ConfigError;
pub use crate::config::request_config::RequestConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EndpointConfig {
    pub url: String,
    pub method: String,
    pub auth: Option<EndpointAuth>,
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub request: RequestConfig,
    pub response: Option<ResponseConfig>,
}

impl EndpointConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.url.is_empty() {
            return Err(ConfigError::MissingRequiredField("endpoint.url".to_string()));
        }

        let valid_methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "WRITE"];
        if !valid_methods.contains(&self.method.as_str()) {
            return Err(ConfigError::InvalidDatabaseType(self.method.clone()));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum EndpointAuth {
    #[serde(rename = "bearer")]
    Bearer {
        token: String,
        #[serde(default)]
        header_name: Option<String>,
    },
    #[serde(rename = "api_key")]
    ApiKey {
        header_name: String,
        token: String,
    },
    #[serde(rename = "basic")]
    Basic {
        username: String,
        password: String,
    },
    #[serde(rename = "oauth2")]
    OAuth2 {
        client_id: String,
        client_secret: String,
        token_url: String,
    },
}


#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseConfig {
    pub success_codes: Vec<u16>,
    pub handle_duplicates: String,
}