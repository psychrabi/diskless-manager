use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::types::disk::DatasetOperationResponse;

#[derive(Debug, Serialize, Deserialize)]
pub struct RenameDiskRequest {
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePoolRequest {
    pub name: String,
    pub disk: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolExistsRequest {
    pub pool_name: Option<String>,
}

fn validate_pool_request(request: &CreatePoolRequest) -> Result<(), StatusCode> {
    crate::validation::validate_pool_name(&request.name).map_err(|_| StatusCode::BAD_REQUEST)?;
    if !request
        .name
        .starts_with(|character: char| character.is_ascii_alphabetic())
        || request.disk.is_empty()
        || !request
            .disk
            .starts_with(|character: char| character.is_ascii_alphanumeric())
        || !request.disk.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        || request.disk == "."
        || request.disk == ".."
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

async fn persist_pool_name(pool: &sqlx::SqlitePool, name: &str) -> Result<(), StatusCode> {
    sqlx::query("INSERT INTO app_config (key, value) VALUES ('zpool_name', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(serde_json::to_string(name).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?).execute(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(())
}

pub async fn list_disks(State(_state): State<AppState>) -> Result<Json<Vec<String>>, StatusCode> {
    match crate::disks::list_block_devices() {
        Ok(devices) => Ok(Json(devices)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn rename_disk(
    Path(name): Path<String>,
    State(_state): State<AppState>,
    Json(request): Json<RenameDiskRequest>,
) -> Result<Json<DatasetOperationResponse>, StatusCode> {
    // Validate that the path parameter matches the request
    if name != request.old_name {
        return Err(StatusCode::BAD_REQUEST);
    }

    match crate::disks::rename_zfs_dataset(&request.old_name, &request.new_name) {
        Ok(message) => Ok(Json(DatasetOperationResponse::success(
            &message,
            Some(&request.new_name),
        ))),
        Err(e) => Ok(Json(DatasetOperationResponse::error(&e.to_string()))),
    }
}

pub async fn create_pool(
    State(state): State<AppState>,
    Json(request): Json<CreatePoolRequest>,
) -> Result<Json<DatasetOperationResponse>, StatusCode> {
    validate_pool_request(&request)?;
    let _guard = state.client_mutations.lock().await;
    let output = tokio::process::Command::new("sudo")
        .args([
            "-n",
            "zpool",
            "create",
            &request.name,
            &format!("/dev/{}", request.disk),
        ])
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            persist_pool_name(&state.db_pool, &request.name).await?;
            let mut config = crate::config::get_config();
            config.settings["zpool_name"] = serde_json::json!(request.name);
            crate::config::set_config(&config);
            Ok(Json(DatasetOperationResponse::success(
                &format!("ZFS pool '{}' created successfully", request.name),
                Some(&request.name),
            )))
        }
        Ok(output) => {
            let error_msg = String::from_utf8_lossy(&output.stderr).to_string();
            log::error!("Failed to create ZFS pool: {error_msg}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn pool_exists(State(_state): State<AppState>) -> Result<Json<bool>, StatusCode> {
    use std::process::Command;
    let pool_name = crate::config::get_zpool_name();
    let output = Command::new("zpool")
        .args(["list", "-H", "-o", "name", &pool_name])
        .output();

    match output {
        Ok(output) => {
            if output.status.success() {
                let pools = String::from_utf8_lossy(&output.stdout);
                let has_pool = pools.trim() == pool_name;
                Ok(Json(has_pool))
            } else {
                Ok(Json(false))
            }
        }
        Err(_) => Ok(Json(false)),
    }
}

#[cfg(test)]
mod setup_tests {
    use super::*;

    #[test]
    fn pool_creation_rejects_options_and_path_traversal() {
        for (name, disk) in [
            ("-f", "sda"),
            ("tank", "../sda"),
            ("tank", "-f"),
            ("", "sda"),
            ("tank", ""),
        ] {
            assert_eq!(
                validate_pool_request(&CreatePoolRequest {
                    name: name.into(),
                    disk: disk.into()
                }),
                Err(StatusCode::BAD_REQUEST)
            );
        }
        assert!(validate_pool_request(&CreatePoolRequest {
            name: "tank".into(),
            disk: "nvme0n1".into()
        })
        .is_ok());
    }

    #[tokio::test]
    async fn created_pool_selection_is_persisted_for_setup_and_storage() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE app_config (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        persist_pool_name(&pool, "tank").await.unwrap();
        let value: String =
            sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'zpool_name'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(value, "\"tank\"");
    }
}
