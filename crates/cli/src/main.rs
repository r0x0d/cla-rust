//! Wrapper for goose binary that provides convenient 'c' command shortcuts.
//! Maps: c -h → goose --help, c -i → goose session, c "query" → goose run -t "query"
//! Unmapped subcommands pass through directly to goose.

mod config;
mod commands;
mod helpers;

use clap::Parser;
use log::info;

use crate::commands::chat::Cli;

fn main() {
    // Initialize logging - responds to RUST_LOG environment variable
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp(None)
        .init();
    
    info!("Command Line Assistant CLI starting");
    
    // Parse command-line arguments and execute
    let cli = Cli::parse();
    cli.execute();
}
