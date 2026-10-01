use axum::{
    body::{to_bytes, Body, Bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn backup_routes_enforce_admin_access_upload_limit_and_stage_without_mutating_live_data() {
    let (mut state, _, admin, user) = super::security_tests::setup().await;
    let directory = tempfile::tempdir().unwrap();
    state.config_path = directory.path().join("config.json");
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite:{}?mode=rwc",
        directory.path().join("diskless.db").display()
    ))
    .await
    .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    for (id, role) in [("admin", "admin"), ("operator", "user")] {
        sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES (?, ?, 'hash', ?, '', '')")
            .bind(id).bind(id).bind(role).execute(&pool).await.unwrap();
    }
    state.db_pool.close().await;
    state.db_pool = pool.clone();
    state.application = std::sync::Arc::new(crate::application::ApplicationServices::new(pool));
    std::fs::write(
        directory.path().join("jwt-secret"),
        crate::auth::jwt_secret(),
    )
    .unwrap();
    let app = super::routes::create_app(state.clone());

    for (method, path) in [
        ("GET", "/api/system/backup"),
        ("POST", "/api/system/restore"),
        ("POST", "/api/system/setup/complete"),
    ] {
        for (token, expected) in [
            ("", StatusCode::UNAUTHORIZED),
            (&user, StatusCode::FORBIDDEN),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{method} {path}");
        }
    }

    // Restore accepts bodies above the general 16 MiB API limit, but never above 64 MiB.
    for (mebibytes, expected) in [
        (17, StatusCode::BAD_REQUEST),
        (65, StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        let chunk = Bytes::from(vec![b' '; 1024 * 1024]);
        let body = Body::from_stream(futures::stream::iter(
            (0..mebibytes).map(move |_| Ok::<_, std::io::Error>(chunk.clone())),
        ));
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/system/restore")
                    .header("authorization", format!("Bearer {admin}"))
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert!(!directory.path().join("pending-restore.json").exists());
    }

    let download = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/system/backup")
                .header("authorization", format!("Bearer {admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(download.headers()["cache-control"], "no-store");
    let bytes = to_bytes(download.into_body(), 64 * 1024 * 1024)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app_config (key, value) VALUES ('live-only', 'true')")
        .execute(&state.db_pool)
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/system/restore")
                .header("authorization", format!("Bearer {admin}"))
                .header("content-type", "application/json")
                .body(Body::from(bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        std::fs::read(directory.path().join("pending-restore.json")).unwrap(),
        bytes
    );
    let value: String = sqlx::query_scalar("SELECT value FROM app_config WHERE key='live-only'")
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
    assert_eq!(value, "true");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/system/restore")
                .header("authorization", format!("Bearer {admin}"))
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    state.db_pool.close().await;
}
