use once_cell::sync::Lazy;
use etcetera::{AppStrategyArgs};

pub static GOOSE_APP_STRATEGY: Lazy<AppStrategyArgs> = Lazy::new(|| AppStrategyArgs {
    top_level_domain: "Block".to_string(),
    author: "Block".to_string(),
    app_name: "goose".to_string(),
});