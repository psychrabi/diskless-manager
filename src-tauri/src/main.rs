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

        let state = AppState::new()
            .await
            .expect("Failed to initialize AppState");
        match client::add_client_impl(&state, req).await {
            Ok(v) => info!("Success auto adding: {}", v),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    app_lib::run().await;
}
