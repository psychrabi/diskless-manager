use axum::{
    body::Body,
    extract::{connect_info::ConnectInfo, State},
    http::{Request, StatusCode},
    Json, Router,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

use crate::{
    audit_logger::{AuditEventFilter, AuditLogger},
    auth::authenticate_user,
    state::AppState,
    types::LoginRequest,
};

pub(crate) async fn setup() -> (AppState, Router, String, String) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let password_hash = bcrypt::hash("StrongPass1", 4).unwrap();
    for (id, role) in [("admin", "admin"), ("operator", "user")] {
        sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, '', '')")
            .bind(id).bind(id).bind(&password_hash).bind(role).execute(&pool).await.unwrap();
    }
    let admin = authenticate_user(&pool, "admin", "StrongPass1")
        .await
        .unwrap()
        .token;
    let user = authenticate_user(&pool, "operator", "StrongPass1")
        .await
        .unwrap()
        .token;
    let state = AppState {
        config: crate::config::RuntimeConfig::default(),
        client_mutations: crate::state::ClientMutationLocks::default(),
        settings: Arc::new(RwLock::new(crate::core::config::Settings::default())),
        db_pool: pool.clone(),
        config_path: std::path::PathBuf::new(),
        client_ips: Arc::new(RwLock::new(Vec::new())),
        metrics_collector: Arc::new(crate::metrics::MetricsCollector::default()),
        ssh_executor: Arc::new(crate::ssh_executor::SshExecutor::new()),
        application: Arc::new(crate::application::ApplicationServices::new(pool)),
    };
    let app = super::routes::create_app(state.clone());
    (state, app, admin, user)
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    token: &str,
    body: serde_json::Value,
) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn assert_revoked(app: &Router, token: &str) {
    assert_eq!(
        request(app, "GET", "/api/clients", token, json!(null)).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            app,
            "POST",
            "/api/auth/validate",
            "",
            json!({"token": token})
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/ws/metrics")
                .header("sec-websocket-protocol", format!("diskless-auth, {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_cannot_write_service_configs_or_destroy_snapshots() {
    let (_, app, admin, user) = setup().await;
    // Unknown resources ensure a regression never invokes a privileged write.
    for path in [
        "/api/services/nonexistent/configure",
        "/api/images/missing/snapshots/missing/rollback",
    ] {
        assert_eq!(
            request(&app, "POST", path, &user, json!({"content": "test"})).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(&app, "POST", path, &admin, json!({"content": "test"})).await,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        request(&app, "GET", "/api/clients", &user, json!(null)).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn authenticated_mutations_are_audited_but_reads_are_not() {
    let (state, app, admin, _) = setup().await;
    let path = "/api/images/missing/snapshots/missing/rollback";
    assert_eq!(
        request(&app, "POST", path, &admin, json!({})).await,
        StatusCode::NOT_FOUND
    );

    let logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let events = logger
        .query_events(&AuditEventFilter::default())
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].actor, "admin");
    assert_eq!(events[0].module, "images");
    assert_eq!(events[0].action, "rollback");
    assert_eq!(events[0].method, "POST");
    assert_eq!(events[0].path, path);
    assert_eq!(events[0].status, "failed");
    assert_eq!(events[0].status_code, 404);

    assert_eq!(
        request(&app, "GET", "/api/clients", &admin, json!(null)).await,
        StatusCode::OK
    );
    assert_eq!(
        logger
            .query_events(&AuditEventFilter::default())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn login_success_and_failure_are_audited_without_credentials() {
    let (state, _, _, _) = setup().await;
    let peer = ConnectInfo(SocketAddr::from(([192, 0, 2, 10], 5000)));

    let success = super::handlers::auth::login(
        State(state.clone()),
        peer,
        Json(LoginRequest {
            username: "admin".into(),
            password: "StrongPass1".into(),
        }),
    )
    .await
    .unwrap();
    assert!(!success.0.token.is_empty());

    let failure = super::handlers::auth::login(
        State(state.clone()),
        ConnectInfo(SocketAddr::from(([192, 0, 2, 10], 5001))),
        Json(LoginRequest {
            username: "admin".into(),
            password: "WrongPass1".into(),
        }),
    )
    .await;
    assert!(failure.is_err());

    let events = AuditLogger::new(Arc::new(state.db_pool.clone()))
        .query_events(&AuditEventFilter {
            module: Some("auth".into()),
            ..AuditEventFilter::default()
        })
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|event| event.action == "login"));
    assert!(events.iter().all(|event| event.actor == "admin"));
    assert_eq!(events[0].ip.as_deref(), Some("192.0.2.10"));
    assert_eq!(events[0].status, "failed");
    assert_eq!(events[1].status, "success");
    assert!(events.iter().all(|event| {
        !event
            .details
            .as_deref()
            .unwrap_or_default()
            .contains("Pass")
    }));
}

#[tokio::test]
async fn deleting_an_account_revokes_http_validation_and_websocket_access() {
    let (_, app, admin, user) = setup().await;
    assert_eq!(
        request(&app, "DELETE", "/api/users/operator", &admin, json!(null)).await,
        StatusCode::OK
    );
    assert_revoked(&app, &user).await;
}

#[tokio::test]
async fn changing_roles_revokes_existing_sessions() {
    let (state, app, admin, _) = setup().await;
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/users/admin",
            &admin,
            json!({"role": "user"})
        )
        .await,
        StatusCode::OK
    );
    assert_revoked(&app, &admin).await;
    let fresh = authenticate_user(&state.db_pool, "admin", "StrongPass1")
        .await
        .unwrap();
    assert_eq!(fresh.user.role, "user");
    assert_eq!(
        request(&app, "GET", "/api/clients", &fresh.token, json!(null)).await,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "DELETE",
            "/api/users/operator",
            &fresh.token,
            json!(null)
        )
        .await,
        StatusCode::FORBIDDEN
    );
    // Restoring the old role must not resurrect its previously issued token.
    sqlx::query("UPDATE users SET role = 'admin' WHERE id = 'admin'")
        .execute(&state.db_pool)
        .await
        .unwrap();
    assert_revoked(&app, &admin).await;
}

#[tokio::test]
async fn password_reset_revokes_existing_sessions_but_allows_new_login() {
    let (state, app, admin, user) = setup().await;
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/users/operator/password",
            &admin,
            json!({"password": "NewStrongPass2"})
        )
        .await,
        StatusCode::OK
    );
    assert_revoked(&app, &user).await;
    let fresh = authenticate_user(&state.db_pool, "operator", "NewStrongPass2")
        .await
        .unwrap();
    assert_eq!(
        request(&app, "GET", "/api/clients", &fresh.token, json!(null)).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn admin_password_change_revokes_its_session() {
    let (_, app, admin, _) = setup().await;
    assert_eq!(
        request(
            &app,
            "PUT",
            "/api/auth/admin/password",
            &admin,
            json!({"old_password": "StrongPass1", "new_password": "NewStrongPass2"})
        )
        .await,
        StatusCode::OK
    );
    assert_revoked(&app, &admin).await;
}

#[tokio::test]
async fn tokens_without_session_version_are_rejected() {
    let (_, app, _, _) = setup().await;
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &json!({
            "sub": "admin", "username": "admin", "role": "admin",
            "iat": chrono::Utc::now().timestamp(), "exp": chrono::Utc::now().timestamp() + 3600
        }),
        &jsonwebtoken::EncodingKey::from_secret(crate::auth::jwt_secret()),
    )
    .unwrap();
    assert_revoked(&app, &token).await;
}

