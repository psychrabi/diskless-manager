use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Router,
};
use serde_json::json;
use std::time::Duration;

// API routes configuration
use crate::api::handlers::{
    auth::{
        bootstrap_first_admin, check_admin_exists, login, update_admin_password,
        validate_auth_token,
    },
    clients::{create_client, delete_client, get_client_boot_history, update_client},
    clients_v2::{get_client, list_clients},
    config::get_config,
    control::{
        cancel_operation, get_audit_logs, get_scheduled_operations, reboot_client,
        remote_desktop_client, shutdown_client,
    },
    dashboard::{get_client_io_metrics, get_client_overview, get_default_image},
    dhcp_reconciliation::{inspect_dhcp_reconciliation, repair_dhcp_reconciliation},
    disks::{create_pool, list_disks, pool_exists, rename_disk},
    images::{
        clone_image, create_image, create_snapshot, delete_image, delete_snapshot, get_image,
        get_image_info, get_snapshots, import_image, list_images, list_masters, rename_image,
        resize_image, rollback_snapshot, set_default_image, update_image, verify_image,
    },
    license::{activate_license_handler, get_license_info_handler},
    logs::{clear_logs, get_logs},
    nvmeof::{inspect_nvmeof_boot, prepare_nvmeof_boot, remove_nvmeof_boot},
    reconciliation::{inspect_storage_reconciliation, repair_storage_reconciliation},
    services::{
        configure_service, get_service_config, get_service_status, install_service, list_services,
        restart_all_services, restart_service, start_all_services, start_service,
        stop_all_services, stop_service,
    },
    ssh::{execute_ssh_command, get_windows_system_info, test_ssh_connection},
    system::{
        apply_network_settings, check_dependencies, clear_cache, detect_server_network,
        get_interface_ip, get_network_interfaces, get_ram_usage, get_server_status, get_settings,
        get_system_info, get_zfs_arcstat, initialize_server, save_settings,
        setup_privileged_access,
    },
    system_reconciliation::inspect_system_reconciliation_handler,
    users::{create_user, delete_user, get_user, list_users, update_user, update_user_password},
    ws::ws_metrics_handler,
    zfs::{create_dataset, delete_dataset, get_zpool_stats, list_datasets, list_zpools},
};
use crate::api::middleware::{cors_layer, rate_limit_auth, require_auth, AuthRateLimiter};
use tower::limit::ConcurrencyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

