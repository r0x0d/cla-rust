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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::openai::{ChatCompletionRequest, Message};

    /// Test Lightspeed Core request pass-through
    #[test]
    fn test_lightspeed_core_transform_request() {
        let provider = LightspeedCoreProvider::new();
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                    name: None,
                },
            ],
            temperature: Some(0.7),
            top_p: None,
            n: None,
            stream: None,
            stop: None,
            max_tokens: Some(100),
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };
        
        let backend_request = provider.transform_request(&request);
        
        // Should preserve the original request structure
        assert_eq!(
            backend_request.get("model").and_then(|v| v.as_str()),
            Some("test-model")
        );
        assert!(backend_request.get("messages").is_some());
    }

    /// Test Lightspeed Core request transformation preserves all fields
    #[test]
    fn test_lightspeed_core_transform_request_preserves_fields() {
        let provider = LightspeedCoreProvider::new();
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                    name: None,
                },
            ],
            temperature: Some(0.7),
            top_p: Some(0.9),
            n: Some(1),
            stream: Some(false),
            stop: Some(vec!["STOP".to_string()]),
            max_tokens: Some(100),
            presence_penalty: Some(0.5),
            frequency_penalty: Some(0.3),
            user: Some("test-user".to_string()),
            extra: std::collections::HashMap::new(),
        };
        
        let backend_request = provider.transform_request(&request);
        
        // All fields should be preserved
        assert_eq!(backend_request.get("model").and_then(|v| v.as_str()), Some("test-model"));
        // Float comparison with epsilon tolerance due to JSON serialization precision
        let temp = backend_request.get("temperature").and_then(|v| v.as_f64()).unwrap();
        assert!((temp - 0.7).abs() < 0.001, "Temperature should be approximately 0.7");
        let top_p = backend_request.get("top_p").and_then(|v| v.as_f64()).unwrap();
        assert!((top_p - 0.9).abs() < 0.001, "Top_p should be approximately 0.9");
        assert_eq!(backend_request.get("max_tokens").and_then(|v| v.as_u64()), Some(100));
        assert!(backend_request.get("messages").is_some());
    }

    /// Test Lightspeed Core response transformation with valid OpenAI format
    #[test]
    fn test_lightspeed_core_transform_response_valid() {
        let provider = LightspeedCoreProvider::new();
        let backend_response = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "This is a response"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        });
        
        let result = provider.transform_response(&backend_response, "test-model");
        
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.model, "test-model");
        assert_eq!(response.choices[0].message.content, "This is a response");
    }

    /// Test Lightspeed Core response transformation with invalid format
    #[test]
    fn test_lightspeed_core_transform_response_invalid() {
        let provider = LightspeedCoreProvider::new();
        
        // Wrong format
        let backend_response = json!({
            "data": {
                "text": "This is not OpenAI format"
            }
        });
        
        let result = provider.transform_response(&backend_response, "test-model");
        assert!(result.is_err(), "Should fail with non-OpenAI format");
    }

    /// Test Lightspeed Core streaming text extraction
    #[test]
    fn test_lightspeed_core_extract_streaming_text() {
        let provider = LightspeedCoreProvider::new();
        let backend_response = json!({
            "choices": [{
                "message": {
                    "content": "Streaming content"
                }
            }]
        });
        
        let result = provider.extract_streaming_text(&backend_response);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Streaming content");
    }

    /// Test Lightspeed Core streaming text extraction with missing fields
    #[test]
    fn test_lightspeed_core_extract_streaming_text_error() {
        let provider = LightspeedCoreProvider::new();
        let backend_response = json!({
            "choices": []
        });
        
        let result = provider.extract_streaming_text(&backend_response);
        assert!(result.is_err(), "Should fail with empty choices");
    }
}

