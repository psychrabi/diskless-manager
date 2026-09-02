// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_lib::client;
use app_lib::state::AppState;
use app_lib::types::AddClientRequest;
use clap::{Parser, Subcommand};
use log::info;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new client
    AutoAddClient {
        #[arg(long, default_value = "")]
        name: String,
        #[arg(long)]
        mac: String,
        #[arg(long)]
        ip: String,
        #[arg(long, default_value = "")]
        master: Option<String>,
        #[arg(long)]
        snapshot: Option<String>,
        #[arg(long)]
        keep_writeback: Option<bool>,
        #[arg(long)]
        use_game_disk: Option<bool>,
    },
}

fn init_cli_logging() {
    let log_path = app_lib::log_file_path();
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file_appender = tracing_appender::rolling::never(
        log_path.parent().unwrap_or(std::path::Path::new(".")),
        log_path.file_name().unwrap_or_default(),
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_level(true)
        .with_target(true)
        .init();
    // Leak the guard so the background writer lives for the process lifetime.
    Box::leak(Box::new(_guard));
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Some(Commands::AutoAddClient {
        name,
        mac,
        ip,
        master,
        snapshot,
        keep_writeback,
        use_game_disk,
    }) = cli.command
    {
        init_cli_logging();
        info!("Auto adding client: {}", name);
        let req = AddClientRequest {
            name,
            mac,
            ip,
            master: master.unwrap_or_default(),
            snapshot,
            keep_writeback: keep_writeback.or(Some(true)),
            use_game_disk,
        };

        let Ok(state) = AppState::new().await else {
            log::error!("Failed to initialize AppState");
            std::process::exit(1);
        };
        match client::add_client_impl(&state, req).await {
            Ok(v) => info!("Success auto adding: {}", v),
            Err(e) => {
                log::error!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    if let Err(error) = app_lib::run().await {
        log::error!("Application startup failed: {error:#}");
        std::process::exit(1);
    }
}
