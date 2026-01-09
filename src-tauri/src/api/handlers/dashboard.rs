use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct DefaultImageResponse {
    pub name: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientOverviewResponse {
    pub total: i64,
    pub online: i64,
    pub offline: i64,
}

pub async fn get_default_image(
    State(state): State<AppState>,
) -> Result<Json<DefaultImageResponse>, StatusCode> {
    // Query the database for an image where is_default = 1
    let result: Result<(String, String), _> =
        sqlx::query_as("SELECT id, name FROM images WHERE is_default = 1 LIMIT 1")
            .fetch_one(&state.db_pool)
            .await;

    if let Ok((id, name)) = result {
        return Ok(Json(DefaultImageResponse {
            name: Some(name),
            id: Some(id),
        }));
    }

    Ok(Json(DefaultImageResponse {
        name: None,
        id: None,
    }))
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
