use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;

use super::ws::{fetch_metrics, MetricsUpdate};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct DefaultImageResponse {
    pub name: Option<String>,
    pub creation_date: Option<String>,
    pub clones: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientOverviewResponse {
    pub total: i64,
    pub online: i64,
    pub offline: i64,
}

pub async fn get_default_image(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Query the database for the default image
    match sqlx::query_as::<_, (String, String)>(
        "SELECT name, path FROM images WHERE is_default = 1 LIMIT 1",
    )
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some((name, path))) => {
            // Now get ZFS info for this image
            let output = Command::new("zfs")
                .args(["get", "creation,clones", "-o", "value", "-H", &path])
                .output();

            match output {
                Ok(output) if output.status.success() => {
                    let content = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<&str> = content.lines().collect();

                    if lines.len() >= 2 {
                        Ok(Json(json!({
                            "name": name,
                            "creation_date": lines[0],
                            "clones": lines[1]
                        })))
                    } else {
                        Ok(Json(json!({
                            "name": name,
                            "creation_date": null,
                            "clones": null,
                            "message": "Could not retrieve ZFS info"
                        })))
                    }
                }
                _ => Ok(Json(json!({
                    "name": name,
                    "creation_date": null,
                    "clones": null,
                    "message": "ZFS dataset not accessible"
                }))),
            }
        }
        Ok(None) => Ok(Json(json!({
            "name": null,
            "creation_date": null,
            "clones": null,
            "message": "No default image set"
        }))),
        Err(_) => Ok(Json(json!({
            "name": null,
            "creation_date": null,
            "clones": null,
            "message": "Database error"
        }))),
    }
}

pub async fn get_client_overview(
    State(state): State<AppState>,
) -> Result<Json<ClientOverviewResponse>, StatusCode> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clients")
        .fetch_one(&state.db_pool)
        .await
        .unwrap_or((0,));

    let online: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clients WHERE status = 'Online'")
        .fetch_one(&state.db_pool)
        .await
        .unwrap_or((0,));

    let offline = total.0 - online.0;

    Ok(Json(ClientOverviewResponse {
        total: total.0,
        online: online.0,
        offline,
    }))
}

pub async fn get_client_io_metrics(
    State(state): State<AppState>,
) -> Result<Json<MetricsUpdate>, StatusCode> {
    fetch_metrics(&state).await.map(Json).map_err(|error| {
        log::error!("failed to collect dashboard metrics: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}
