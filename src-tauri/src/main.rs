// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use log::error;

#[tokio::main]
async fn main() {
    match std::env::args().nth(1).as_deref() {
        // Privileged LIO transaction protocol (JSON on stdin/stdout).
        // Invoked by the running application through sudo, never by users.
        Some("internal-iscsi") => {
            if let Err(error) = app_lib::infrastructure::iscsi::configfs::run_command() {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            return;
        }
        Some(other) => {
            eprintln!("unknown command: {other}");
            std::process::exit(2);
        }
        None => {}
    }

    if let Err(error) = app_lib::run().await {
        // The GUI logger may not have initialized when startup fails.
        eprintln!("Application startup failed: {error:#}");
        error!("Application startup failed: {error:#}");
        std::process::exit(1);
    }
}
