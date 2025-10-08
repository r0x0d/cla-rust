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

