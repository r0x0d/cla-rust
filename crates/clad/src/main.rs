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
mod config;
mod state;
mod proxy;
mod openai;
mod provider;

use axum::{http::{HeaderValue, Method}, routing::{get, post}, Router};
use std::{net::SocketAddr, path::Path, sync::Arc};
use tower_http::cors::CorsLayer;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tracing::info;

use crate::{
    config::Config,
    proxy::{chat_completions_handler, health_check_handler, models_handler},
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
    // Load configuration first (before logging is initialized)
    let config_path = std::env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".to_string());
    let config_file = Path::new(&config_path).join("command-line-assistant").join("config.toml");
    
    // Load config to get the log level
    let config = match std::fs::read_to_string(&config_file) {
        Ok(contents) => {
            match toml::from_str::<Config>(&contents) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("Failed to parse config from {}: {}", config_file.display(), e);
                    eprintln!("Please check your config.toml file. See config.toml.example for reference.");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to read config from {}: {}", config_file.display(), e);
            eprintln!("Please create a config.toml file. See config.toml.example for reference.");
            std::process::exit(1);
        }
    };
    
    // Initialize logging with the configured log level
    let filter_string = config.get_tracing_filter();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| filter_string.into()),
        )
        .init();
    
    // Now emit deprecation warnings if needed
    if config.database.is_some() {
        tracing::warn!("The [database] configuration section is deprecated and will be removed in a future version");
    }
    
    if config.history.is_some() {
        tracing::warn!("The [history] configuration section is deprecated and will be removed in a future version");
    }
    
    if config.logging.audit.is_some() {
        tracing::warn!("The [logging.audit] configuration section is deprecated and will be removed in a future version");
    }
    
    info!("Using log level from config: {}", config.logging.level);
    
    // Warn about deprecated configurations
    config.backend.warn_if_using_deprecated_proxies();
    
    let proxy_config = config.get_proxy_config();
    info!("Starting proxy server on {}:{}", proxy_config.host, proxy_config.port);
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
    let config_arc = Arc::new(config);
    let state = AppState {
        config: config_arc.clone(),
        client,
        provider,
    };

    // Get the effective proxy configuration
    let proxy_config = config_arc.get_proxy_config();
    
    // Setup CORS with configured allowed origins
    let allowed_origins: Vec<HeaderValue> = proxy_config.allowed_origins
        .iter()
        .filter_map(|origin| {
            origin.parse::<HeaderValue>().ok().or_else(|| {
                tracing::warn!("Invalid CORS origin configured: {}", origin);
                None
            })
        })
        .collect();

    if allowed_origins.is_empty() {
        eprintln!("Error: No valid CORS origins configured");
        std::process::exit(1);
    }

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]);

    // Setup rate limiting
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(proxy_config.rate_limit_per_second)
            .burst_size(proxy_config.rate_limit_burst)
            .finish()
            .unwrap_or_else(|| {
                eprintln!("Failed to configure rate limiter with the given parameters");
                std::process::exit(1);
            })
    );

    let governor_layer = GovernorLayer {
        config: governor_conf,
    };

    // Build application with all middleware
    let app = Router::new()
        .route("/health", get(health_check_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        .route("/v1/models", get(models_handler))
        .layer(governor_layer)
        .layer(cors)
        .with_state(state);

    // Bind and serve
    let addr = format!("{}:{}", proxy_config.host, proxy_config.port);
    let socket_addr: SocketAddr = addr.parse().unwrap_or_else(|e| {
        eprintln!("Invalid address '{}': {}", addr, e);
        std::process::exit(1);
    });
    
    let listener = tokio::net::TcpListener::bind(socket_addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Failed to bind to {}: {}", socket_addr, e);
            eprintln!("Make sure the port is not in use and you have proper permissions");
            std::process::exit(1);
        });
    
    info!("Proxy server listening on {}", socket_addr);
    
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}

