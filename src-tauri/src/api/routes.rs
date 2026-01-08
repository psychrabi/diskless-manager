use axum::{
    routing::{get, post, put},
    Router,
};

use crate::api::handlers::{
    auth::login,
    clients::{
        create_client, delete_client, get_client, get_client_boot_history, list_clients,
        update_client,
    },
    images::{
        create_image, delete_image, get_image, list_images, list_masters, rename_image,
        update_image,
    },
    services::{get_service_status, list_services, restart_service, start_service, stop_service},
    system::{get_server_status, get_system_info},
};
use crate::api::middleware::{cors_layer, require_auth};
use tower::limit::ConcurrencyLimitLayer;
use tower_http::trace::TraceLayer;

pub fn create_app(state: crate::state::AppState) -> Router {
    Router::new()
        // Auth routes (no auth required)
        .route("/api/auth/login", post(login))
        // Client routes (auth required)
        .route("/api/clients", get(list_clients).post(create_client))
        .route(
            "/api/clients/{id}",
            get(get_client).put(update_client).delete(delete_client),
        )
        .route(
            "/api/clients/{id}/boot-history",
            get(get_client_boot_history),
        )
        // Image routes (auth required)
        .route("/api/images", get(list_images).post(create_image))
        .route("/api/masters", get(list_masters))
        .route(
            "/api/images/{id}",
            get(get_image).put(update_image).delete(delete_image),
        )
        .route("/api/images/{id}/rename", put(rename_image))
        // Service routes (auth required)
        .route("/api/services", get(list_services))
        .route("/api/services/{name}/status", get(get_service_status))
        .route("/api/services/{name}/start", post(start_service))
        .route("/api/services/{name}/stop", post(stop_service))
        .route("/api/services/{name}/restart", post(restart_service))
        // System routes (auth required)
        .route("/api/system/info", get(get_system_info))
        .route("/api/system/status", get(get_server_status))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(ConcurrencyLimitLayer::new(100))
        .layer(axum::middleware::from_fn_with_state(state, require_auth))
        .layer(cors_layer())
}