#[tokio::test]
async fn an_open_websocket_closes_after_account_revocation() {
    assert_open_websocket_closes(false).await;
}

#[tokio::test]
async fn an_open_websocket_closes_after_token_expiration() {
    assert_open_websocket_closes(true).await;
}

async fn assert_open_websocket_closes(expire_token: bool) {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    let (state, app, admin, mut user) = setup().await;
    if expire_token {
        let mut claims = crate::auth::validate_token(&state.db_pool, &user)
            .await
            .unwrap();
        claims.exp = chrono::Utc::now().timestamp() + 2;
        user = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(crate::auth::jwt_secret()),
        )
        .unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server_app = app.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            server_app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        socket.write_all(format!(
            "GET /ws/metrics HTTP/1.1\r\nHost: {address}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Protocol: diskless-auth, {user}\r\n\r\n"
        ).as_bytes()).await.unwrap();
        let mut reader = BufReader::new(socket);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.starts_with("HTTP/1.1 101"), "{line}");
        loop {
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            if line == "\r\n" { break; }
        }
        if !expire_token {
            assert_eq!(request(&app, "DELETE", "/api/users/operator", &admin, json!(null)).await, StatusCode::OK);
        }
        // Consume any metrics already in flight and require a server Close frame.
        loop {
            let opcode = reader.read_u8().await.unwrap() & 0x0f;
            let length = reader.read_u8().await.unwrap();
            assert_eq!(length & 0x80, 0, "server frames must be unmasked");
            let length = match length {
                126 => u64::from(reader.read_u16().await.unwrap()),
                127 => reader.read_u64().await.unwrap(),
                value => u64::from(value),
            };
            assert!(length <= 1024 * 1024);
            let mut payload = vec![0; length as usize];
            reader.read_exact(&mut payload).await.unwrap();
            if opcode == 8 { break; }
        }
    }).await;
    server.abort();
    result.expect("revoked WebSocket should close before the timeout");
    assert_revoked(&app, &user).await;
}

