//! Chat command implementation
//!
//! This module handles the chat functionality, including both interactive
//! mode and quick query mode.

use std::path::PathBuf;
use std::process::{Command, exit};
use clap::Parser;
use log::{debug, error, info};

use crate::helpers::{
    is_goose_subcommand, validate_args, ensure_goose_config_files,
    find_goose, get_filtered_env, status_to_exit_code,
    EX_UNAVAILABLE, EX_SOFTWARE, EX_OSERR, EX_CANTCREAT,
};

/// Command Line Assistant (c) - Your Quick AI Helper
#[derive(Parser, Debug)]
#[command(
    name = "c",
    author,
    version,
    about = "Command Line Assistant (c) - Your Quick AI Helper",
    long_about = "The 'c' command provides a simplified interface for quick AI assistance.\n\
                  Simply type your question or request as natural text after 'c'.\n\n\
                  Interactive Mode:\n  \
                    Use 'c -i' to start a continuous conversation session where you can\n  \
                    ask multiple questions and get detailed assistance.\n\n\
                  Quick Queries:\n  \
                    Use 'c \"your question\"' for one-off questions. The AI will provide\n  \
                    a response and exit.",
    after_help = "EXAMPLES:\n    \
                  c -i                    # Start interactive session\n    \
                  c \"how do I list files\" # Ask a quick question\n    \
                  c explain this code     # Multi-word queries work naturally",
    disable_help_subcommand = true,
)]
pub struct Cli {
    /// Start an interactive session
    #[arg(short, long, conflicts_with = "query")]
    pub interactive: bool,

    /// Your question or query (everything after 'c' that isn't a flag)
    #[arg(trailing_var_arg = true, allow_hyphen_values = false)]
    pub query: Vec<String>,
}

impl Cli {
    /// Execute the chat command
    pub fn execute(&self) {
        // Check if first query argument is a restricted goose subcommand
        if !self.interactive && !self.query.is_empty() && is_goose_subcommand(&self.query[0]) {
            eprintln!("Error: Direct goose subcommands are not available.");
            eprintln!("Please use the simplified interface instead.");
            eprintln!("\nRun 'c --help' for usage information.");
            exit(1);
        }
        
        // If no interactive flag and no query, show error
        if !self.interactive && self.query.is_empty() {
            eprintln!("Error: You must either provide a query or use the -i/--interactive flag.");
            eprintln!("\nRun 'c --help' for usage information.");
            exit(1);
        }
        
        // Validate arguments
        if let Err(e) = validate_args(&self.query) {
            error!("Invalid arguments: {}", e);
            eprintln!("Error: {}", e);
            exit(EX_SOFTWARE);
        }
            
        // Ensure config files exist before running goose
        if let Err(e) = ensure_goose_config_files() {
            error!("Failed to ensure config files: {:#}", e);
            eprintln!("Error setting up configuration: {}", e);
            eprintln!("This may be due to insufficient permissions or disk space.");
            exit(EX_CANTCREAT);
        }
        
        // Find the goose binary
        let goose = match find_goose() {
            Ok(path) => path,
            Err(e) => {
                error!("Failed to find goose binary: {:#}", e);
                eprintln!("Error: goose binary not found");
                eprintln!("Please ensure goose is installed at /usr/bin/goose");
                eprintln!("Or set GOOSE_BINARY environment variable to the correct path");
                exit(EX_UNAVAILABLE);
            }
        };
        
        info!("Using goose binary: {:?}", goose);
        
        // Build command arguments
        let goose_args = self.build_goose_args();
        debug!("Goose arguments: {:?}", goose_args);
        
        // Execute goose
        self.run_goose(&goose, &goose_args);
    }

