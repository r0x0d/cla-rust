use serde::{Deserialize, Serialize};
use serde_json::Value;

/// OpenAI chat completion request structure
/// This matches what Goose will send
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub n: Option<u32>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub stop: Option<Vec<String>>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    #[serde(default)]
    pub user: Option<String>,
    /// Additional fields that might be present
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, Value>,
}

/// Chat message structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// OpenAI chat completion response structure
/// This is what we need to return to Goose
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Streaming response chunk (for SSE)
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Models list response
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<Model>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub object: String,
    pub created: i64,
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
        };
        
        let json_str = serde_json::to_string(&msg).unwrap();
        assert!(!json_str.contains("name"));
    }
}