#[tokio::test]
async fn audit_sensitive_mutations_require_admin() {
    let (_, app, _, user) = setup().await;
    for (path, body) in [
        ("/api/services/install", json!({"service":"iftop"})),
        (
            "/api/clients/missing/chap/rotate",
            json!({"service":"iftop"}),
        ),
        (
            "/api/ssh/execute-command",
            json!({"host":"192.0.2.1","username":"operator","command":"true"}),
        ),
    ] {
        assert_eq!(
            request(&app, "POST", path, &user, body).await,
            StatusCode::FORBIDDEN
        );
    }
}

#[tokio::test]
async fn installer_media_and_super_mutations_are_authenticated_and_admin_only() {
    let (_, app, _, user) = setup().await;
    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/installer-media")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(unauthorized.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    for key in ["code", "message", "operation_id", "details"] {
        assert!(body.get(key).is_some());
    }
    for path in [
        "/api/installer-media/upload",
        "/api/installer-media/media-1/default",
        "/api/installer-media/media-1/verification",
        "/api/installer-media/media-1",
        "/api/clients/client-1/super/enter",
        "/api/clients/client-1/super/save",
        "/api/clients/client-1/super/discard",
        "/api/installation-sessions/session-1/cancel",
        "/api/installation-sessions/session-1/close-recovery",
        "/api/clients/client-1/identity/capture",
        "/api/clients/client-1/identity/apply",
    ] {
        assert_eq!(
            request(&app, "POST", path, &user, json!({})).await,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    assert_eq!(
        request(&app, "GET", "/api/installer-media", &user, json!(null)).await,
        StatusCode::FORBIDDEN
    );
    // A well-formed preparation request must still be refused before any storage work.
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/clients/client-1/installation-sessions",
            &user,
            json!({"recipe":"interactive","confirm_erase":true})
        )
        .await,
        StatusCode::FORBIDDEN
    );
    for path in [
        "/api/clients/client-1/super/status",
        "/api/clients/client-1/installation-sessions",
        "/api/clients/client-1/identity",
    ] {
        assert_eq!(
            request(&app, "GET", path, &user, json!(null)).await,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    assert_eq!(
        request(
            &app,
            "DELETE",
            "/api/clients/client-1/identity",
            &user,
            json!(null)
        )
        .await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn admin_can_upload_media_without_path_leak_and_unknown_client_verification_is_rejected() {
    let (state, app, admin, _) = setup().await;
    let media_root = tempfile::tempdir().unwrap();
    state.settings.write().await.http.root_dir = media_root.path().to_string_lossy().into_owned();

    fn bootable_iso(label: &[u8]) -> Vec<u8> {
        let mut image = vec![0; 20 * 2048];
        let pvd = &mut image[16 * 2048..17 * 2048];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 1;
        pvd[40..40 + label.len()].copy_from_slice(label);
        pvd[80..84].copy_from_slice(&20u32.to_le_bytes());
        pvd[84..88].copy_from_slice(&20u32.to_be_bytes());
        pvd[128..130].copy_from_slice(&2048u16.to_le_bytes());
        let boot = &mut image[17 * 2048..18 * 2048];
        boot[0] = 0;
        boot[1..6].copy_from_slice(b"CD001");
        boot[6] = 1;
        boot[7..30].copy_from_slice(b"EL TORITO SPECIFICATION");
        boot[71..75].copy_from_slice(&19u32.to_le_bytes());
        let term = &mut image[18 * 2048..19 * 2048];
        term[0] = 255;
        term[1..6].copy_from_slice(b"CD001");
        term[6] = 1;
        let catalog = &mut image[19 * 2048..19 * 2048 + 64];
        catalog[0] = 1;
        catalog[1] = 0;
        catalog[30] = 0x55;
        catalog[31] = 0xaa;
        let sum: u16 = catalog[..32]
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .sum();
        catalog[28..30].copy_from_slice(&0u16.wrapping_sub(sum).to_le_bytes());
        catalog[32] = 0x88;
        catalog[38..40].copy_from_slice(&4u16.to_le_bytes());
        catalog[40..44].copy_from_slice(&18u32.to_le_bytes());
        image
    }

    fn upload_body(boundary: &str, media_type: &str, iso: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"display_name\"\r\n\r\nTest WinPE\r\n").as_bytes());
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"media_type\"\r\n\r\n{media_type}\r\n").as_bytes());
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"installer.iso\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
        body.extend_from_slice(iso);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    async fn upload_request(
        app: &Router,
        admin: &str,
        boundary: &str,
        media_type: &str,
        iso: Vec<u8>,
    ) -> axum::response::Response {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/installer-media/upload")
                    .header("authorization", format!("Bearer {admin}"))
                    .header(
                        "content-type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(upload_body(boundary, media_type, &iso)))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn assert_error_contract(
        response: axum::response::Response,
        status: StatusCode,
        code: &str,
    ) {
        assert_eq!(response.status(), status);
        let body: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["code"], code);
        for key in ["message", "operation_id", "details"] {
            assert!(body.get(key).is_some());
        }
    }

    let boundary = "installer-media-test-boundary";
    let iso = bootable_iso(b"WINPE");
    let uploaded = upload_request(&app, &admin, boundary, "winpe", iso.clone()).await;
    assert_eq!(uploaded.status(), StatusCode::CREATED);
    let uploaded: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(uploaded.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(uploaded.get("managed_path").is_none());
    assert!(!uploaded
        .to_string()
        .contains(media_root.path().to_str().unwrap()));
    let media_id = uploaded["id"].as_str().unwrap();
    let listed = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/installer-media")
                .header("authorization", format!("Bearer {admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(listed.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(listed[0]["id"], media_id);
    assert!(listed[0].get("managed_path").is_none());
    assert_eq!(listed[0]["bios_verified"], false);
    assert_eq!(listed[0]["uefi_verified"], false);

    assert_error_contract(
        upload_request(&app, &admin, boundary, "unknown", iso.clone()).await,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    )
    .await;
    state
        .settings
        .write()
        .await
        .installer_media
        .max_upload_bytes = iso.len() - 1;
    assert_error_contract(
        upload_request(&app, &admin, boundary, "winpe", iso.clone()).await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "policy_violation",
    )
    .await;
    let catalog_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM installer_media")
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
    assert_eq!(catalog_rows, 1, "failed uploads must not be cataloged");

    let verification = json!({"checksum": uploaded["checksum"], "firmware_mode":"uefi", "test_client_id":"unknown-client", "assigned_image_revision":"pool/image@revision", "observed_luns":[0,1], "boot_result":"passed"});
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/installer-media/{media_id}/verification"))
                .header("authorization", format!("Bearer {admin}"))
                .header("content-type", "application/json")
                .body(Body::from(verification.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let error: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap(),
    )
    .unwrap();
    for key in ["code", "message", "operation_id", "details"] {
        assert!(error.get(key).is_some());
    }
    assert_eq!(error["code"], "not_found");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM installer_media_boot_verifications")
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn audit_password_mutations_reject_weak_passwords() {
    let (_, app, admin, _) = setup().await;
    for (method, path, body) in [
        (
            "POST",
            "/api/users",
            json!({"username":"newuser","password":"","role":"user"}),
        ),
        (
            "PUT",
            "/api/users/operator/password",
            json!({"password":"short"}),
        ),
        (
            "PUT",
            "/api/auth/admin/password",
            json!({"old_password":"StrongPass1","new_password":"weak"}),
        ),
    ] {
        assert_eq!(
            request(&app, method, path, &admin, body).await,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn audit_install_rejects_arbitrary_packages() {
    let (_, app, admin, _) = setup().await;
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/services/install",
            &admin,
            json!({"service":"--help"})
        )
        .await,
        StatusCode::BAD_REQUEST
    );
}

fn report_multipart(report: serde_json::Value) -> (String, Vec<u8>) {
    let boundary = "driver-report-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        "Content-Disposition: form-data; name=\"report\"\r\nContent-Type: application/json\r\n\r\n"
            .as_bytes(),
    );
    body.extend_from_slice(report.to_string().as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary.to_string(), body)
}

async fn submit_report(app: &Router, report: serde_json::Value) -> (StatusCode, serde_json::Value) {
    let (boundary, body) = report_multipart(report);
    // oneshot has no socket; provide the peer the handler binds to sessions.
    let peer: axum::extract::ConnectInfo<std::net::SocketAddr> =
        axum::extract::ConnectInfo("192.0.2.99:49152".parse().unwrap());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/pxe/network-drivers/reports")
                .extension(peer)
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(json!(null)),
    )
}

async fn insert_report_client(
    pool: &sqlx::SqlitePool,
    id: &str,
    octet: u8,
    target_iqn: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO clients (id, name, mac, ip, master, target_iqn, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(id)
    .bind(format!("client-{id}"))
    .bind(format!("02:00:00:00:00:{octet:02x}"))
    .bind("192.0.2.10")
    .bind("master")
    .bind(target_iqn)
    .execute(pool)
    .await
    .unwrap();
}

fn valid_report(client_id: &str) -> serde_json::Value {
    json!({
        "client_id": client_id,
        "windows_version": "11 23H2",
        "architecture": "x64",
        "boot_nic_ids": ["PCI\\VEN_8086&DEV_15F3"],
    })
}

async fn report_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM windows_driver_reports")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn driver_report_from_unknown_client_is_rejected_without_mutation() {
    let (state, app, _, _) = setup().await;
    let (status, body) = submit_report(&app, valid_report("ghost")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
    assert_eq!(report_count(&state.db_pool).await, 0);
}

#[tokio::test]
async fn driver_report_without_client_target_is_rejected_without_mutation() {
    let (state, app, _, _) = setup().await;
    insert_report_client(&state.db_pool, "nt1", 1, None).await;
    let (status, _) = submit_report(&app, valid_report("nt1")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(report_count(&state.db_pool).await, 0);
}

#[tokio::test]
async fn driver_report_without_active_session_is_rejected_without_mutation() {
    let (state, app, _, _) = setup().await;
    insert_report_client(
        &state.db_pool,
        "nt2",
        2,
        Some("iqn.2026-01.test:target.nt2"),
    )
    .await;
    // No live configfs session exists in tests, so the peer binding fails
    // closed; neither the report row nor the catalog may change.
    let (status, body) = submit_report(&app, valid_report("nt2")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["message"].as_str().unwrap().contains("session"));
    assert_eq!(report_count(&state.db_pool).await, 0);
}

#[tokio::test]
async fn driver_report_requires_boot_nic_ids() {
    let (state, app, _, _) = setup().await;
    insert_report_client(
        &state.db_pool,
        "nt3",
        3,
        Some("iqn.2026-01.test:target.nt3"),
    )
    .await;
    let (status, _) = submit_report(
        &app,
        json!({
            "client_id": "nt3",
            "windows_version": "11",
            "architecture": "x64",
            "boot_nic_ids": [],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(report_count(&state.db_pool).await, 0);
}

#[tokio::test]
async fn driver_revisions_are_admin_only() {
    let (_, app, admin, user) = setup().await;
    assert_eq!(
        request(
            &app,
            "GET",
            "/api/pxe/windows/revisions?source_image_id=img",
            &user,
            json!(null)
        )
        .await,
        StatusCode::FORBIDDEN
    );
    // Admin sees an empty revision list for an unknown source image.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/pxe/windows/revisions?source_image_id=img")
                .header("authorization", format!("Bearer {admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        json!([])
    );
}

#[tokio::test]
async fn driver_boot_tests_are_admin_only() {
    let (_, app, _, user) = setup().await;
    assert_eq!(
        request(
            &app,
            "GET",
            "/api/pxe/windows/boot-tests?revision_id=img",
            &user,
            json!(null)
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/pxe/windows/boot-tests",
            &user,
            json!({"revision_id":"r","test_client_id":"c","expected_nic_id":"n"})
        )
        .await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn network_driver_catalog_mutations_are_admin_only() {
    let (_, app, _, user) = setup().await;
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/pxe/network-drivers/rebuild",
            &user,
            json!(null)
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            "DELETE",
            "/api/pxe/network-drivers/not-a-driver",
            &user,
            json!(null)
        )
        .await,
        StatusCode::FORBIDDEN
    );
}
