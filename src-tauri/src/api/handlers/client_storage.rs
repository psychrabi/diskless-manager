use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    application::client_lifecycle::PendingRecoverySafetyError, state::AppState, types::Claims,
};

#[derive(serde::Serialize)]
pub struct ClientStorageResponse {
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
enum ClientWritebackError {
    #[error("Client not found")]
    MissingClient,
    #[error("Client has no snapshot-backed writeback clone")]
    UnsupportedWriteback,
}

fn status_for(error: &anyhow::Error) -> StatusCode {
    if error.downcast_ref::<PendingRecoverySafetyError>().is_some() {
        StatusCode::CONFLICT
    } else if error
        .downcast_ref::<ClientWritebackError>()
        .is_some_and(|error| matches!(error, ClientWritebackError::MissingClient))
    {
        StatusCode::NOT_FOUND
    } else if error
        .downcast_ref::<ClientWritebackError>()
        .is_some_and(|error| matches!(error, ClientWritebackError::UnsupportedWriteback))
    {
        StatusCode::CONFLICT
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// Delete this client's boot and selected game clones while retaining its configuration.
pub async fn delete_client_writeback(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ClientStorageResponse>, (StatusCode, Json<super::clients::ErrorResponse>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(super::clients::ErrorResponse {
                status: StatusCode::FORBIDDEN.as_u16(),
                error: "Administrator access required".into(),
            }),
        ));
    }
    let task_state = state.clone();
    let task_id = id.clone();
    let result = crate::api::util::spawn_client_locked(
        state.client_mutations.clone(),
        id.clone(),
        None,
        async move {
            let client = task_state
                .application
                .clients
                .get_by_string(&task_id)
                .await?
                .ok_or(ClientWritebackError::MissingClient)?;
            if client
                .snapshot
                .as_deref()
                .map_or(true, |value| value.trim().is_empty())
                || client
                    .writeback
                    .as_deref()
                    .map_or(true, |value| value.trim().is_empty())
            {
                return Err(ClientWritebackError::UnsupportedWriteback.into());
            }
            crate::application::client_lifecycle::delete_writeback_locked(&task_state, &client)
                .await?;
            Ok(client.name)
        },
    )
    .await
    .map_err(anyhow::Error::from)
    .and_then(|result| result);
    match result {
        Ok(Ok(name)) => Ok(Json(ClientStorageResponse {
            message: format!("Writeback storage deleted for {name}"),
        })),
        Ok(Err(error)) => {
            let status = status_for(&error);
            tracing::warn!(client_id = %id, %error, "client writeback deletion failed");
            Err((
                status,
                Json(super::clients::ErrorResponse {
                    status: status.as_u16(),
                    error: error.to_string(),
                }),
            ))
        }
        Err(error) => {
            tracing::error!(client_id = %id, %error, "client writeback deletion task failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::clients::ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: "Client storage deletion failed".into(),
                }),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::status_for;
    use crate::application::client_lifecycle::PendingRecoverySafetyError;
    use axum::http::StatusCode;

    #[test]
    fn unsafe_or_pending_operations_map_to_conflict() {
        for error in [
            PendingRecoverySafetyError::PendingOperation,
            PendingRecoverySafetyError::SharedStorage,
            PendingRecoverySafetyError::ClientConnected,
            PendingRecoverySafetyError::NvmeExported,
        ] {
            assert_eq!(status_for(&anyhow::Error::new(error)), StatusCode::CONFLICT);
        }
    }

    #[test]
    fn typed_client_lookup_and_writeback_errors_keep_their_status() {
        use super::ClientWritebackError;

        assert_eq!(
            status_for(&anyhow::Error::new(ClientWritebackError::MissingClient)),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_for(&anyhow::Error::new(
                ClientWritebackError::UnsupportedWriteback
            )),
            StatusCode::CONFLICT
        );
    }

    async fn fixture_client(state: &crate::state::AppState) -> crate::domain::Client {
        state
            .application
            .clients
            .create(crate::domain::CreateClient {
                installer_media_id: None,
                enabled: None,
                name: "storage-test".into(),
                mac: "02:00:00:00:00:01".into(),
                ip: "192.0.2.1".into(),
                master: "test-image".into(),
                snapshot: Some("test-image@ready".into()),
                block_store: None,
                block_device: None,
                target_iqn: None,
                pxe_mode: Default::default(),
                keep_writeback: true,
                use_game_disk: false,
                game_disks: Vec::new(),
                chap_enabled: true,
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn rejects_regular_users_and_clients_without_writeback() {
        use axum::{
            body::{to_bytes, Body},
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;

        let (state, app, _admin, user) = crate::api::security_tests::setup().await;
        let client = fixture_client(&state).await;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/clients/{}/writeback/delete", client.id))
                    .header("authorization", format!("Bearer {user}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/clients/{}/writeback/delete", client.id))
                    .header("authorization", format!("Bearer {}", _admin))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = serde_json::from_slice::<serde_json::Value>(
            &to_bytes(response.into_body(), 4096).await.unwrap(),
        )
        .unwrap();
        assert!(body["message"].as_str().unwrap().contains("writeback"));
        assert!(body.get("operation_id").is_some());
    }

    #[tokio::test]
    async fn rejects_clients_with_pending_recovery_before_storage_access() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;

        let (state, app, admin, _) = crate::api::security_tests::setup().await;
        let client = fixture_client(&state).await;
        sqlx::query("UPDATE clients SET writeback = 'pool/clients/storage-test/writeback', block_device = '/dev/zvol/pool/clients/storage-test/writeback', block_store = 'storage-test', target_iqn = 'iqn.test:storage-test' WHERE id = ?")
            .bind(client.id.as_str()).execute(&state.db_pool).await.unwrap();
        sqlx::query("INSERT INTO client_offline_resets (client_id, fingerprint, operation) VALUES (?, 'test', '{}')")
            .bind(client.id.as_str()).execute(&state.db_pool).await.unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/clients/{}/writeback/delete", client.id))
                    .header("authorization", format!("Bearer {admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(body["message"]
            .as_str()
            .unwrap()
            .contains("pending storage operation"));
    }
}
