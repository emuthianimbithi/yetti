use crate::config::ConfigError;
pub use crate::config::request_config::RequestConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
            return Err(ConfigError::MissingRequiredField(
                "endpoint.url".to_string(),
            ));
        }

        let method = self.method.to_ascii_uppercase();
        let valid_methods = ["GET", "POST", "PUT", "PATCH", "DELETE"];
        if !valid_methods.contains(&method.as_str()) {
            return Err(ConfigError::InvalidHttpMethod(self.method.clone()));
        }

        if self.request.format != "json" {
            return Err(ConfigError::InvalidValue {
                field: "endpoint.request.format".to_string(),
                value: self.request.format.clone(),
            });
        }

        if self.request.batch_size == Some(0) {
            return Err(ConfigError::InvalidValue {
                field: "endpoint.request.batch_size".to_string(),
                value: "0".to_string(),
            });
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
    ApiKey { header_name: String, token: String },
    #[serde(rename = "basic")]
    Basic { username: String, password: String },
    #[serde(rename = "oauth2")]
    OAuth2 {
        client_id: String,
        client_secret: String,
        token_url: String,
        #[serde(default)]
        scopes: Option<Vec<String>>,
        #[serde(default)]
        audience: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResponseConfig {
    #[serde(default = "default_success_codes")]
    pub success_codes: Vec<u16>,
    pub handle_duplicates: String,
}

fn default_success_codes() -> Vec<u16> {
    vec![200, 201, 202, 204]
}
