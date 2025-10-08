use async_trait::async_trait;
use serde_json::{json, Value};

use crate::openai::{ChatCompletionRequest, ChatCompletionResponse, Choice, Message, Usage};
use crate::provider::{common, Provider};
use crate::proxy::AppError;

/// RHEL Lightspeed provider
/// Transforms requests to the command-line-assistant backend format
/// Based on: https://github.com/rhel-lightspeed/command-line-assistant
pub struct RhelLightspeedProvider;

impl RhelLightspeedProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Provider for RhelLightspeedProvider {
    /// Transform OpenAI request to command-line-assistant backend format
    /// Based on: https://github.com/rhel-lightspeed/command-line-assistant/blob/main/command_line_assistant/dbus/interfaces/chat.py
    fn transform_request(&self, openai_req: &ChatCompletionRequest) -> Value {
        // The command-line-assistant backend expects:
        // {
        //   "question": "user's current message",
        //   "context": [...previous messages for context...]
        // }
        
        // Extract the last user message (most recent)
        let user_message = openai_req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("");
        
        // Build context from previous messages (excluding the last user message)
        // Not being used right now. Probably we will replace this with MCP calls.
        let _context: Vec<Value> = openai_req
            .messages
            .iter()
            .filter_map(|m| {
                // Include all messages for context
                Some(json!({
                    "role": m.role,
                    "content": m.content
                }))
            })
            .collect();
        
        json!({
            "question": user_message,
            //"context": context
        })
    }
    
