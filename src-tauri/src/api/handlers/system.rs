use axum::{extract::State, http::StatusCode, Json};

use crate::core::service::ServiceManager;
use crate::state::AppState;

pub async fn get_system_info(
    State(_state): State<AppState>,
) -> Result<Json<crate::commands::system::SystemInfo>, StatusCode> {
    // Call the existing Tauri command function directly - it doesn't need state
    match crate::commands::system::get_system_info().await {
        Ok(info) => Ok(Json(info)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_server_status(
    State(state): State<AppState>,
) -> Result<Json<crate::commands::system::ServerStatus>, StatusCode> {
    // Replicate the logic from the Tauri command
    let service_manager = ServiceManager::new();
    let services = service_manager.list_services();
    let services_running = services.iter().filter(|s| s.running).count() as u32;

    let clients_count: (i64,) = match sqlx::query_as("SELECT COUNT(*) FROM clients")
        .fetch_one(&state.db_pool)
        .await
    {
        Ok(count) => count,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let images_count: (i64,) = match sqlx::query_as("SELECT COUNT(*) FROM images")
        .fetch_one(&state.db_pool)
        .await
    {
        Ok(count) => count,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let status = crate::commands::system::ServerStatus {
        initialized: true,
        services_running,
        services_total: services.len() as u32,
        clients_count: clients_count.0 as u32,
        images_count: images_count.0 as u32,
    };

    Ok(Json(status))
}
