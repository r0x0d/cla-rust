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
use crate::openai::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ChunkChoice, Delta, Model, ModelsResponse};
use crate::provider::common::{uuid_simple, current_timestamp};

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
    // Use the provider's handle_request method
    let response = state.provider.handle_request(
        &state.client,
        state.config.clone(),
        request,
    ).await?;

    info!("Successfully processed non-streaming request");
    Ok(Json(response))
}

/// Handle streaming chat completion request
async fn handle_streaming_request(
    state: AppState,
    request: ChatCompletionRequest,
) -> Result<Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>>, AppError> {
    // Transform OpenAI request to backend format using the provider
    let backend_request = state.provider.transform_request(&request);

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

    // Extract the reply from the backend using the provider
    let generated_text = state.provider.extract_streaming_text(&backend_response)?;

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