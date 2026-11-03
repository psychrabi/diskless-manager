use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use super::clients::ErrorResponse;
use crate::{
    application::client_personalization as personalization, domain::ClientId,
    infrastructure::pxe::windows_personalization::WindowsPersonalizationPlan, state::AppState,
    types::Claims,
};

#[derive(Debug, Deserialize, Default)]
pub struct PersonalizationRequest {
    #[serde(default)]
    pub computer_name: Option<String>,
    #[serde(default)]
    pub convert_static_ipv4_to_dhcp: bool,
    #[serde(default)]
    pub clear_stale_drive_letters: bool,
}

#[derive(Debug, Serialize)]
pub struct PersonalizationView {
    pub client_id: String,
    /// Whether this client's assigned image is Windows, i.e. whether a plan can
    /// ever be applied. A stored plan on a non-Windows client is refused.
    pub supported: bool,
    pub computer_name: Option<String>,
    pub convert_static_ipv4_to_dhcp: bool,
    pub clear_stale_drive_letters: bool,
    pub updated_at: Option<i64>,
    /// When a plan was last applied to this client's clone.
    pub last_applied_at: Option<i64>,
    /// Why the last attempt failed, if it did. The plan is kept for a retry.
    pub last_error: Option<String>,
}

fn forbidden(message: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            status: StatusCode::FORBIDDEN.as_u16(),
            error: message.to_string(),
        }),
    )
}

fn failure(status: StatusCode, message: String) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            status: status.as_u16(),
            error: message,
        }),
    )
}

/// Resolve the client and whether its assigned image is Windows.
async fn client_support(
    state: &AppState,
    id: &str,
) -> Result<(String, bool), (StatusCode, Json<ErrorResponse>)> {
    let client_id = ClientId::from_string(id.to_owned())
        .map_err(|error| failure(StatusCode::BAD_REQUEST, error.to_string()))?;
    let client = state
        .application
        .clients
        .get(&client_id)
        .await
        .map_err(|error| failure(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, format!("Client not found: {id}")))?;
    let supported = state
        .application
        .images
        .get(&client.master)
        .await
        .map(|image| image.os_type == crate::core::image::OsType::Windows)
        .unwrap_or(false);
    Ok((client_id.to_string(), supported))
}

fn view(
    client_id: &str,
    supported: bool,
    stored: Option<personalization::ClientPersonalization>,
) -> PersonalizationView {
    match stored {
        Some(stored) => PersonalizationView {
            client_id: client_id.to_owned(),
            supported,
            computer_name: stored.plan.computer_name,
            convert_static_ipv4_to_dhcp: stored.plan.convert_static_ipv4_to_dhcp,
            clear_stale_drive_letters: stored.plan.clear_stale_drive_letters,
            updated_at: Some(stored.updated_at),
            last_applied_at: stored.last_applied_at,
            last_error: stored.last_error,
        },
        None => PersonalizationView {
            client_id: client_id.to_owned(),
            supported,
            computer_name: None,
            convert_static_ipv4_to_dhcp: false,
            clear_stale_drive_letters: false,
            updated_at: None,
            last_applied_at: None,
            last_error: None,
        },
    }
}

fn require_admin(claims: &Claims) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if claims.role != "admin" {
        return Err(forbidden("Administrator access required"));
    }
    Ok(())
}

/// GET /api/clients/{id}/windows-personalization
pub async fn get_personalization(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PersonalizationView>, (StatusCode, Json<ErrorResponse>)> {
    let (client_id, supported) = client_support(&state, &id).await?;
    let stored = personalization::load(&state.db_pool, &client_id)
        .await
        .map_err(|error| {
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load personalization plan: {error}"),
            )
        })?;
    Ok(Json(view(&client_id, supported, stored)))
}

/// PUT /api/clients/{id}/windows-personalization
///
/// An empty plan clears the row. Saving a plan never touches storage: the
/// client's next clone or spare is built with it, and existing spares stop
/// matching the client's fingerprint.
pub async fn put_personalization(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(request): Json<PersonalizationRequest>,
) -> Result<Json<PersonalizationView>, (StatusCode, Json<ErrorResponse>)> {
    require_admin(&claims)?;
    let (client_id, supported) = client_support(&state, &id).await?;
    let plan = WindowsPersonalizationPlan {
        computer_name: request
            .computer_name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty()),
        convert_static_ipv4_to_dhcp: request.convert_static_ipv4_to_dhcp,
        clear_stale_drive_letters: request.clear_stale_drive_letters,
    };
    if !plan.is_empty() && !supported {
        return Err(failure(
            StatusCode::CONFLICT,
            "Personalization applies only to clients whose assigned image is Windows".into(),
        ));
    }
    // An empty plan means "inherit the image identity" and clears the row; only
    // a real plan is validated.
    if !plan.is_empty() {
        plan.validate()
            .map_err(|error| failure(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    }

    // A spare already in flight is built for the previous configuration; the
    // mutation lock keeps the plan change from racing its readiness check.
    let _guard = state.client_mutations.lock_client(&client_id).await;
    personalization::save(
        &state.db_pool,
        &client_id,
        &plan,
        chrono::Utc::now().timestamp(),
    )
    .await
    .map_err(|error| {
        failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save personalization plan: {error}"),
        )
    })?;

    tracing::info!(
        client_id = %client_id,
        user = %claims.sub,
        computer_name = plan.computer_name.as_deref().unwrap_or(""),
        dhcp = plan.convert_static_ipv4_to_dhcp,
        clear_drive_letters = plan.clear_stale_drive_letters,
        "saved client Windows personalization plan"
    );

    let stored = personalization::load(&state.db_pool, &client_id)
        .await
        .map_err(|error| {
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load personalization plan: {error}"),
            )
        })?;
    Ok(Json(view(&client_id, supported, stored)))
}

