use axum::{
    routing::{delete, get, post, put},
    Router,
};

use crate::api::handlers::{
    auth::{check_admin_exists, login, update_admin_password, validate_auth_token},
    clients::{
        create_client, delete_client, get_client, get_client_boot_history, list_clients,
        update_client,
    },
    config::get_config,
    dashboard::{get_client_overview, get_default_image},
    disks::{create_pool, list_disks, pool_exists, rename_disk},
    images::{
        clone_image, create_image, create_snapshot, delete_image, get_image, get_image_info,
        import_image, list_images, list_masters, rename_image, resize_image, update_image,
        verify_image,
    },
    license::get_license_info_handler,
    logs::{clear_logs, get_logs},
    services::{
        configure_service, get_service_config, get_service_status, list_services,
        restart_all_services, restart_service, start_all_services, start_service,
        stop_all_services, stop_service,
    },
    system::{
        apply_network_settings, check_dependencies, clear_cache, detect_server_network,
        get_interface_ip, get_network_interfaces, get_ram_usage, get_server_status, get_settings,
        get_system_info, get_zfs_arcstat, initialize_server, save_settings,
        setup_privileged_access,
    },
    zfs::{create_dataset, delete_dataset, get_zpool_stats, list_datasets, list_zpools},
};
use crate::api::middleware::{cors_layer, require_auth};
use tower::limit::ConcurrencyLimitLayer;
use tower_http::trace::TraceLayer;

pub fn create_app(state: crate::state::AppState) -> Router {
    Router::new()
        // Auth routes (no auth required)
        .route("/api/auth/login", post(login))
        .route("/api/auth/validate", post(validate_auth_token))
        .route("/api/auth/admin/password", put(update_admin_password))
        .route("/api/auth/admin/exists", get(check_admin_exists))
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
        .route("/api/images/import", post(import_image))
        .route("/api/images/{id}/clone", post(clone_image))
        .route("/api/images/{id}/snapshots", post(create_snapshot))
        .route("/api/images/{id}/info", get(get_image_info))
        .route("/api/images/{id}/resize", post(resize_image))
        .route("/api/images/{id}/verify", post(verify_image))
        // Service routes (auth required)
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
        // System routes (auth required)
        .route("/api/system/info", get(get_system_info))
        .route("/api/system/status", get(get_server_status))
        .route("/api/system/initialize", post(initialize_server))
        .route("/api/system/dependencies", get(check_dependencies))
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
        // Disk routes (auth required)
        .route("/api/disks", get(list_disks))
        .route("/api/disks/{name}/rename", put(rename_disk))
        .route("/api/disks/pool", post(create_pool))
        .route("/api/disks/pool/exists", get(pool_exists))
        // ZFS routes (auth required)
        .route("/api/zfs/pools", get(list_zpools))
        .route("/api/zfs/pools/stats", get(get_zpool_stats))
        .route("/api/zfs/datasets", get(list_datasets).post(create_dataset))
        .route("/api/zfs/datasets/{dataset}", delete(delete_dataset))
        // Config routes (auth required)
        .route("/api/config", get(get_config))
        // Dashboard routes (auth required)
        .route("/api/dashboard/default-image", get(get_default_image))
        .route("/api/dashboard/clients", get(get_client_overview))
        // Logs routes (auth required)
        .route("/api/logs", get(get_logs).delete(clear_logs))
        // License routes (auth required)
        .route("/api/license/info", get(get_license_info_handler))
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(ConcurrencyLimitLayer::new(100))
        .layer(axum::middleware::from_fn_with_state(state, require_auth))
        .layer(cors_layer())
}
