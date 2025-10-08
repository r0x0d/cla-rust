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

    /// Validate certificate file permissions (Unix-only)
    #[cfg(unix)]
    fn validate_cert_permissions(path: &str, file_type: &str) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        
        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to read {} file metadata {}: {}", file_type, path, e))?;
        
        let permissions = metadata.permissions();
        let mode = permissions.mode();
        
        // Warn if file is readable by group or others (mode & 0o077)
        if mode & 0o077 != 0 {
            tracing::warn!(
                "Warning: {} file {} has overly permissive permissions ({:o}). \
                 Should be 0600 or more restrictive for security.",
                file_type, path, mode & 0o777
            );
        }
        
        Ok(())
    }
    
    /// Create an HTTP client with certificate-based authentication
    /// This is shared by all providers that need certificate authentication
    pub fn create_authenticated_client(config: &Config) -> Result<reqwest::Client, Box<dyn std::error::Error>> {
        // Validate certificate file permissions before reading
        validate_cert_permissions(&config.backend.auth.cert_file, "certificate")?;
        validate_cert_permissions(&config.backend.auth.key_file, "key")?;
        
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

    /// Helper: Generate a cryptographically secure UUID
    pub fn uuid_simple() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Helper: Get current Unix timestamp
    pub fn current_timestamp() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
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
            // Log detailed error internally
            tracing::error!("Backend returned error {}: {}", status, error_body);
            // Return sanitized error to client
            return Err(AppError::BackendError(format!(
                "Backend returned status {}",
                status
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

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Tests for common utility functions
    // ============================================================================

    #[test]
    fn test_uuid_simple_generates_valid_uuid() {
        let uuid = common::uuid_simple();
        
        // UUID v4 format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
        assert_eq!(uuid.len(), 36, "UUID should be 36 characters");
        assert_eq!(uuid.chars().filter(|&c| c == '-').count(), 4, "UUID should have 4 dashes");
        
        // Check that it's a valid UUID format
        assert!(uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'), "UUID should only contain hex digits and dashes");
    }

    #[test]
    fn test_uuid_simple_generates_unique_uuids() {
        let uuid1 = common::uuid_simple();
        let uuid2 = common::uuid_simple();
        let uuid3 = common::uuid_simple();
        
        assert_ne!(uuid1, uuid2, "UUIDs should be unique");
        assert_ne!(uuid2, uuid3, "UUIDs should be unique");
        assert_ne!(uuid1, uuid3, "UUIDs should be unique");
    }

    #[test]
    fn test_current_timestamp_returns_positive() {
        let timestamp = common::current_timestamp();
        assert!(timestamp > 0, "Timestamp should be positive");
    }

    #[test]
    fn test_current_timestamp_is_recent() {
        let timestamp = common::current_timestamp();
        
        // Should be after 2020-01-01 (1577836800)
        assert!(timestamp > 1577836800, "Timestamp should be after 2020");
        
        // Should be before 2050-01-01 (2524608000)
        assert!(timestamp < 2524608000, "Timestamp should be before 2050");
    }

    #[test]
    fn test_current_timestamp_is_monotonic() {
        let ts1 = common::current_timestamp();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let ts2 = common::current_timestamp();
        
        assert!(ts2 >= ts1, "Timestamps should be monotonically increasing");
    }

    #[test]
    fn test_current_timestamp_precision() {
        let ts1 = common::current_timestamp();
        let ts2 = common::current_timestamp();
        
        // Timestamps should be in seconds, so rapid calls might return the same value
        // or differ by at most a few seconds
        assert!((ts2 - ts1) < 2, "Rapid timestamp calls should be close");
    }
}

