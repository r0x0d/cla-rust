use once_cell::sync::Lazy;
use etcetera::{AppStrategyArgs};

/// Goose application strategy configuration for determining config directory paths
pub static GOOSE_APP_STRATEGY: Lazy<AppStrategyArgs> = Lazy::new(|| AppStrategyArgs {
    top_level_domain: "Block".to_string(),
    author: "Block".to_string(),
    app_name: "goose".to_string(),
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goose_app_strategy_initialization() {
        // Test that the lazy static initializes correctly
        let strategy = &*GOOSE_APP_STRATEGY;
        
        assert_eq!(strategy.top_level_domain, "Block");
        assert_eq!(strategy.author, "Block");
        assert_eq!(strategy.app_name, "goose");
    }

    #[test]
    fn test_goose_app_strategy_is_immutable() {
        // Access multiple times to ensure it's consistently the same
        let strategy1 = &*GOOSE_APP_STRATEGY;
        let strategy2 = &*GOOSE_APP_STRATEGY;
        
        assert_eq!(strategy1.app_name, strategy2.app_name);
        assert_eq!(strategy1.author, strategy2.author);
        assert_eq!(strategy1.top_level_domain, strategy2.top_level_domain);
    }
}