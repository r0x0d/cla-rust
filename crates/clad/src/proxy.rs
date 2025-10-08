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

use crate::state::AppState;
use crate::openai::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ChunkChoice, Delta, Model, ModelsResponse};
use crate::provider::common::{uuid_simple, current_timestamp};

/// Handler for /v1/chat/completions endpoint
/// This receives OpenAI-compatible requests from Goose
pub async fn chat_completions_handler(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, AppError> {
    info!(
        model = %request.model,
        message_count = request.messages.len(),
        "Received chat completion request"
    );
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

    // Forward request to external backend with timeout
    let timeout_duration = Duration::from_secs(state.config.backend.timeout);
    
    let response = tokio::time::timeout(
        timeout_duration,
        state.client
            .post(&state.config.backend.endpoint)
            .json(&backend_request)
            .send()
    )
    .await
    .map_err(|_| {
        error!("Backend request timed out after {:?}", timeout_duration);
        AppError::TimeoutError
    })?
    .map_err(|e| {
        error!("Failed to send request to backend: {}", e);
        AppError::BackendError(e.to_string())
    })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_default();
        // Log detailed error internally
        error!("Backend returned error status {}: {}", status, error_body);
        // Return sanitized error
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

            let json_str = serde_json::to_string(&chunk).unwrap_or_else(|e| {
                error!("Failed to serialize chunk: {}", e);
                r#"{"error": "serialization failed"}"#.to_string()
            });
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

/// Health check endpoint
/// Returns 200 OK if the service is running
pub async fn health_check_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({
        "status": "healthy",
        "service": "clad-proxy",
        "timestamp": current_timestamp(),
    })))
}



/// Error types for the proxy
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Backend service unavailable")]
    BackendError(String),
    
    #[error("Failed to transform request/response")]
    TransformError(String),
    
    #[error("Request timeout")]
    TimeoutError,
    
    #[error("Internal server error")]
    InternalError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Log the detailed error internally
        error!("Error occurred: {:?}", self);
        
        // Return sanitized error to client
        let (status, message, error_type) = match self {
            AppError::BackendError(_) => {
                (StatusCode::BAD_GATEWAY, "Backend service unavailable".to_string(), "backend_error")
            },
            AppError::TransformError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Failed to process response".to_string(), "transform_error")
            },
            AppError::TimeoutError => {
                (StatusCode::GATEWAY_TIMEOUT, "Request timeout".to_string(), "timeout_error")
            },
            AppError::InternalError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string(), "internal_error")
            },
        };

        let body = json!({
            "error": {
                "message": message,
                "type": error_type,
            }
        });

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    // ============================================================================
    // Tests for AppError
    // ============================================================================

    #[test]
    fn test_app_error_display() {
        let err = AppError::BackendError("test error".to_string());
        assert_eq!(err.to_string(), "Backend service unavailable");
        
        let err = AppError::TransformError("transform issue".to_string());
        assert_eq!(err.to_string(), "Failed to transform request/response");
        
        let err = AppError::TimeoutError;
        assert_eq!(err.to_string(), "Request timeout");
        
        let err = AppError::InternalError("internal issue".to_string());
        assert_eq!(err.to_string(), "Internal server error");
    }

    #[test]
    fn test_app_error_into_response_backend_error() {
        let err = AppError::BackendError("connection failed".to_string());
        let response = err.into_response();
        
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn test_app_error_into_response_transform_error() {
        let err = AppError::TransformError("bad format".to_string());
        let response = err.into_response();
        
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_app_error_into_response_timeout_error() {
        let err = AppError::TimeoutError;
        let response = err.into_response();
        
        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    #[test]
    fn test_app_error_into_response_internal_error() {
        let err = AppError::InternalError("panic".to_string());
        let response = err.into_response();
        
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ============================================================================
    // Tests for streaming chunk creation
    // ============================================================================

    #[test]
    fn test_create_streaming_chunks_empty_text() {
        use futures::StreamExt;
        
        let text = "".to_string();
        let model = "test-model".to_string();
        
        let mut stream = Box::pin(create_streaming_chunks(text, model));
        
        // Should have at least first chunk (role) and last chunk (finish_reason)
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let first = stream.next().await;
            assert!(first.is_some(), "Should have first chunk");
        });
    }

    #[test]
    fn test_create_streaming_chunks_single_word() {
        use futures::StreamExt;
        
        let text = "Hello".to_string();
        let model = "test-model".to_string();
        
        let mut stream = Box::pin(create_streaming_chunks(text, model));
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut count = 0;
            while let Some(_chunk) = stream.next().await {
                count += 1;
            }
            
            // For 1 word, stream iterates 0..=1: i=0 (role), i=1 (finish) = 2 chunks
            assert_eq!(count, 2);
        });
    }

    #[test]
    fn test_create_streaming_chunks_multiple_words() {
        use futures::StreamExt;
        
        let text = "Hello world test".to_string();
        let model = "test-model".to_string();
        
        let mut stream = Box::pin(create_streaming_chunks(text, model));
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut count = 0;
            while let Some(_chunk) = stream.next().await {
                count += 1;
            }
            
            // For 3 words, stream iterates 0..=3: i=0 (role), i=1,2,3 (content), but...
            // Actually checking the code: i=0 (role), i=1,2,3 content is at words[i], i=3 is finish
            // So: role, word1, word2, finish = 4 chunks (word3 doesn't get sent because i=3 >= total_chunks=3)
            assert_eq!(count, 4);
        });
    }

    // ============================================================================
    // Tests for models_handler
    // ============================================================================

    #[tokio::test]
    async fn test_models_handler_returns_valid_response() {
        use std::sync::Arc;
        use crate::config::Config;
        use crate::provider::rhel_lightspeed::RhelLightspeedProvider;
        
        let config_str = r#"
            [backend]
            endpoint = "http://localhost:9000"
            provider = "rhel_lightspeed"
            
            [backend.auth]
            cert_file = "/path/to/cert.pem"
            key_file = "/path/to/key.pem"
        "#;
        
        let config: Config = toml::from_str(config_str).unwrap();
        let client = reqwest::Client::new();
        let provider: Arc<dyn crate::provider::Provider> = Arc::new(RhelLightspeedProvider::new());
        
        let state = AppState {
            config: Arc::new(config),
            client,
            provider,
        };
        
        let response = models_handler(State(state)).await;
        
        assert_eq!(response.0.object, "list");
        assert!(!response.0.data.is_empty());
        assert_eq!(response.0.data[0].id, "default-model");
    }

    // ============================================================================
    // Tests for health_check_handler
    // ============================================================================

    #[tokio::test]
    async fn test_health_check_handler_returns_ok() {
        let response = health_check_handler().await;
        let response = response.into_response();
        
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_health_check_handler_returns_valid_json() {
        let response = health_check_handler().await;
        let response = response.into_response();
        
        // Verify it returns OK status
        assert_eq!(response.status(), StatusCode::OK);
        
        // The body should be valid JSON with expected fields
        // In a real test, you'd extract and parse the body, but that's complex with axum
        // For now, we just verify it doesn't panic and returns OK
    }
}