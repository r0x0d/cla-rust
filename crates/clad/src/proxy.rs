use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response, Sse},
    Json,
};
use futures::stream::{self, Stream, StreamExt};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info};

// Update this import to match your actual module structure.
// If you don't have an openai.rs file, create one in src/ and define the required structs there.
use crate::state::AppState;
use crate::openai::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, Choice, ChunkChoice, Delta, Message, Model, ModelsResponse, Usage};

/// Handler for /v1/chat/completions endpoint
/// This receives OpenAI-compatible requests from Goose
pub async fn chat_completions_handler(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, AppError> {
    info!("Received chat completion request for model: {}", request.model);
    debug!("Request: {:?}", ::serde_json::to_string_pretty(&request));

    // Check if streaming is requested
    let is_streaming = request.stream.unwrap_or(false);

    if is_streaming {
        info!("Streaming response requested");
        Ok(handle_streaming_request(state, request).await?.into_response())
    } else {
        info!("Non-streaming response requested");
        Ok(handle_non_streaming_request(state, request).await?.into_response())
    }
}

/// Handle non-streaming chat completion request
async fn handle_non_streaming_request(
    state: AppState,
    request: ChatCompletionRequest,
) -> Result<Json<ChatCompletionResponse>, AppError> {
    // Transform OpenAI request to your external backend format
    // CUSTOMIZE THIS: Modify based on your backend's API specification
    let backend_request = transform_to_backend_format(&request);

    println!("Backend request: {:?}", backend_request);
    // Forward request to external backend
    let backend_req = state
        .client
        .post(&state.config.backend.endpoint)
        .json(&backend_request);

    let response = backend_req
        .send()
        .await
        .map_err(|e| {
            error!("Failed to send request to backend: {}", e);
            AppError::BackendError(e.to_string())
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        error!("Backend returned error {}: {}", status, error_body);
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
            error!("Failed to parse backend response: {}", e);
            AppError::BackendError(e.to_string())
        })?;

    // Transform backend response to OpenAI format
    // CUSTOMIZE THIS: Modify based on your backend's response format
    let openai_response = transform_to_openai_format(&backend_response, &request.model)?;

    info!("Successfully processed non-streaming request");
    Ok(Json(openai_response))
}

/// Handle streaming chat completion request
async fn handle_streaming_request(
    state: AppState,
    request: ChatCompletionRequest,
) -> Result<Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>>, AppError> {
    // Transform OpenAI request to backend format
    let backend_request = transform_to_backend_format(&request);

    // Forward request to external backend
    let backend_req = state
        .client
        .post(&state.config.backend.endpoint)
        .json(&backend_request);

    let response = backend_req
        .send()
        .await
        .map_err(|e| {
            error!("Failed to send request to backend: {}", e);
            AppError::BackendError(e.to_string())
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        error!("Backend returned error {}: {}", status, error_body);
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
            error!("Failed to parse backend response: {}", e);
            AppError::BackendError(e.to_string())
        })?;

    debug!("Backend response for streaming: {:?}", backend_response);

    // Extract the reply from the backend
    let generated_text = backend_response
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::TransformError(format!(
                "Could not extract reply from backend response for streaming"
            ))
        })?
        .to_string();

    // Create streaming chunks
    let stream = create_streaming_chunks(generated_text, request.model);

    info!("Successfully started streaming response");
    Ok(Sse::new(stream))
}

/// Create a stream of SSE events from the complete response text
/// This simulates streaming by breaking the response into chunks
fn create_streaming_chunks(
    text: String,
    model: String,
) -> impl Stream<Item = Result<axum::response::sse::Event, Infallible>> {
    let chunk_id = format!("chatcmpl-{}", uuid_simple());
    let created = current_timestamp();

    // Split text into words for streaming simulation
    let words: Vec<String> = text.split_whitespace().map(|s| format!("{} ", s)).collect();
    let total_chunks = words.len();

    stream::iter(0..=total_chunks).then(move |i| {
        let chunk_id = chunk_id.clone();
        let model = model.clone();
        let words = words.clone();

        async move {
            // Small delay to simulate streaming
            if i > 0 {
                sleep(Duration::from_millis(20)).await;
            }

            let chunk = if i == 0 {
                // First chunk: send role
                ChatCompletionChunk {
                    id: chunk_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created,
                    model: model.clone(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: Delta {
                            role: Some("assistant".to_string()),
                            content: None,
                        },
                        finish_reason: None,
                    }],
                }
            } else if i < total_chunks {
                // Middle chunks: send content
                ChatCompletionChunk {
                    id: chunk_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created,
                    model: model.clone(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: Some(words[i].clone()),
                        },
                        finish_reason: None,
                    }],
                }
            } else {
                // Last chunk: send finish reason
                ChatCompletionChunk {
                    id: chunk_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created,
                    model: model.clone(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: None,
                        },
                        finish_reason: Some("stop".to_string()),
                    }],
                }
            };

            let json_str = serde_json::to_string(&chunk).unwrap();
            Ok::<_, Infallible>(axum::response::sse::Event::default().data(json_str))
        }
    })
}

/// Handler for /v1/models endpoint
/// Returns a list of available models
pub async fn models_handler(State(_state): State<AppState>) -> Json<ModelsResponse> {
    // CUSTOMIZE THIS: Return your actual available models
    Json(ModelsResponse {
        object: "list".to_string(),
        data: vec![
            Model {
                id: "default-model".to_string(),
                object: "model".to_string(),
                created: 1234567890,
                owned_by: "clad-redux".to_string(),
            },
        ],
    })
}

/// Transform OpenAI request to command-line-assistant backend format
/// Based on: https://github.com/rhel-lightspeed/command-line-assistant/blob/main/command_line_assistant/dbus/interfaces/chat.py
pub(crate) fn transform_to_backend_format(openai_req: &ChatCompletionRequest) -> Value {
    // The command-line-assistant backend expects:
    // {
    //   "message": "user's current message",
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
    let context: Vec<Value> = openai_req
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
pub(crate) fn transform_to_openai_format(
    backend_resp: &Value,
    model: &str,
) -> Result<ChatCompletionResponse, AppError> {
    // The command-line-assistant backend returns:
    // {
    //   "reply": "assistant's response text",
    //   ... possibly other fields ...
    // }
    
    // Extract the "reply" field from the backend response
    let generated_text = backend_resp
        .get("data")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::TransformError(format!(
                "Could not extract 'reply' field from backend response. Response: {:?}",
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
        id: format!("chatcmpl-{}", uuid_simple()),
        object: "chat.completion".to_string(),
        created: current_timestamp(),
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

/// Helper: Generate a simple UUID-like identifier
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", now)
}

/// Helper: Get current Unix timestamp
fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Error types for the proxy
#[derive(Debug)]
pub enum AppError {
    BackendError(String),
    TransformError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::BackendError(msg) => (StatusCode::BAD_GATEWAY, msg),
            AppError::TransformError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = json!({
            "error": {
                "message": message,
                "type": "proxy_error",
            }
        });

        (status, Json(body)).into_response()
    }
}