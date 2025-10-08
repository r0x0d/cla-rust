use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::config::Config;
use crate::openai::{ChatCompletionRequest, ChatCompletionResponse};
use crate::proxy::AppError;

/// Common functionality shared across all providers
pub mod common {
    use std::fs;
    use crate::config::Config;

    /// Create an HTTP client with certificate-based authentication
    /// This is shared by all providers that need certificate authentication
    pub fn create_authenticated_client(config: &Config) -> Result<reqwest::Client, Box<dyn std::error::Error>> {
        // Read certificate and key files
        let cert_pem = fs::read(&config.backend.auth.cert_file)
            .map_err(|e| format!("Failed to read cert file {}: {}", config.backend.auth.cert_file, e))?;
        let key_pem = fs::read(&config.backend.auth.key_file)
            .map_err(|e| format!("Failed to read key file {}: {}", config.backend.auth.key_file, e))?;
        
        // Create identity from certificate and key (PEM format)
        let identity = reqwest::Identity::from_pkcs8_pem(&cert_pem, &key_pem)?;
        
        // Build client with identity and optional proxy settings
        let mut client_builder = reqwest::Client::builder()
            .identity(identity)
            .timeout(std::time::Duration::from_secs(config.backend.timeout));
        
        // Add proxy configuration if specified
        if let Some(proxies) = &config.backend.proxies {
            if let Some(http_proxy) = proxies.get("http") {
                client_builder = client_builder.proxy(reqwest::Proxy::http(http_proxy)?);
            }
            if let Some(https_proxy) = proxies.get("https") {
                client_builder = client_builder.proxy(reqwest::Proxy::https(https_proxy)?);
            }
        }
        
        Ok(client_builder.build()?)
    }

    /// Helper: Generate a simple UUID-like identifier
    pub fn uuid_simple() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{:x}", now)
    }

    /// Helper: Get current Unix timestamp
    pub fn current_timestamp() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }
}

/// Trait that all providers must implement
#[async_trait]
pub trait Provider: Send + Sync {
    /// Transform OpenAI request to backend format
    fn transform_request(&self, request: &ChatCompletionRequest) -> Value;
    
    /// Transform backend response to OpenAI format
    fn transform_response(&self, backend_response: &Value, model: &str) -> Result<ChatCompletionResponse, AppError>;
    
    /// Handle non-streaming request
    async fn handle_request(
        &self,
        client: &reqwest::Client,
        config: Arc<Config>,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, AppError> {
        let backend_request = self.transform_request(&request);
        
        // Forward request to external backend
        let backend_req = client
            .post(&config.backend.endpoint)
            .json(&backend_request);

        let response = backend_req
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to send request to backend: {}", e);
                AppError::BackendError(e.to_string())
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            tracing::error!("Backend returned error {}: {}", status, error_body);
            return Err(AppError::BackendError(format!(
                "Backend returned status {}: {}",
                status, error_body
            )));
        }

        // Parse backend response
        let backend_response: Value = response
            .json()
            .await
            .map_err(|e| {
                tracing::error!("Failed to parse backend response: {}", e);
                AppError::BackendError(e.to_string())
            })?;

        // Transform backend response to OpenAI format
        self.transform_response(&backend_response, &request.model)
    }
    
    /// Extract text for streaming (can be overridden)
    fn extract_streaming_text(&self, backend_response: &Value) -> Result<String, AppError> {
        backend_response
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::TransformError(format!(
                    "Could not extract text from backend response for streaming"
                ))
            })
            .map(|s| s.to_string())
    }
}

pub mod rhel_lightspeed;
pub mod lightspeed_core;

