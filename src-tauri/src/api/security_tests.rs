use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tower::ServiceExt;

use crate::{auth::authenticate_user, state::AppState};

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
        client_mutations: Arc::new(Mutex::new(())),
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
