// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use log::error;

/// File + stdout logging for both `tracing` and `log` records (the latter
/// via LogTracer, replacing the removed Tauri log plugin).
fn init_logging() {
    let log_path = app_lib::log_file_path();
    let parent = log_path.parent().unwrap_or(std::path::Path::new("."));
    let name = log_path.file_name().unwrap_or_default();
    let file_appender = tracing_appender::rolling::never(parent, name);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .init();
    // Leak the guard so the background writer lives for the process lifetime.
    Box::leak(Box::new(guard));
    let _ = tracing_log::LogTracer::init();
}

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

    init_logging();
    if let Err(error) = app_lib::run().await {
        // The GUI logger may not have initialized when startup fails.
        eprintln!("Application startup failed: {error:#}");
        error!("Application startup failed: {error:#}");
        std::process::exit(1);
    }
}
