#![expect(dead_code, reason = "Old Tauri commands replaced by Axum handlers - no handler for versions yet")]

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub id: String,
    pub base_name: String,
    pub version: String,
    pub image_id: String,
    pub changelog: Option<String>,
    pub is_latest: bool,
    pub is_stable: bool,
    pub created_at: String,
}

pub async fn list_versions(
    state: State<'_, AppState>,
    base_name: String,
) -> Result<Vec<VersionInfo>, String> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<String>,
            bool,
            bool,
            String,
        ),
    >(
        r#"
        SELECT id, base_name, version, image_id, changelog, is_latest, is_stable, created_at
        FROM image_versions
        WHERE base_name = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(&base_name)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| e.to_string())?;

    let versions = rows
        .into_iter()
        .map(
            |(id, base_name, version, image_id, changelog, is_latest, is_stable, created_at)| {
                VersionInfo {
                    id,
                    base_name,
                    version,
                    image_id,
                    changelog,
                    is_latest,
                    is_stable,
                    created_at,
                }
            },
        )
        .collect();

    Ok(versions)
}

pub async fn get_version_history(
    state: State<'_, AppState>,
    base_name: String,
) -> Result<Vec<VersionInfo>, String> {
    list_versions(state, base_name).await
}
