// CLAD-REDUX: OpenAI-Compatible Proxy for Goose
//
// This proxy sits between Goose (https://github.com/block/goose) and the
// command-line-assistant backend (https://github.com/rhel-lightspeed/command-line-assistant).
//
// The proxy translates between:
// - Goose's OpenAI-compatible chat completion API
// - command-line-assistant's custom message/context API format
//
// SETUP:
// 1. Copy config.toml.example to config.toml and configure:
//    - backend.endpoint: Your inference backend endpoint
//    - backend.auth: Certificate and key files for authentication
//    - proxy.host/port: Where this proxy will listen (default: 127.0.0.1:8080)
//
// 2. Build and run:
//    $ cargo build --release
//    $ cargo run --release
//
// 3. Configure Goose to use this proxy:
//    In your Goose configuration (typically ~/.config/goose/config.yaml), add:
//    
//    providers:
//      - name: custom
//        provider: openai
//        base_url: http://127.0.0.1:8080
//        model: default-model
//
// TRANSFORMATION DETAILS:
// - OpenAI format: { "model": "...", "messages": [{"role": "...", "content": "..."}] }
//   → Backend format: { "message": "...", "context": [...] }
// - Backend format: { "data": {"text": "assistant response" } }
//   → OpenAI format: { "choices": [{"message": {"content": "..."}}] }
//
// The transformationoo functions are in src/proxy.rs if you need to adjust them.

mod config;
mod state;
mod proxy;
mod openai;
mod provider;

use axum::{routing::{get, post}, Router};
use std::{path::Path, sync::Arc};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::{
    config::Config,
    proxy::{chat_completions_handler, models_handler},
    state::AppState,
    provider::{
        common::create_authenticated_client,
        rhel_lightspeed::RhelLightspeedProvider,
        lightspeed_core::LightspeedCoreProvider,
    },
};

/// Create a provider based on the configuration
fn create_provider(config: &Config) -> Result<Arc<dyn crate::provider::Provider>, Box<dyn std::error::Error>> {
    match config.backend.provider.as_str() {
        "rhel_lightspeed" => {
            info!("Using RHEL Lightspeed provider");
            Ok(Arc::new(RhelLightspeedProvider::new()))
        }
        "lightspeed_core" => {
            info!("Using Lightspeed Core provider");
            Ok(Arc::new(LightspeedCoreProvider::new()))
        }
        unknown => {
            Err(format!("Unknown provider: {}. Valid options are: rhel_lightspeed, lightspeed_core", unknown).into())
        }
    }
}

/// Main entry point for the proxy server
#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "clad=debug,tower_http=debug".into()),
        )
        .init();

    // Load configuration from TOML file
    let config_path = std::env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".to_string());
    let config_file = Path::new(&config_path).join("command-line-assistant").join("config.toml");
    let config = Config::from_file(&config_file)
        .unwrap_or_else(|e| {
            eprintln!("Failed to load config from {}: {}", config_path, e);
            eprintln!("Please create a config.toml file. See config.toml.example for reference.");
            std::process::exit(1);
        });
    
    info!("Starting proxy server on {}:{}", config.proxy.host, config.proxy.port);
    info!("Forwarding requests to: {}", config.backend.endpoint);

    // Create HTTP client with certificate-based authentication
    let client = create_authenticated_client(&config)
        .unwrap_or_else(|e| {
            eprintln!("Failed to create HTTP client: {}", e);
            std::process::exit(1);
        });

    // Create provider based on configuration
    let provider = create_provider(&config)
        .unwrap_or_else(|e| {
            eprintln!("Failed to create provider: {}", e);
            std::process::exit(1);
        });

    // Create shared state
    let state = AppState {
        config: Arc::new(config.clone()),
        client,
        provider,
    };

    // Setup CORS to allow Goose to connect
    let cors = CorsLayer::new()
        .allow_origin(Any);

    let app = Router::new()
        .route("/v1/chat/completions", post(chat_completions_handler))
        .route("/v1/models", get(models_handler))
        .layer(cors)
        .with_state(state);

    // Bind and serve
    let addr = format!("{}:{}", config.proxy.host, config.proxy.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    
    info!("Proxy server listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

