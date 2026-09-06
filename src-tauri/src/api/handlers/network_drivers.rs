use crate::infrastructure::pxe::{DriverInjectionStatus, NetworkDriverInjectionPlugin, NetworkDriverPackage};
use crate::state::AppState;
use axum::{extract::{Multipart, Path, State}, http::StatusCode, Json};
use serde::Serialize;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

async fn plugin(state: &AppState) -> anyhow::Result<NetworkDriverInjectionPlugin> {
    let root = state.settings.read().await.http.root_dir.clone();
    Ok(NetworkDriverInjectionPlugin::new(PathBuf::from(root)))
}

#[derive(Debug, Serialize)]
pub struct NetworkDriverError { pub error: String }

pub async fn status(State(state): State<AppState>) -> Result<Json<DriverInjectionStatus>, (StatusCode, Json<NetworkDriverError>)> {
    let plugin = plugin(&state).await.map_err(internal_error)?;
    tokio::task::spawn_blocking(move || plugin.status()).await.map_err(|e| internal_message(e.to_string()))?.map(Json).map_err(internal_error)
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<NetworkDriverPackage>>, (StatusCode, Json<NetworkDriverError>)> {
    let plugin = plugin(&state).await.map_err(internal_error)?;
    tokio::task::spawn_blocking(move || plugin.list()).await.map_err(|e| internal_message(e.to_string()))?.map(Json).map_err(internal_error)
}

pub async fn import(State(state): State<AppState>, mut multipart: Multipart) -> Result<Json<NetworkDriverPackage>, (StatusCode, Json<NetworkDriverError>)> {
    let temp_dir = std::env::temp_dir().join("diskless-manager-network-drivers");
    tokio::fs::create_dir_all(&temp_dir).await.map_err(|e| internal_message(e.to_string()))?;
    let mut archive = None;

    while let Some(mut field) = multipart.next_field().await.map_err(|e| bad_request(e.to_string()))? {
        if field.name() != Some("file") { continue; }
        let filename = field.file_name().unwrap_or("driver.zip");
        let safe_name = filename.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect::<String>();
        let path = temp_dir.join(format!("{}-{}", uuid::Uuid::new_v4(), safe_name));
        let mut file = tokio::fs::File::create(&path).await.map_err(|e| internal_message(e.to_string()))?;
        while let Some(chunk) = field.chunk().await.map_err(|e| bad_request(e.to_string()))? {
            file.write_all(&chunk).await.map_err(|e| internal_message(e.to_string()))?;
        }
        file.flush().await.map_err(|e| internal_message(e.to_string()))?;
        archive = Some(path);
        break;
    }

    let archive = archive.ok_or_else(|| bad_request("multipart field 'file' is required".to_string()))?;
    let plugin = plugin(&state).await.map_err(internal_error)?;
    let archive_for_job = archive.clone();
    let result = tokio::task::spawn_blocking(move || plugin.import_zip(&archive_for_job)).await.map_err(|e| internal_message(e.to_string()))?.map_err(internal_error);
    let _ = tokio::fs::remove_file(&archive).await;
    result.map(Json)
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, (StatusCode, Json<NetworkDriverError>)> {
    let plugin = plugin(&state).await.map_err(internal_error)?;
    tokio::task::spawn_blocking(move || plugin.remove(&id)).await.map_err(|e| internal_message(e.to_string()))?.map(|_| StatusCode::NO_CONTENT).map_err(internal_error)
}

pub async fn rebuild(State(state): State<AppState>) -> Result<Json<DriverInjectionStatus>, (StatusCode, Json<NetworkDriverError>)> {
    let plugin = plugin(&state).await.map_err(internal_error)?;
    tokio::task::spawn_blocking(move || plugin.rebuild()).await.map_err(|e| internal_message(e.to_string()))?.map(Json).map_err(internal_error)
}

fn bad_request(message: String) -> (StatusCode, Json<NetworkDriverError>) { (StatusCode::BAD_REQUEST, Json(NetworkDriverError { error: message })) }
fn internal_message(message: String) -> (StatusCode, Json<NetworkDriverError>) { (StatusCode::INTERNAL_SERVER_ERROR, Json(NetworkDriverError { error: message })) }
fn internal_error(error: anyhow::Error) -> (StatusCode, Json<NetworkDriverError>) { log::error!("Network driver operation failed: {error:#}"); internal_message(error.to_string()) }
