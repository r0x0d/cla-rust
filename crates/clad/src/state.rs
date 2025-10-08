// Library interface for clad-redux
// This allows integration tests and external crates to use our modules

use std::sync::Arc;

use crate::config;
use crate::provider::Provider;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<config::Config>,
    pub client: reqwest::Client,
    pub provider: Arc<dyn Provider>,
}