/// DELETE /api/clients/{id}/windows-personalization
pub async fn delete_personalization(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<PersonalizationView>, (StatusCode, Json<ErrorResponse>)> {
    require_admin(&claims)?;
    let (client_id, supported) = client_support(&state, &id).await?;
    let _guard = state.client_mutations.lock_client(&client_id).await;
    personalization::clear(&state.db_pool, &client_id)
        .await
        .map_err(|error| {
            failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to clear personalization plan: {error}"),
            )
        })?;
    tracing::info!(client_id = %client_id, user = %claims.sub, "cleared client Windows personalization plan");
    Ok(Json(view(&client_id, supported, None)))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    async fn seeded() -> (axum::Router, String, String) {
        let (state, app, admin, user) = crate::api::security_tests::setup().await;
        sqlx::query("INSERT INTO images (id, name, kind, os_type, size_gb, path, format, status, created_at, updated_at) VALUES ('win', 'tank/image/windows', 'master', 'windows', 50, '/dev/zvol/tank/image/windows', 'raw', 'ready', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'), ('lin', 'tank/image/linux', 'master', 'linux', 50, '/dev/zvol/tank/image/linux', 'raw', 'ready', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .expect("images should be seeded");
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, snapshot, writeback, enabled, created_at, updated_at) VALUES ('winclient', 'PC-WIN', '02:00:00:00:00:01', '192.0.2.10', 'tank/image/windows', 'tank/image/windows@installed', 'pool/clients/pc-win', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'), ('linclient', 'PC-LIN', '02:00:00:00:00:02', '192.0.2.11', 'tank/image/linux', 'tank/image/linux@installed', 'pool/clients/pc-lin', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&state.db_pool)
            .await
            .expect("clients should be seeded");
        (app, admin, user)
    }

    fn put(token: &str, id: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("PUT")
            .uri(format!("/api/clients/{id}/windows-personalization"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap_or_else(|error| panic!("invalid request: {error}"))
    }

    #[tokio::test]
    async fn plans_are_admin_only_validated_and_scoped_to_windows_clients() {
        let (app, admin, user) = seeded().await;
        let plan = serde_json::json!({
            "computer_name": " pc-lab-07 ",
            "convert_static_ipv4_to_dhcp": true,
            "clear_stale_drive_letters": true
        });

        let response = app
            .clone()
            .oneshot(put(&user, "winclient", plan.clone()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = app
            .clone()
            .oneshot(put(
                &admin,
                "winclient",
                serde_json::json!({"computer_name": "pc_lab"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let response = app
            .clone()
            .oneshot(put(&admin, "linclient", plan.clone()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let response = app
            .clone()
            .oneshot(put(&admin, "winclient", plan))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(body["computer_name"], "PC-LAB-07");
        assert_eq!(body["supported"], true);
        assert_eq!(body["convert_static_ipv4_to_dhcp"], true);
        assert!(body["last_applied_at"].is_null());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/clients/winclient/windows-personalization")
                    .header("authorization", format!("Bearer {admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert_eq!(body["computer_name"], "PC-LAB-07");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/clients/winclient/windows-personalization")
                    .header("authorization", format!("Bearer {admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert!(body["computer_name"].is_null());
        assert!(body["updated_at"].is_null());
    }

    #[tokio::test]
    async fn an_empty_plan_clears_and_unknown_clients_are_not_found() {
        let (app, admin, _user) = seeded().await;
        let response = app
            .clone()
            .oneshot(put(
                &admin,
                "winclient",
                serde_json::json!({"computer_name": "pc-01"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(put(&admin, "winclient", serde_json::json!({})))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert!(body["computer_name"].is_null());
        assert_eq!(body["convert_static_ipv4_to_dhcp"], false);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/clients/missing/windows-personalization")
                    .header("authorization", format!("Bearer {admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
