use std::sync::Arc;
use color_eyre::Result;
use clap::{Arg, Command};

mod domain;
mod ports;
mod adapters;
mod application;

use adapters::{
    api::{AsanaClient, AsanaTaskRepository},
    cache::MokaCacheAdapter,
    config::FileConfigStore,
    tui::{App, run_tui},
};
use application::{TaskService, StateManager, AppError};
use ports::ConfigStore;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize color-eyre for better error reporting
    color_eyre::install()?;

    // Initialize logging to file
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("asana-cli.log")?;
    
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Parse command line arguments
    let matches = Command::new("asana-cli")
        .version("0.1.0")
        .about("A Terminal User Interface for Asana")
        .long_about("A fast, keyboard-driven terminal interface for managing Asana tasks.\n\nIf you have only one workspace, it will be auto-selected.\nFor multiple workspaces, specify one with --workspace.")
        .arg(
            Arg::new("token")
                .long("token")
                .value_name("TOKEN")
                .help("Asana API token (can also be set via ASANA_TOKEN env var)")
                .global(true)
        )
        .arg(
            Arg::new("workspace")
                .long("workspace")
                .value_name("WORKSPACE_ID")
                .help("Workspace ID (required only if you have multiple workspaces)")
                .global(true)
        )
        .subcommand(
            Command::new("tasks")
                .about("Task operations")
                .subcommand(
                    Command::new("list")
                        .about("List tasks as JSON")
                )
        )
        .get_matches();

    // Load configuration
    let config_store = Arc::new(FileConfigStore::new()?);
    let mut config = config_store.load_config().await?;

    // Override with command line arguments or environment variables
    if let Some(token) = matches.get_one::<String>("token") {
        config.api_token = Some(token.clone());
    } else if let Ok(token) = std::env::var("ASANA_TOKEN") {
        config.api_token = Some(token);
    }

    if let Some(workspace) = matches.get_one::<String>("workspace") {
        config.default_workspace = Some(workspace.as_str().into());
    }

    // Check for API token
    let api_token = config.api_token.clone().ok_or_else(|| {
        eprintln!("❌ No Asana API token found!");
        eprintln!();
        eprintln!("To get started:");
        eprintln!("1. Get your personal access token from https://developers.asana.com/");
        eprintln!("2. Run: export ASANA_TOKEN=your_token_here");
        eprintln!("3. Or run: {} --token your_token_here", std::env::args().next().unwrap_or_else(|| "asana-cli".to_string()));
        eprintln!();
        AppError::AuthenticationRequired
    })?;

    // Save config if we got new values
    config_store.save_config(&config).await?;

    // Create dependencies
    let api_client = AsanaClient::new(api_token);
    let task_repo = Arc::new(AsanaTaskRepository::new(api_client));
    
    // Create caches
    let task_cache = Arc::new(MokaCacheAdapter::new(config.cache_ttl_seconds, 1000));
    let comment_cache = Arc::new(MokaCacheAdapter::new(config.cache_ttl_seconds, 1000));
    
    // Create application services
    let task_service = Arc::new(TaskService::new(
        task_repo.clone(),
        task_cache,
        comment_cache,
    ));
    
    let state_manager = Arc::new(StateManager::new(
        task_service,
        task_repo.clone(),
        task_repo.clone(),
        config_store,
    ));

    // Handle subcommands
    match matches.subcommand() {
        Some(("tasks", tasks_matches)) => {
            match tasks_matches.subcommand() {
                Some(("list", _)) => {
                    // Initialize state manager
                    state_manager.initialize().await?;
                    
                    // Get tasks
                    match state_manager.get_tasks_for_current_workspace(false).await {
                        Ok(tasks) => {
                            let json = serde_json::to_string_pretty(&tasks)?;
                            println!("{}", json);
                        }
                        Err(e) => {
                            eprintln!("❌ Failed to list tasks: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                _ => {
                    eprintln!("❌ Unknown tasks subcommand");
                    std::process::exit(1);
                }
            }
        }
        None => {
            // Default behavior - run TUI
            let app = App::new(state_manager);
            
            if let Err(e) = run_tui(app).await {
                match &e.downcast_ref::<AppError>() {
                    Some(AppError::Application(msg)) => {
                        eprintln!("❌ {}", msg);
                        eprintln!();
                        eprintln!("💡 Tip: Use 'cargo run -- --help' for more options");
                    }
                    Some(AppError::AuthenticationRequired) => {
                        // Already handled above with nice formatting
                        eprintln!("❌ Authentication required");
                    }
                    _ => {
                        eprintln!("❌ Application error: {}", e);
                    }
                }
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("❌ Unknown command");
            std::process::exit(1);
        }
    }

    Ok(())
}