use serde::{Deserialize, Serialize};
use serde_json::Value;

/// OpenAI chat completion request structure
/// This matches what Goose will send
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatCompletionRequest {
    /// Model ID
    pub model: String,
    /// List of messages
    pub messages: Vec<Message>,
    /// Temperature
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Top P
    #[serde(default)]
    pub top_p: Option<f32>,
    /// Number of completions
    #[serde(default)]
    pub n: Option<u32>,
    /// Stream
    #[serde(default)]
    pub stream: Option<bool>,
    /// Stop
    #[serde(default)]
    pub stop: Option<Vec<String>>,
    /// Maximum tokens
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Presence penalty
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    /// Frequency penalty
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    /// User
    #[serde(default)]
    pub user: Option<String>,
    /// Tools
    #[serde(default)]
    pub tools: Option<Vec<Tool>>,
    /// Tool choice
    #[serde(default)]
    pub tool_choice: Option<Value>,
    /// Additional fields that might be present
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, Value>,
}

/// Chat message structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    /// Role
    pub role: String,
    /// Content
    #[serde(default)]
    pub content: String,
    /// Name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool calls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Tool call structure for function calling
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    /// ID
    pub id: String,
    /// Call type
    #[serde(rename = "type")]
    pub call_type: String,
    /// Function
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionCall {
    /// Name
    pub name: String,
    /// Arguments
    pub arguments: String,
}

/// Tool definition structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tool {
    /// Tool type
    #[serde(rename = "type")]
    pub tool_type: String,  
    /// Function
    pub function: FunctionDefinition,
}

/// Function definition for tools
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionDefinition {
    /// Name
    pub name: String,
    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Parameters
    pub parameters: Value,
}

/// OpenAI chat completion response structure
/// This is what we need to return to Goose
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    /// ID
    pub id: String,
    /// Object
    pub object: String,
    /// Created
    pub created: i64,
    /// Model
    pub model: String,
    /// Choices
    pub choices: Vec<Choice>,
    /// Usage
    pub usage: Usage,
}

/// Choice structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Choice {
    /// Index
    pub index: u32,
    /// Message
    pub message: Message,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// Usage structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt tokens
    pub prompt_tokens: u32,
    /// Completion tokens
    pub completion_tokens: u32,
    /// Total tokens
    pub total_tokens: u32,
}

/// Streaming response chunk (for SSE)
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    /// ID
    pub id: String,
    /// Object
    pub object: String,
    /// Created
    pub created: i64,
    /// Model
    pub model: String,
    /// Choices
    pub choices: Vec<ChunkChoice>,
}

/// Chunk choice structure
#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkChoice {
    /// Index
    pub index: u32,
    /// Delta
    pub delta: Delta,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// Delta structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Role
    pub role: Option<String>,
    /// Content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Tool calls
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Models list response
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelsResponse {
    /// Object type
    pub object: String,
    /// List of models
    pub data: Vec<Model>,
}

/// Model structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Model {
    /// Model ID
    pub id: String,
    /// Object type
    pub object: String,
    /// Creation timestamp
    pub created: i64,
    /// Owner of the model
    pub owned_by: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test ChatCompletionRequest serialization/deserialization
    #[test]
    fn test_chat_completion_request_serde() {
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                    name: None,
                    tool_calls: None,
                },
            ],
            temperature: Some(0.8),
            top_p: None,
            n: None,
            stream: Some(false),
            stop: None,
            max_tokens: Some(1000),
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        
        // Serialize
        let json_str = serde_json::to_string(&request).unwrap();
        
        // Deserialize
        let deserialized: ChatCompletionRequest = serde_json::from_str(&json_str).unwrap();
        
        assert_eq!(deserialized.model, "gpt-4");
        assert_eq!(deserialized.messages.len(), 1);
        assert_eq!(deserialized.temperature, Some(0.8));
        assert_eq!(deserialized.max_tokens, Some(1000));
    }

    /// Test ChatCompletionRequest with extra fields
    #[test]
    fn test_chat_completion_request_with_extra_fields() {
        use serde_json::json;
        
        let json_data = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "custom_field": "custom_value",
            "another_field": 123
        });
        
        let request: ChatCompletionRequest = serde_json::from_value(json_data).unwrap();
        
        assert_eq!(request.model, "gpt-4");
        assert_eq!(request.messages.len(), 1);
        assert!(request.extra.contains_key("custom_field"));
        assert_eq!(request.extra.get("custom_field").unwrap().as_str(), Some("custom_value"));
    }

    /// Test Message with name field
    #[test]
    fn test_message_with_name() {
        let msg = Message {
            role: "user".to_string(),
            content: "Hello".to_string(),
            name: Some("John".to_string()),
            tool_calls: None,
        };
        
        let json_str = serde_json::to_string(&msg).unwrap();
        assert!(json_str.contains("name"));
        
        let deserialized: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.name, Some("John".to_string()));
    }

    /// Test Message without name field (should be omitted in JSON)
    #[test]
    fn test_message_without_name() {
        let msg = Message {
            role: "assistant".to_string(),
            content: "Hi there".to_string(),
            name: None,
            tool_calls: None,
        };
        
        let json_str = serde_json::to_string(&msg).unwrap();
        assert!(!json_str.contains("name"));
    }
}

