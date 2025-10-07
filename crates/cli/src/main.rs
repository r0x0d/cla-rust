// Wrapper for goose binary that provides convenient 'c' command shortcuts.
// Maps: c -h → goose --help, c -i → goose session, c "query" → goose run -t "query"
// Unmapped subcommands pass through directly to goose.

mod config;

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};
use std::fs;
use etcetera::{choose_app_strategy, AppStrategy};
use serde_json::json;
use anyhow::Context;

use crate::config::GOOSE_APP_STRATEGY;

const DEFAULT_PATHS: &[&str] = &["/usr/bin/goose"];

const GOOSE_SUBCOMMANDS: &[&str] = &[
    "configure", "info", "mcp", "acp", "session", "s", "project", "p",
    "projects", "ps", "run", "recipe", "schedule", "sched", "update",
    "bench", "web",
];

fn find_goose() -> String {
    // Check environment variable first
    if let Ok(env_path) = env::var("GOOSE_BINARY") {
        if Path::new(&env_path).exists() {
            return env_path;
        }
    }
    
    // Check default paths
    for path in DEFAULT_PATHS {
        if Path::new(path).exists() {
            return path.to_string();
        }
    }
    
    eprintln!("Error: goose binary not found");
    exit(69);
}

fn is_goose_subcommand(arg: &str) -> bool {
    GOOSE_SUBCOMMANDS.contains(&arg)
}

fn ensure_goose_config_files() {
    let home_dir = choose_app_strategy(GOOSE_APP_STRATEGY.clone()).context("HOME environment variableF not set").unwrap();
    let config_dir = home_dir.in_config_dir("");
    let custom_providers_dir = config_dir.join("custom_providers");
    
    // Ensure directories exist
    if let Err(e) = fs::create_dir_all(&custom_providers_dir) {
        eprintln!("Error creating config directories: {}", e);
        exit(72);
    }
    
    // Check and create config.yaml
    let config_yaml_path = config_dir.join("config.yaml");
    if !config_yaml_path.exists() {
        let config_yaml_content = r#"OPENAI_HOST: http://127.0.0.1:8080
OPENAI_BASE_PATH: v1/chat/completions
GOOSE_MODEL: default-model
GOOSE_PROVIDER: custom_clad
extensions:
  memory:
    enabled: true
    type: builtin
    name: memory
    display_name: Memory
    description: null
    timeout: 300
    bundled: true
    available_tools: []
"#;
        if let Err(e) = fs::write(&config_yaml_path, config_yaml_content) {
            eprintln!("Error writing config.yaml: {}", e);
            exit(73);
        }
    }
    
    // Check and create clad.json
    let clad_json_path = custom_providers_dir.join("custom_clad.json");
    if !clad_json_path.exists() {
        let clad_json_content = json!({
            "name": "custom_clad",
            "engine": "openai",
            "display_name": "clad",
            "description": "Command Line Assistant Daemon Proxy",
            "api_key_env": "",
            "base_url": "http://127.0.0.1:8080",
            "models": [
                {
                    "name": "default-model",
                    "context_limit": 128000,
                    "input_token_cost": null,
                    "output_token_cost": null,
                    "currency": null,
                    "supports_cache_control": null
                }
            ],
            "headers": null,
            "timeout_seconds": null,
            "supports_streaming": false
        });
        
        let json_string = serde_json::to_string_pretty(&clad_json_content)
            .unwrap_or_else(|e| {
                eprintln!("Error serializing clad.json: {}", e);
                exit(75);
            });
        
        if let Err(e) = fs::write(&clad_json_path, json_string) {
            eprintln!("Error writing clad.json: {}", e);
            exit(74);
        }
    }
}

fn main() {
    // Ensure config files exist before running goose
    ensure_goose_config_files();
    
    let goose = find_goose();
    let args: Vec<String> = env::args().skip(1).collect();
    
    // Build command arguments
    let goose_args: Vec<String> = if args.is_empty() {
        // No arguments - show help
        vec!["--help".to_string()]
    } else if args[0] == "-h" || args[0] == "--help" {
        // Help flag
        vec!["--help".to_string()]
    } else if args[0] == "-i" || args[0] == "--interactive" {
        // Interactive mode
        vec!["session".to_string()]
    } else if is_goose_subcommand(&args[0]) {
        // Direct passthrough of goose subcommands
        args
    } else {
        // Treat as query text
        let mut cmd = vec!["run".to_string(), "-t".to_string()];
        cmd.push(args.join(" "));
        cmd
    };
    
    // Execute goose
    let status = Command::new(&goose)
        .args(&goose_args)
        .status();
    
    match status {
        Ok(exit_status) => {
            exit(exit_status.code().unwrap_or(1));
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(70);
        }
    }
}