pub fn create_app(state: crate::state::AppState) -> Router {
    let cors = cors_layer();
    let rate_limiter = AuthRateLimiter::new();

    // Periodically purge stale rate-limit entries.
    {
        let limiter = rate_limiter.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                limiter.cleanup().await;
            }
        });
    }

    // Health check checks DB connectivity.
    let health_pool = state.db_pool.clone();
    let public_router = Router::new()
        .route(
            "/health",
            get(move || {
                let pool = health_pool.clone();
                async move {
                    match sqlx::query("SELECT 1").execute(&pool).await {
                        Ok(_) => (StatusCode::OK, "OK").into_response(),
                        Err(_) => (
                            StatusCode::SERVICE_UNAVAILABLE,
                            axum::Json(json!({ "error": "database unavailable" })),
                        )
                            .into_response(),
                    }
                }
            }),
        )
        .route("/api/auth/login", post(login))
        .route("/api/auth/validate", post(validate_auth_token))
        .route("/api/auth/bootstrap", post(bootstrap_first_admin))
        .route("/api/auth/admin/exists", get(check_admin_exists))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(300),
        ))
        .with_state(state.clone());

    let ws_router = Router::new()
        .route("/ws/metrics", axum::routing::get(ws_metrics_handler))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    let api_router = Router::new()
        .route("/api/clients", get(list_clients).post(create_client))
        .route("/api/config", get(get_config))
        .route("/api/license/info", get(get_license_info_handler))
        .route("/api/disks", get(list_disks))
        .route("/api/disks/pool/exists", get(pool_exists))
        .route("/api/system/dependencies", get(check_dependencies))
        .route(
            "/api/clients/{id}",
            get(get_client).put(update_client).delete(delete_client),
        )
        .route(
            "/api/clients/{id}/boot-history",
            get(get_client_boot_history),
        )
        .route(
            "/api/clients/{id}/nvmeof",
            get(inspect_nvmeof_boot).delete(remove_nvmeof_boot),
        )
        .route(
            "/api/clients/{id}/nvmeof/prepare",
            post(prepare_nvmeof_boot),
        )
        .route("/api/clients/{id}/shutdown", post(shutdown_client))
        .route("/api/clients/{id}/reboot", post(reboot_client))
        .route(
            "/api/clients/{id}/remote-desktop",
            post(remote_desktop_client),
        )
        .route(
            "/api/operations/{operation_id}/cancel",
            post(cancel_operation),
        )
        .route("/api/audit-logs", get(get_audit_logs))
        .route("/api/scheduled-operations", get(get_scheduled_operations))
        .route("/api/images", get(list_images).post(create_image))
        .route("/api/masters", get(list_masters))
        .route(
            "/api/images/{id}",
            get(get_image).put(update_image).delete(delete_image),
        )
        .route("/api/images/{id}/rename", put(rename_image))
        .route("/api/images/import", post(import_image))
        .route("/api/images/{id}/clone", post(clone_image))
        .route(
            "/api/images/{id}/snapshots",
            post(create_snapshot).get(get_snapshots),
        )
        .route(
            "/api/images/{id}/snapshots/{snapshot_name}",
            delete(delete_snapshot),
        )
        .route(
            "/api/images/{id}/snapshots/{snapshot_name}/rollback",
            post(rollback_snapshot),
        )
        .route("/api/images/{id}/info", get(get_image_info))
        .route("/api/images/{id}/resize", post(resize_image))
        .route("/api/images/{id}/verify", post(verify_image))
        .route("/api/images/{id}/set-default", post(set_default_image))
        .route("/api/services", get(list_services))
        .route("/api/services/{name}/status", get(get_service_status))
        .route("/api/services/{name}/start", post(start_service))
        .route("/api/services/{name}/stop", post(stop_service))
        .route("/api/services/{name}/restart", post(restart_service))
        .route("/api/services/all/start", post(start_all_services))
        .route("/api/services/all/stop", post(stop_all_services))
        .route("/api/services/all/restart", post(restart_all_services))
        .route("/api/services/{name}/config", get(get_service_config))
        .route("/api/services/{name}/configure", post(configure_service))
        .route("/api/services/install", post(install_service))
        .route("/api/system/info", get(get_system_info))
        .route("/api/system/status", get(get_server_status))
        .route("/api/system/initialize", post(initialize_server))
        .route("/api/system/cache/clear", post(clear_cache))
        .route(
            "/api/system/network/interfaces",
            get(get_network_interfaces),
        )
        .route(
            "/api/system/network/interfaces/{name}/ip",
            get(get_interface_ip),
        )
        .route("/api/system/network/detect", post(detect_server_network))
        .route("/api/system/network/apply", post(apply_network_settings))
        .route("/api/system/settings", get(get_settings).put(save_settings))
        .route(
            "/api/system/privileged-access",
            post(setup_privileged_access),
        )
        .route("/api/system/ram-usage", get(get_ram_usage))
        .route("/api/system/zfs-arcstat", get(get_zfs_arcstat))
        .route(
            "/api/system/reconciliation",
            get(inspect_system_reconciliation_handler),
        )
        .route(
            "/api/system/reconciliation/storage",
            get(inspect_storage_reconciliation),
        )
        .route(
            "/api/system/reconciliation/storage/{id}",
            post(repair_storage_reconciliation),
        )
        .route(
            "/api/system/reconciliation/dhcp",
            get(inspect_dhcp_reconciliation),
        )
        .route(
            "/api/system/reconciliation/dhcp/{id}",
            post(repair_dhcp_reconciliation),
        )
        .route("/api/disks/{name}/rename", put(rename_disk))
        .route("/api/disks/pool", post(create_pool))
        .route("/api/zfs/pools", get(list_zpools))
        .route("/api/zfs/pools/stats", get(get_zpool_stats))
        .route("/api/zfs/datasets", get(list_datasets).post(create_dataset))
        .route("/api/zfs/datasets/{dataset}", delete(delete_dataset))
        .route("/api/dashboard/default-image", get(get_default_image))
        .route("/api/dashboard/clients", get(get_client_overview))
        .route(
            "/api/dashboard/clients/io-metrics",
            get(get_client_io_metrics),
        )
        .route("/api/logs", get(get_logs).delete(clear_logs))
        .route("/api/license/activate", post(activate_license_handler))
        .route("/api/users", get(list_users).post(create_user))
        .route(
            "/api/users/{id}",
            get(get_user).put(update_user).delete(delete_user),
        )
        .route("/api/users/{id}/password", put(update_user_password))
        .route("/api/auth/admin/password", put(update_admin_password))
        .route("/api/ssh/test-connection", post(test_ssh_connection))
        .route("/api/ssh/execute-command", post(execute_ssh_command))
        .route("/api/ssh/system-info", post(get_windows_system_info))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ));

    Router::new()
        .merge(public_router)
        .merge(ws_router)
        .merge(api_router)
        .fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                axum::Json(json!({ "error": "route not found" })),
            )
        })
        .layer(cors)
        .layer(axum::middleware::from_fn_with_state(
            rate_limiter,
            rate_limit_auth,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(ConcurrencyLimitLayer::new(100))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(300),
        ))
        .with_state(state)
}
