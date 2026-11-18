use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::error::api_error;
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

fn validate_pool_request(request: &CreatePoolRequest) -> Result<(), String> {
    crate::validation::validate_pool_name(&request.name)
        .map_err(|error| format!("Invalid pool name: {error}"))?;
    if !request
        .name
        .starts_with(|character: char| character.is_ascii_alphabetic())
    {
        return Err("A pool name must start with a letter".to_string());
    }
    if request.disk.is_empty()
        || !request
            .disk
            .starts_with(|character: char| character.is_ascii_alphanumeric())
        || !request.disk.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        || request.disk == "."
        || request.disk == ".."
    {
        return Err("Select a single block device name, such as sdb or nvme0n1".to_string());
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

/// Create the pool, reporting the real `zpool` failure to the setup wizard.
///
/// The suite runs this from the setup wizard, so a bare 500 would leave the
/// operator without the reason (`device is in use`, a missing device, and so
/// on). Failures use the shared error contract and never include secrets.
pub async fn create_pool(
    State(state): State<AppState>,
    Json(request): Json<CreatePoolRequest>,
) -> Result<Json<DatasetOperationResponse>, (StatusCode, Json<Value>)> {
    validate_pool_request(&request)
        .map_err(|message| api_error(StatusCode::BAD_REQUEST, message, Value::Null))?;
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
            if persist_pool_name(&state.db_pool, &request.name)
                .await
                .is_err()
            {
                log::error!(
                    "Pool '{}' was created but its name was not saved",
                    request.name
                );
                return Err(api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "ZFS pool '{}' was created, but the server could not save it as the active pool. Select it again after restarting the service.",
                        request.name
                    ),
                    Value::Null,
                ));
            }
            let mut config = crate::config::get_config(&state.config);
            config.settings["zpool_name"] = serde_json::json!(request.name);
            crate::config::set_config(&state.config, &config);
            Ok(Json(DatasetOperationResponse::success(
                &format!("ZFS pool '{}' created successfully", request.name),
                Some(&request.name),
            )))
        }
        Ok(output) => {
            let error_msg = zpool_failure_message(&output.stderr);
            log::error!("Failed to create ZFS pool: {error_msg}");
            Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Could not create ZFS pool '{}': {error_msg}", request.name),
                serde_json::json!({ "disk": request.disk }),
            ))
        }
        Err(error) => {
            log::error!("Failed to run zpool create: {error}");
            Err(api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Could not run zpool create: {error}"),
                Value::Null,
            ))
        }
    }
}

/// Collapse `zpool` stderr into one line without its `cannot create` prefix,
/// so the wizard shows the reason rather than a bare 500.
fn zpool_failure_message(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let combined = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let message = combined
        .strip_prefix("cannot create")
        .and_then(|rest| rest.split_once(':').map(|(_, reason)| reason.trim()))
        .filter(|reason| !reason.is_empty())
        .unwrap_or(combined.as_str());
    if message.is_empty() {
        "zpool reported no error message".to_string()
    } else {
        message.to_string()
    }
}

pub async fn pool_exists(State(state): State<AppState>) -> Result<Json<bool>, StatusCode> {
    let pool_name = crate::config::get_zpool_name(&state.config);
    let output = tokio::process::Command::new("zpool")
        .args(["list", "-H", "-o", "name", &pool_name])
        .output()
        .await;

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
            assert!(
                validate_pool_request(&CreatePoolRequest {
                    name: name.into(),
                    disk: disk.into()
                })
                .is_err(),
                "{name:?} / {disk:?} must be rejected"
            );
        }
        assert!(validate_pool_request(&CreatePoolRequest {
            name: "tank".into(),
            disk: "nvme0n1".into()
        })
        .is_ok());
    }

    #[test]
    fn zpool_stderr_becomes_a_readable_single_line_reason() {
        assert_eq!(
            zpool_failure_message(b"cannot create 'tank': device '/dev/sda' is in use\n"),
            "device '/dev/sda' is in use"
        );
        assert_eq!(
            zpool_failure_message(b"invalid vdev specification\nuse '-f' to override\n"),
            "invalid vdev specification use '-f' to override"
        );
        assert_eq!(
            zpool_failure_message(b"   \n"),
            "zpool reported no error message"
        );
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