    /// Transform command-line-assistant backend response to OpenAI format
    /// Based on: https://github.com/rhel-lightspeed/command-line-assistant/blob/main/command_line_assistant/dbus/interfaces/chat.py
    fn transform_response(&self, backend_resp: &Value, model: &str) -> Result<ChatCompletionResponse, AppError> {
        // The command-line-assistant backend returns:
        // {
        //   "data": {
        //     "text": "assistant's response text"
        //   },
        //   ... possibly other fields ...
        // }
        
        // Extract the "text" field from the backend response
        let generated_text = backend_resp
            .get("data")
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::TransformError(format!(
                    "Could not extract 'data.text' field from backend response. Response: {:?}",
                    backend_resp
                ))
            })?;

        // Extract token usage if available
        let (prompt_tokens, completion_tokens, total_tokens) = if let Some(usage) = backend_resp.get("usage") {
            (
                usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                usage.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            )
        } else {
            // Estimate token counts if not provided
            let estimated_prompt = 0;
            let estimated_completion = (generated_text.len() / 4) as u32;
            (estimated_prompt, estimated_completion, estimated_prompt + estimated_completion)
        };

        // Build OpenAI-compatible response
        Ok(ChatCompletionResponse {
            id: format!("chatcmpl-{}", common::uuid_simple()),
            object: "chat.completion".to_string(),
            created: common::current_timestamp(),
            model: model.to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content: generated_text.to_string(),
                    name: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens,
            },
        })
    }

    /// Extract text for streaming from RHEL Lightspeed backend
    fn extract_streaming_text(&self, backend_response: &Value) -> Result<String, AppError> {
        backend_response
            .get("data")
            .and_then(|v| v.get("text"))
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

    /// Test RHEL Lightspeed request transformation
    #[test]
    fn test_rhel_lightspeed_transform_request() {
        let provider = RhelLightspeedProvider::new();
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: "What is Rust?".to_string(),
                    name: None,
                },
            ],
            temperature: None,
            top_p: None,
            n: None,
            stream: None,
            stop: None,
            max_tokens: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };
        
        let backend_request = provider.transform_request(&request);
        
        // Should have a "question" field
        assert!(backend_request.get("question").is_some());
        assert_eq!(
            backend_request.get("question").and_then(|v| v.as_str()),
            Some("What is Rust?")
        );
    }

    /// Test RHEL Lightspeed request transformation with multiple messages
    #[test]
    fn test_rhel_lightspeed_transform_request_multiple_messages() {
        let provider = RhelLightspeedProvider::new();
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "You are a helpful assistant.".to_string(),
                    name: None,
                },
                Message {
                    role: "user".to_string(),
                    content: "What is Linux?".to_string(),
                    name: None,
                },
                Message {
                    role: "assistant".to_string(),
                    content: "Linux is an operating system.".to_string(),
                    name: None,
                },
                Message {
                    role: "user".to_string(),
                    content: "Tell me more about kernels.".to_string(),
                    name: None,
                },
            ],
            temperature: None,
            top_p: None,
            n: None,
            stream: None,
            stop: None,
            max_tokens: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };
        
        let backend_request = provider.transform_request(&request);
        
        // Should extract the last user message
        assert_eq!(
            backend_request.get("question").and_then(|v| v.as_str()),
            Some("Tell me more about kernels.")
        );
    }

    /// Test RHEL Lightspeed request transformation with only system message
    #[test]
    fn test_rhel_lightspeed_transform_request_no_user_message() {
        let provider = RhelLightspeedProvider::new();
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "You are a helpful assistant.".to_string(),
                    name: None,
                },
            ],
            temperature: None,
            top_p: None,
            n: None,
            stream: None,
            stop: None,
            max_tokens: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: std::collections::HashMap::new(),
        };
        
        let backend_request = provider.transform_request(&request);
        
        // Should handle empty user message gracefully
        assert_eq!(
            backend_request.get("question").and_then(|v| v.as_str()),
            Some("")
        );
    }

    /// Test RHEL Lightspeed response transformation with valid backend response
    #[test]
    fn test_rhel_lightspeed_transform_response_valid() {
        let provider = RhelLightspeedProvider::new();
        let backend_response = json!({
            "data": {
                "text": "This is the assistant's response"
            }
        });
        
        let result = provider.transform_response(&backend_response, "test-model");
        
        assert!(result.is_ok(), "Transform should succeed");
        let response = result.unwrap();
        assert_eq!(response.model, "test-model");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.role, "assistant");
        assert_eq!(response.choices[0].message.content, "This is the assistant's response");
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
    }

    /// Test RHEL Lightspeed response transformation with usage data
    #[test]
    fn test_rhel_lightspeed_transform_response_with_usage() {
        let provider = RhelLightspeedProvider::new();
        let backend_response = json!({
            "data": {
                "text": "Response text"
            },
            "usage": {
                "prompt_tokens": 50,
                "completion_tokens": 30,
                "total_tokens": 80
            }
        });
        
        let result = provider.transform_response(&backend_response, "test-model");
        
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.usage.prompt_tokens, 50);
        assert_eq!(response.usage.completion_tokens, 30);
        assert_eq!(response.usage.total_tokens, 80);
    }

    /// Test RHEL Lightspeed response transformation with missing text field (error case)
    #[test]
    fn test_rhel_lightspeed_transform_response_missing_text() {
        let provider = RhelLightspeedProvider::new();
        
        // Missing "text" field
        let backend_response = json!({
            "data": {}
        });
        
        let result = provider.transform_response(&backend_response, "test-model");
        assert!(result.is_err(), "Should fail when text field is missing");
        
        // Missing "data" field entirely
        let backend_response = json!({
            "error": "something went wrong"
        });
        
        let result = provider.transform_response(&backend_response, "test-model");
        assert!(result.is_err(), "Should fail when data field is missing");
    }

    /// Test RHEL Lightspeed streaming text extraction
    #[test]
    fn test_rhel_lightspeed_extract_streaming_text() {
        let provider = RhelLightspeedProvider::new();
        let backend_response = json!({
            "data": {
                "text": "Streaming response content"
            }
        });
        
        let result = provider.extract_streaming_text(&backend_response);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Streaming response content");
    }

    /// Test RHEL Lightspeed streaming text extraction failure
    #[test]
    fn test_rhel_lightspeed_extract_streaming_text_error() {
        let provider = RhelLightspeedProvider::new();
        let backend_response = json!({
            "data": {
                "wrong_field": "value"
            }
        });
        
        let result = provider.extract_streaming_text(&backend_response);
        assert!(result.is_err(), "Should fail when text field is missing");
    }
}

