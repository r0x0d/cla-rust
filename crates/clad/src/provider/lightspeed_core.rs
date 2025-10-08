use async_trait::async_trait;
use serde_json::Value;

use crate::openai::{ChatCompletionRequest, ChatCompletionResponse};
use crate::provider::Provider;
use crate::proxy::AppError;

/// Lightspeed Core provider
/// Simple pass-through provider that forwards requests as-is
pub struct LightspeedCoreProvider;

impl LightspeedCoreProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for LightspeedCoreProvider {
    /// Pass through the request without transformation
    /// The backend is expected to understand OpenAI format directly
    fn transform_request(&self, openai_req: &ChatCompletionRequest) -> Value {
        // Convert the request to JSON and pass it through
        serde_json::to_value(openai_req).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize request: {}", e);
            serde_json::json!({})
        })
    }
    
    /// Pass through the response without transformation
    /// The backend is expected to return OpenAI format directly
    fn transform_response(&self, backend_resp: &Value, _model: &str) -> Result<ChatCompletionResponse, AppError> {
        // Try to deserialize the backend response directly as OpenAI format
        serde_json::from_value(backend_resp.clone())
            .map_err(|e| {
                AppError::TransformError(format!(
                    "Failed to parse backend response as OpenAI format: {}. Response: {:?}",
                    e, backend_resp
                ))
            })
    }

    /// Extract text for streaming - expect OpenAI format
    fn extract_streaming_text(&self, backend_response: &Value) -> Result<String, AppError> {
        // For OpenAI format, look for choices[0].message.content
        backend_response
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::TransformError(format!(
                    "Could not extract text from backend response for streaming"
                ))
            })
            .map(|s| s.to_string())
    }
}