    /// Build the argument vector for the goose command
    fn build_goose_args(&self) -> Vec<String> {
        if self.interactive {
            // Interactive mode
            debug!("Interactive mode requested");
            return vec!["session".to_string()];
        }
        
        // Treat as query text - Pass each argument separately
        debug!("Treating as query with {} arguments", self.query.len());
        let mut cmd = vec!["run".to_string(), "-t".to_string()];
        
        // SECURITY: Don't join arguments - pass them separately
        // The goose binary will handle them appropriately
        cmd.extend_from_slice(&self.query);
        cmd
    }

    /// Run the goose command with the given arguments
    fn run_goose(&self, goose: &PathBuf, goose_args: &[String]) {
        // Filter environment variables for security
        let filtered_env = get_filtered_env();
        debug!("Passing {} filtered environment variables", filtered_env.len());
        
        // Execute goose with proper I/O inheritance
        let mut cmd = Command::new(goose);
        cmd.args(goose_args)
            .env_clear()  // Clear all env vars first
            .envs(filtered_env)  // Then set only filtered ones
            .stdin(std::process::Stdio::inherit())   // Inherit stdin for interactive mode
            .stdout(std::process::Stdio::inherit())  // Inherit stdout for output
            .stderr(std::process::Stdio::inherit()); // Inherit stderr for errors
        
        debug!("Spawning goose process");
        
        match cmd.spawn() {
            Ok(mut child) => {
                debug!("Child process spawned with PID: {}", child.id());
                
                // Wait for child process to complete
                // Note: Signals (SIGINT, SIGTERM, etc.) are automatically sent to the
                // entire process group by the OS, so the child will receive them naturally.
                // The parent's wait() call will be interrupted by signals, allowing proper
                // cleanup and exit code propagation.
                match child.wait() {
                    Ok(exit_status) => {
                        let exit_code = status_to_exit_code(exit_status);
                        info!("Goose process completed with exit code: {}", exit_code);
                        exit(exit_code);
                    }
                    Err(e) => {
                        error!("Failed to wait for goose process: {}", e);
                        eprintln!("Error waiting for goose process: {}", e);
                        exit(EX_OSERR);
                    }
                }
            }
            Err(e) => {
                error!("Failed to execute goose: {}", e);
                eprintln!("Error executing goose: {}", e);
                eprintln!("Command: {:?}", goose);
                exit(EX_SOFTWARE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Tests for build_goose_args - CRITICAL for proper command handling
    // ============================================================================

    #[test]
    fn test_build_goose_args_interactive() {
        let cli = Cli {
            interactive: true,
            query: vec![],
        };
        let result = cli.build_goose_args();
        assert_eq!(result, vec!["session".to_string()]);
    }

    #[test]
    fn test_build_goose_args_query() {
        let cli = Cli {
            interactive: false,
            query: vec!["what".to_string(), "is".to_string(), "rust".to_string()],
        };
        let result = cli.build_goose_args();
        
        // Should be: ["run", "-t", "what", "is", "rust"]
        assert_eq!(result[0], "run");
        assert_eq!(result[1], "-t");
        assert_eq!(result[2], "what");
        assert_eq!(result[3], "is");
        assert_eq!(result[4], "rust");
    }

    #[test]
    fn test_build_goose_args_query_with_special_chars() {
        let cli = Cli {
            interactive: false,
            query: vec!["query with spaces".to_string(), "and!special#chars".to_string()],
        };
        let result = cli.build_goose_args();
        
        assert_eq!(result[0], "run");
        assert_eq!(result[1], "-t");
        assert_eq!(result[2], "query with spaces");
        assert_eq!(result[3], "and!special#chars");
    }

    #[test]
    fn test_build_goose_args_preserves_arg_boundaries() {
        // Critical security test: ensure arguments are passed separately
        let cli = Cli {
            interactive: false,
            query: vec!["arg1".to_string(), "arg2".to_string(), "arg3".to_string()],
        };
        let result = cli.build_goose_args();
        
        // Should be: ["run", "-t", "arg1", "arg2", "arg3"]
        assert_eq!(result.len(), 5);
        assert!(result.iter().all(|arg| !arg.contains(' ')), 
                "Arguments should not be joined with spaces");
    }
}

