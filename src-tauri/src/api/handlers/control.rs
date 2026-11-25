use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::HashMap;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path as StdPath, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command as RdpCommand};
use tracing::{debug, error, info};

use crate::audit_logger::{
    AuditEventFilter, AuditLogFilter, AuditLogger, ControlOperation, OperationResult,
};
use crate::state::AppState;
use chrono::Utc;
use sqlx::sqlite::SqlitePool;

#[cfg(test)]
mod linux_control_tests {
    use super::*;

    #[tokio::test]
    async fn metadata_wins_and_legacy_distros_resolve_without_windows_fallback() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE images (id TEXT, name TEXT, os_type TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO images VALUES ('custom', 'diskless/image-disk/windows-looking', 'linux')",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            resolve_master_os(&pool, "diskless/image-disk/windows-looking")
                .await
                .unwrap(),
            "linux"
        );
        for name in [
            "fedora-fixed",
            "cachyos",
            "ubuntu-24-04",
            "kubuntu",
            "debian",
            "arch",
            "steamos",
        ] {
            assert_eq!(resolve_master_os(&pool, name).await.unwrap(), "linux");
        }
        assert_eq!(
            resolve_master_os(&pool, "windows11").await.unwrap(),
            "windows"
        );
        assert!(resolve_master_os(&pool, "unknown-image").await.is_err());
    }

    #[test]
    fn linux_power_uses_safe_noninteractive_ssh_and_sudo() {
        for action in ["shutdown", "reboot"] {
            for force in [false, true] {
                let cmd = build_power_command(action, "192.168.1.101", "linux", force, "diskless");
                assert_eq!(cmd.get_program(), "ssh");
                let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
                assert!(args.contains(&"BatchMode=yes"));
                assert!(args.contains(&"StrictHostKeyChecking=yes"));
                assert!(args.contains(&"diskless"));
                let remote = args.last().unwrap();
                assert!(remote.starts_with("sudo -n systemctl --no-block"));
                assert_eq!(remote.contains("--force"), force);
                assert!(remote.ends_with(if action == "shutdown" {
                    "poweroff"
                } else {
                    "reboot"
                }));
            }
        }
        assert_eq!(
            build_power_command("reboot", "192.168.1.102", "windows", false, "diskless")
                .get_program(),
            "net"
        );
    }

    #[test]
    fn ssh_user_validation_rejects_options_and_shell_input() {
        assert_eq!(power_ssh_user(None).unwrap(), "diskless");
        assert_eq!(power_ssh_user(Some("admin-user")).unwrap(), "admin-user");
        for name in ["", "-oProxyCommand=x", "user;id", "user@host", "a\nb"] {
            assert!(power_ssh_user(Some(name)).is_err());
        }
    }

    #[test]
    fn rdp_viewer_prefers_native_wayland_and_only_uses_available_display_types() {
        let wayland = RdpDisplay::Wayland {
            runtime_dir: PathBuf::from("/run/user/1000"),
            display: "wayland-1".into(),
            x11_display: None,
            x11_authority: None,
        };
        assert_eq!(wayland.programs(), ["wlfreerdp3", "wlfreerdp"]);

        let wayland_with_xwayland = RdpDisplay::Wayland {
            runtime_dir: PathBuf::from("/run/user/1000"),
            display: "wayland-1".into(),
            x11_display: Some(":0".into()),
            x11_authority: None,
        };
        assert_eq!(
            wayland_with_xwayland.programs(),
            ["wlfreerdp3", "wlfreerdp", "xfreerdp3", "xfreerdp"]
        );

        let x11 = RdpDisplay::X11 {
            display: ":0".into(),
            x11_authority: None,
        };
        assert_eq!(x11.programs(), ["xfreerdp3", "xfreerdp"]);
    }

    #[test]
    fn x11_authority_prefers_the_user_cookie_then_gdm_then_runtime_files() {
        let runtime_dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        assert_eq!(x11_authority(runtime_dir.path(), Some(home.path())), None);

        let gdm = runtime_dir.path().join("gdm");
        std::fs::create_dir(&gdm).unwrap();
        std::fs::write(gdm.join("Xauthority"), b"cookie").unwrap();
        assert_eq!(
            x11_authority(runtime_dir.path(), Some(home.path())),
            Some(gdm.join("Xauthority"))
        );

        std::fs::write(home.path().join(".Xauthority"), b"cookie").unwrap();
        assert_eq!(
            x11_authority(runtime_dir.path(), Some(home.path())),
            Some(home.path().join(".Xauthority"))
        );

        let fallback_dir = tempfile::tempdir().unwrap();
        std::fs::write(fallback_dir.path().join("xauth_test"), b"cookie").unwrap();
        assert_eq!(
            x11_authority(fallback_dir.path(), None),
            Some(fallback_dir.path().join("xauth_test"))
        );
    }

    #[test]
    fn wayland_socket_selection_prefers_the_requested_socket_and_ignores_files() {
        let runtime_dir = tempfile::tempdir().unwrap();
        let first = runtime_dir.path().join("wayland-0");
        let second = runtime_dir.path().join("wayland-1");
        let _first = std::os::unix::net::UnixListener::bind(&first).unwrap();
        let _second = std::os::unix::net::UnixListener::bind(&second).unwrap();
        std::fs::write(runtime_dir.path().join("wayland-9.lock"), b"").unwrap();
        std::fs::write(runtime_dir.path().join("wayland-file"), b"").unwrap();

        assert_eq!(
            preferred_wayland_socket(runtime_dir.path(), Some("wayland-0")).as_deref(),
            Some("wayland-0")
        );
        let selected = preferred_wayland_socket(runtime_dir.path(), Some("wayland-9")).unwrap();
        assert!(selected == "wayland-0" || selected == "wayland-1");
        assert_eq!(
            preferred_wayland_socket(runtime_dir.path(), None).as_deref(),
            Some(selected.as_str())
        );
    }

    #[test]
    fn viewer_environment_matches_the_detected_display_server() {
        let wayland = RdpDisplay::Wayland {
            runtime_dir: PathBuf::from("/run/user/1000"),
            display: "wayland-1".into(),
            x11_display: Some(":0".into()),
            x11_authority: Some(PathBuf::from("/run/user/1000/xauth_test")),
        };
        assert_eq!(
            wayland.environment(),
            [
                ("XDG_RUNTIME_DIR", "/run/user/1000".into()),
                ("WAYLAND_DISPLAY", "wayland-1".into()),
                ("DISPLAY", ":0".into()),
                ("XAUTHORITY", "/run/user/1000/xauth_test".into()),
            ]
        );

        let x11 = RdpDisplay::X11 {
            display: ":0".into(),
            x11_authority: Some(PathBuf::from("/home/alice/.Xauthority")),
        };
        assert_eq!(
            x11.environment(),
            [
                ("DISPLAY", ":0".into()),
                ("XAUTHORITY", "/home/alice/.Xauthority".into()),
            ]
        );
    }

    #[test]
    fn logind_selection_prefers_active_local_wayland_sessions() {
        let session =
            |id: &str, kind: &str, class: &str, active: bool, remote: bool| LogindSession {
                id: id.into(),
                user: "alice".into(),
                uid: 1000,
                kind: kind.into(),
                display: if kind == "x11" {
                    ":0".into()
                } else {
                    String::new()
                },
                active,
                remote,
                class: class.into(),
                state: if active {
                    "active".into()
                } else {
                    "online".into()
                },
            };
        let selected = select_graphical_session(vec![
            session("1", "tty", "manager", true, false),
            session("2", "x11", "user", true, false),
            session("3", "wayland", "user", true, false),
            session("4", "wayland", "user", true, true),
            session("5", "wayland", "user", false, false),
        ])
        .unwrap();
        assert_eq!(selected.id, "3");
        assert_eq!(
            select_graphical_session(vec![
                session("6", "tty", "user", true, false),
                session("7", "x11", "user", true, false),
            ])
            .unwrap()
            .id,
            "7"
        );
        assert!(select_graphical_session(vec![session("8", "tty", "user", true, false)]).is_none());
    }

    #[test]
    fn logind_properties_keep_empty_display_values() {
        let properties = parse_logind_properties("Id=3\nType=wayland\nDisplay=\nActive=yes\n");
        assert_eq!(properties.get("Type").map(String::as_str), Some("wayland"));
        assert_eq!(properties.get("Display").map(String::as_str), Some(""));
    }

    #[test]
    fn wayland_display_requires_a_socket() {
        let runtime_dir = tempfile::tempdir().unwrap();
        let path = runtime_dir.path().join("wayland-test");
        assert!(!wayland_socket_available(
            StdPath::new("wayland-test"),
            Some(runtime_dir.path())
        ));
        let _display = std::os::unix::net::UnixListener::bind(&path).unwrap();
        assert!(wayland_socket_available(
            StdPath::new("wayland-test"),
            Some(runtime_dir.path())
        ));
        assert!(wayland_socket_available(&path, None));
        assert!(!wayland_socket_available(
            StdPath::new("wayland-test"),
            None
        ));
    }

    #[tokio::test]
    async fn immediate_viewer_exit_is_a_startup_failure() {
        let mut process = RdpCommand::new("sh")
            .args(["-c", "exit 24"])
            .spawn()
            .unwrap();
        let error = check_rdp_startup(&mut process).await.unwrap_err();
        assert!(error.contains("exited during startup"));
        assert!(error.contains("24"));
    }

    #[test]
    fn linux_rdp_negotiates_security_and_keeps_password_out_of_args() {
        let cmd = build_rdp_command("xfreerdp", "192.168.1.101", 3390, "diskless", "linux");
        let args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap())
            .collect();
        assert!(args.contains(&"/from-stdin:force"));
        assert!(args.contains(&"/v:192.168.1.101:3390"));
        assert!(!args
            .iter()
            .any(|a| a.starts_with("/p:") || *a == "/sec:nla"));
        assert!(build_rdp_command(
            "xfreerdp",
            "192.168.1.102",
            3389,
            "Administrator",
            "windows"
        )
        .as_std()
        .get_args()
        .any(|a| a == "/sec:nla"));
    }
}

pub(super) async fn resolve_master_os(pool: &SqlitePool, master: &str) -> Result<String, String> {
    let name = master.split('@').next().unwrap_or(master);
    let metadata: Option<String> =
        sqlx::query_scalar("SELECT os_type FROM images WHERE name = ? OR id = ? LIMIT 1")
            .bind(name)
            .bind(name)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("Failed to read image OS: {e}"))?;
    if let Some(os) = metadata {
        return os
            .parse::<crate::core::image::OsType>()
            .map(|os| os.to_string())
            .map_err(|e| e.to_string());
    }
    let name = name.rsplit('/').next().unwrap_or(name).to_lowercase();
    if name.contains("windows") {
        Ok("windows".into())
    } else if [
        "linux", "fedora", "cachyos", "ubuntu", "debian", "arch", "steamos", "mint", "opensuse",
    ]
    .iter()
    .any(|os| name.contains(os))
    {
        Ok("linux".into())
    } else {
        Err("Image OS is unknown. Set the image OS type to Linux or Windows before using remote controls.".into())
    }
}

fn power_ssh_user(username: Option<&str>) -> Result<String, String> {
    let user = username.unwrap_or("diskless");
    if user.is_empty()
        || user.len() > 64
        || user.starts_with('-')
        || !user
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
    {
        return Err("Invalid Linux SSH username".into());
    }
    Ok(user.into())
}

/// Request to perform a shutdown operation
#[derive(Debug, Deserialize)]
pub struct ShutdownRequest {
    pub force: Option<bool>,
    pub delay_minutes: Option<u32>,
    pub ssh_username: Option<String>,
}

/// Request to perform a reboot operation
#[derive(Debug, Deserialize)]
pub struct RebootRequest {
    pub force: Option<bool>,
    pub delay_minutes: Option<u32>,
    pub ssh_username: Option<String>,
}

/// Request for remote desktop connection
#[derive(Deserialize)]
pub struct RemoteDesktopRequest {
    pub username: Option<String>,
    pub password: Option<String>,
    pub port: Option<u16>,
}

/// Request to cancel a scheduled operation
#[derive(Debug, Deserialize)]
pub struct CancelOperationRequest {
    pub reason: Option<String>,
}

/// Query parameters for audit logs
#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    pub client_id: Option<String>,
    pub operation_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Query parameters for administrative audit events
#[derive(Debug, Deserialize)]
pub struct AuditEventQuery {
    pub actor: Option<String>,
    pub module: Option<String>,
    pub action: Option<String>,
    pub status: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Query parameters for scheduled operations
#[derive(Debug, Deserialize)]
pub struct ScheduledOperationsQuery {
    pub client_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Response for scheduled operations
#[derive(Debug, Serialize)]
pub struct ScheduledOperationsResponse {
    pub operations: Vec<ScheduledOperation>,
    pub total: usize,
}

/// Scheduled operation entry
#[derive(Debug, Serialize)]
pub struct ScheduledOperation {
    pub id: String,
    pub client_id: String,
    pub operation_type: String,
    pub operation_mode: String,
    pub scheduled_time: String,
    pub created_at: String,
    pub result: Option<String>,
}

/// Response for control operations
#[derive(Debug, Serialize)]
pub struct ControlOperationResponse {
    pub success: bool,
    pub message: String,
    pub operation_id: Option<String>,
    pub timestamp: String,
}

/// A power operation (shutdown/reboot) delayed until a scheduled time.
#[derive(Clone)]
struct ScheduledPowerOp {
    operation_id: String,
    client_id: String,
    client_name: String,
    client_ip: String,
    operation_type: String,
    master_os: String,
    force: bool,
    ssh_username: String,
}

/// Build the OS-specific command used to shutdown or reboot a client.
pub(super) fn build_power_command(
    operation_type: &str,
    ip: &str,
    master_os: &str,
    force: bool,
    ssh_username: &str,
) -> Command {
    let is_shutdown = operation_type == "shutdown";
    if master_os.contains("linux") {
        let action = format!(
            "sudo -n systemctl --no-block {}{}",
            if force { "--force " } else { "" },
            if is_shutdown { "poweroff" } else { "reboot" }
        );
        let mut cmd = Command::new("ssh");
        cmd.args([
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=1",
            "-l",
            ssh_username,
            "--",
            ip,
        ])
        .arg(action);
        cmd
    } else if is_shutdown {
        let mut cmd = Command::new("net");
        cmd.args(["rpc", "shutdown", "-I", ip, "-U", "diskless%1", "-t", "0"]);
        if force {
            cmd.arg("-f");
        }
        cmd
    } else {
        let mut cmd = Command::new("net");
        cmd.args([
            "rpc",
            "shutdown",
            "-r",
            "-I",
            ip,
            "-U",
            "diskless%1",
            "-t",
            "0",
        ]);
        if force {
            cmd.arg("-f");
        }
        cmd
    }
}

/// Run a previously scheduled power operation once its delay has elapsed.
async fn run_scheduled_power_op(pool: SqlitePool, op: ScheduledPowerOp, delay_secs: u64) {
    tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

    // Bail out if the operation was cancelled while we were waiting.
    let pending = match sqlx::query_scalar::<_, Option<String>>(
        "SELECT result FROM scheduled_operations WHERE id = ?",
    )
    .bind(&op.operation_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(result)) => {
            let result = result.unwrap_or_default();
            result.is_empty() || result == "pending"
        }
        _ => true,
    };
    if !pending {
        debug!(
            "Scheduled operation {} skipped (no longer pending)",
            op.operation_id
        );
        return;
    }

    let os_label = if op.master_os.contains("linux") {
        "Linux"
    } else {
        "Windows"
    };
    let verb = if op.operation_type == "shutdown" {
        "Shutdown"
    } else {
        "Reboot"
    };

    let (success, message) = match crate::api::util::run_command(&mut build_power_command(
        &op.operation_type,
        &op.client_ip,
        &op.master_os,
        op.force,
        &op.ssh_username,
    ))
    .await
    {
        Ok(output) if output.status.success() => (
            true,
            format!(
                "{} command sent to {} ({})",
                verb, op.client_name, op.client_ip
            ),
        ),
        Ok(output) => {
            let msg = format!(
                "Failed to {} {} client ({}): {}",
                op.operation_type,
                os_label,
                op.client_ip,
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", msg);
            (false, msg)
        }
        Err(e) => {
            let msg = format!("Failed to execute SSH: {}", e);
            error!("{}", msg);
            (false, msg)
        }
    };

    let result_column = if success {
        "success".to_string()
    } else {
        format!("failed: {}", message)
    };
    if let Err(e) = sqlx::query("UPDATE scheduled_operations SET result = ? WHERE id = ?")
        .bind(&result_column)
        .bind(&op.operation_id)
        .execute(&pool)
        .await
    {
        error!(
            "Failed to update scheduled operation {}: {}",
            op.operation_id, e
        );
    }

    let audit_logger = AuditLogger::new(Arc::new(pool.clone()));
    let audit_operation = ControlOperation {
        client_id: op.client_id.clone(),
        client_name: op.client_name.clone(),
        client_ip: op.client_ip.clone(),
        os_type: op.master_os.clone(),
        operation_type: op.operation_type.clone(),
        operation_mode: if op.force { "force" } else { "graceful" }.to_string(),
        delay_minutes: Some((delay_secs / 60) as u32),
        timestamp: Utc::now(),
        administrator: "system".to_string(),
        result: if success {
            OperationResult::Success
        } else {
            OperationResult::Failed(message)
        },
    };
    if let Err(e) = audit_logger.log_operation(&audit_operation).await {
        error!("Failed to log scheduled operation: {}", e);
    }
    info!(
        "Scheduled {} operation {} finished: {}",
        op.operation_type, op.operation_id, result_column
    );
}

/// Response for remote desktop operations
#[derive(Debug, Serialize)]
pub struct RemoteDesktopResponse {
    pub success: bool,
    pub protocol_used: String,
    pub message: String,
    pub timestamp: String,
}

/// Response for audit logs
#[derive(Debug, Serialize)]
pub struct AuditLogsResponse {
    pub logs: Vec<crate::audit_logger::AuditLogEntry>,
    pub total: usize,
}

/// Response for administrative audit events
#[derive(Debug, Serialize)]
pub struct AuditEventsResponse {
    pub events: Vec<crate::audit_logger::AuditEvent>,
    pub total: usize,
}

/// Error response
#[derive(Debug)]
pub struct ErrorResponse {
    pub status: u16,
    pub error: String,
    pub details: Option<String>,
}

impl Serialize for ErrorResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        crate::api::error::serialize_api_error(
            self.status,
            &self.error,
            self.details.as_ref().map_or_else(
                || serde_json::json!({}),
                |details| serde_json::json!({ "reason": details }),
            ),
            serializer,
        )
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(self),
        )
            .into_response()
    }
}

/// Minimal transport-facing view used by the legacy control handlers.
/// Client loading itself is delegated to the authoritative application service.
struct ControlClient {
    id: String,
    name: String,
    ip: String,
    master: String,
}

async fn load_control_client(
    state: &AppState,
    client_id: &str,
) -> Result<ControlClient, (StatusCode, Json<ErrorResponse>)> {
    let client = state
        .application
        .clients
        .get_by_string(client_id)
        .await
        .map_err(|e| {
            error!("Failed to get client {}: {}", client_id, e);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: StatusCode::NOT_FOUND.as_u16(),
                    error: format!("Client not found: {}", client_id),
                    details: None,
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    status: StatusCode::NOT_FOUND.as_u16(),
                    error: format!("Client not found: {}", client_id),
                    details: None,
                }),
            )
        })?;

    Ok(ControlClient {
        id: client.id.to_string(),
        name: client.name,
        ip: client.ip.to_string(),
        master: client.master,
    })
}

/// Handle shutdown request for a client
pub async fn shutdown_client(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(request): Json<ShutdownRequest>,
) -> Result<Json<ControlOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let force = request.force.unwrap_or(false);
    let delay_minutes = request.delay_minutes;
    let ssh_username = power_ssh_user(request.ssh_username.as_deref()).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: 400,
                error,
                details: None,
            }),
        )
    })?;

    info!(
        "Shutdown request for client {} (force={}, delay={:?})",
        client_id, force, delay_minutes
    );

    let client = load_control_client(&state, &client_id).await?;

    let ip = &client.ip;
    if ip.is_empty() {
        let error_msg = format!("IP address not found for '{}'", client.name);
        error!("{}", error_msg);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: error_msg,
                details: None,
            }),
        ));
    }

    let master_os = resolve_master_os(&state.db_pool, &client.master)
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: 400,
                    error,
                    details: None,
                }),
            )
        })?;

    if let Some(delay_minutes) = delay_minutes {
        if delay_minutes > 0 {
            let operation_id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now();
            let scheduled_time = now + chrono::Duration::minutes(i64::from(delay_minutes));
            let op_mode = if force { "force" } else { "graceful" };

            sqlx::query(
                "INSERT INTO scheduled_operations (id, client_id, operation_type, operation_mode, scheduled_time, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&operation_id)
            .bind(&client.id)
            .bind("shutdown")
            .bind(op_mode)
            .bind(scheduled_time.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!(
                    "Failed to schedule shutdown operation for {}: {}",
                    client.name, e
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        error: "Failed to schedule shutdown operation".to_string(),
                        details: Some(e.to_string()),
                    }),
                )
            })?;

            let pool = state.db_pool.clone();
            let op = ScheduledPowerOp {
                operation_id: operation_id.clone(),
                client_id: client.id.clone(),
                client_name: client.name.clone(),
                client_ip: client.ip.clone(),
                operation_type: "shutdown".to_string(),
                master_os: master_os.clone(),
                force,
                ssh_username: ssh_username.clone(),
            };
            let delay_secs = u64::from(delay_minutes) * 60;
            tokio::spawn(async move {
                run_scheduled_power_op(pool, op, delay_secs).await;
            });

            let message = format!(
                "Shutdown for {} ({}) scheduled for {}",
                client.name,
                client.ip,
                scheduled_time.format("%Y-%m-%d %H:%M:%S")
            );
            info!("{}", message);
            return Ok(Json(ControlOperationResponse {
                success: true,
                message,
                operation_id: Some(operation_id),
                timestamp: now.to_rfc3339(),
            }));
        }
    }

    let (success, message) = match crate::api::util::run_command(&mut build_power_command(
        "shutdown",
        ip,
        &master_os,
        force,
        &ssh_username,
    ))
    .await
    {
        Ok(output) if output.status.success() => {
            let msg = format!("Shutdown command sent to {} ({})", client.name, ip);
            info!("{}", msg);
            (true, msg)
        }
        Ok(output) => {
            let msg = format!(
                "Failed to shutdown {} client ({}): {}",
                if master_os.contains("linux") {
                    "Linux"
                } else {
                    "Windows"
                },
                ip,
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", msg);
            (false, msg)
        }
        Err(e) => {
            error!("Failed to execute SSH: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: format!("Failed to execute SSH: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // Log the operation
    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let operation = ControlOperation {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        client_ip: client.ip.clone(),
        os_type: master_os.clone(),
        operation_type: "shutdown".to_string(),
        operation_mode: if force { "force" } else { "graceful" }.to_string(),
        delay_minutes,
        timestamp: Utc::now(),
        administrator: "system".to_string(),
        result: if success {
            OperationResult::Success
        } else {
            OperationResult::Failed(message.clone())
        },
    };

    if let Err(e) = audit_logger.log_operation(&operation).await {
        error!("Failed to log shutdown operation: {}", e);
    }

    if !success {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                status: 502,
                error: message,
                details: None,
            }),
        ));
    }
    Ok(Json(ControlOperationResponse {
        success,
        message,
        operation_id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Handle reboot request for a client
pub async fn reboot_client(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(request): Json<RebootRequest>,
) -> Result<Json<ControlOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let force = request.force.unwrap_or(false);
    let delay_minutes = request.delay_minutes;
    let ssh_username = power_ssh_user(request.ssh_username.as_deref()).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: 400,
                error,
                details: None,
            }),
        )
    })?;

    info!(
        "Reboot request for client {} (force={}, delay={:?})",
        client_id, force, delay_minutes
    );

    let client = load_control_client(&state, &client_id).await?;

    let ip = &client.ip;
    if ip.is_empty() {
        let error_msg = format!("IP address not found for '{}'", client.name);
        error!("{}", error_msg);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: error_msg,
                details: None,
            }),
        ));
    }

    let master_os = resolve_master_os(&state.db_pool, &client.master)
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: 400,
                    error,
                    details: None,
                }),
            )
        })?;

    if let Some(delay_minutes) = delay_minutes {
        if delay_minutes > 0 {
            let operation_id = uuid::Uuid::new_v4().to_string();
            let now = Utc::now();
            let scheduled_time = now + chrono::Duration::minutes(i64::from(delay_minutes));
            let op_mode = if force { "force" } else { "graceful" };

            sqlx::query(
                "INSERT INTO scheduled_operations (id, client_id, operation_type, operation_mode, scheduled_time, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&operation_id)
            .bind(&client.id)
            .bind("reboot")
            .bind(op_mode)
            .bind(scheduled_time.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!(
                    "Failed to schedule reboot operation for {}: {}",
                    client.name, e
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        error: "Failed to schedule reboot operation".to_string(),
                        details: Some(e.to_string()),
                    }),
                )
            })?;

            let pool = state.db_pool.clone();
            let op = ScheduledPowerOp {
                operation_id: operation_id.clone(),
                client_id: client.id.clone(),
                client_name: client.name.clone(),
                client_ip: client.ip.clone(),
                operation_type: "reboot".to_string(),
                master_os: master_os.clone(),
                force,
                ssh_username: ssh_username.clone(),
            };
            let delay_secs = u64::from(delay_minutes) * 60;
            tokio::spawn(async move {
                run_scheduled_power_op(pool, op, delay_secs).await;
            });

            let message = format!(
                "Reboot for {} ({}) scheduled for {}",
                client.name,
                client.ip,
                scheduled_time.format("%Y-%m-%d %H:%M:%S")
            );
            info!("{}", message);
            return Ok(Json(ControlOperationResponse {
                success: true,
                message,
                operation_id: Some(operation_id),
                timestamp: now.to_rfc3339(),
            }));
        }
    }

    let (success, message) = match crate::api::util::run_command(&mut build_power_command(
        "reboot",
        ip,
        &master_os,
        force,
        &ssh_username,
    ))
    .await
    {
        Ok(output) if output.status.success() => {
            let msg = format!("Reboot command sent to {} ({})", client.name, ip);
            info!("{}", msg);
            (true, msg)
        }
        Ok(output) => {
            let msg = format!(
                "Failed to reboot {} client ({}): {}",
                if master_os.contains("linux") {
                    "Linux"
                } else {
                    "Windows"
                },
                ip,
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", msg);
            (false, msg)
        }
        Err(e) => {
            error!("Failed to execute SSH: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: format!("Failed to execute SSH: {}", e),
                    details: None,
                }),
            ));
        }
    };

    // Log the operation
    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let operation = ControlOperation {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        client_ip: client.ip.clone(),
        os_type: master_os.clone(),
        operation_type: "reboot".to_string(),
        operation_mode: if force { "force" } else { "graceful" }.to_string(),
        delay_minutes,
        timestamp: Utc::now(),
        administrator: "system".to_string(),
        result: if success {
            OperationResult::Success
        } else {
            OperationResult::Failed(message.clone())
        },
    };

    if let Err(e) = audit_logger.log_operation(&operation).await {
        error!("Failed to log reboot operation: {}", e);
    }

    if !success {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                status: 502,
                error: message,
                details: None,
            }),
        ));
    }
    Ok(Json(ControlOperationResponse {
        success,
        message,
        operation_id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

fn rdp_arguments(ip: &str, port: u16, username: &str, os: &str) -> Vec<String> {
    let endpoint = if ip.contains(':') {
        format!("[{ip}]:{port}")
    } else {
        format!("{ip}:{port}")
    };
    let mut args = vec![
        format!("/v:{endpoint}"),
        format!("/u:{username}"),
        "/d:".into(),
        "/from-stdin:force".into(),
        "/cert:tofu".into(),
        "/w:1920".into(),
        "/h:1080".into(),
        "/dynamic-resolution".into(),
        "/network:lan".into(),
        "/timeout:20000".into(),
    ];
    if os == "windows" {
        args.push("/sec:nla".into());
    }
    args
}

fn configure_rdp_process(cmd: &mut RdpCommand) {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
}

fn build_rdp_command(program: &str, ip: &str, port: u16, username: &str, os: &str) -> RdpCommand {
    let mut cmd = RdpCommand::new(program);
    cmd.args(rdp_arguments(ip, port, username, os));
    configure_rdp_process(&mut cmd);
    cmd
}

/// Detect failures that stop the viewer before it can open a window.
async fn check_rdp_startup(process: &mut Child) -> Result<(), String> {
    match tokio::time::timeout(std::time::Duration::from_secs(1), process.wait()).await {
        Ok(Ok(status)) => Err(format!("FreeRDP exited during startup ({status})")),
        Ok(Err(error)) => Err(format!("Could not check FreeRDP startup: {error}")),
        Err(_) => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RdpDisplay {
    Wayland {
        runtime_dir: PathBuf,
        display: String,
        x11_display: Option<String>,
        x11_authority: Option<PathBuf>,
    },
    X11 {
        display: String,
        x11_authority: Option<PathBuf>,
    },
}

impl RdpDisplay {
    fn label(&self) -> &'static str {
        match self {
            RdpDisplay::Wayland { .. } => "Wayland",
            RdpDisplay::X11 { .. } => "X11",
        }
    }

    fn programs(&self) -> &'static [&'static str] {
        match self {
            RdpDisplay::Wayland {
                x11_display: Some(_),
                ..
            } => &["wlfreerdp3", "wlfreerdp", "xfreerdp3", "xfreerdp"],
            RdpDisplay::Wayland { .. } => &["wlfreerdp3", "wlfreerdp"],
            RdpDisplay::X11 { .. } => &["xfreerdp3", "xfreerdp"],
        }
    }

    fn environment(&self) -> Vec<(&'static str, String)> {
        match self {
            RdpDisplay::Wayland {
                runtime_dir,
                display,
                x11_display,
                x11_authority,
            } => {
                let mut environment = vec![
                    (
                        "XDG_RUNTIME_DIR",
                        runtime_dir.to_string_lossy().into_owned(),
                    ),
                    ("WAYLAND_DISPLAY", display.clone()),
                ];
                if let Some(x11_display) = x11_display {
                    environment.push(("DISPLAY", x11_display.clone()));
                    if let Some(x11_authority) = x11_authority {
                        environment
                            .push(("XAUTHORITY", x11_authority.to_string_lossy().into_owned()));
                    }
                }
                environment
            }
            RdpDisplay::X11 {
                display,
                x11_authority,
            } => {
                let mut environment = vec![("DISPLAY", display.clone())];
                if let Some(x11_authority) = x11_authority {
                    environment.push(("XAUTHORITY", x11_authority.to_string_lossy().into_owned()));
                }
                environment
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RdpSession {
    user: Option<String>,
    uid: u32,
    display: RdpDisplay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogindSession {
    id: String,
    user: String,
    uid: u32,
    kind: String,
    display: String,
    active: bool,
    remote: bool,
    class: String,
    state: String,
}

fn parse_logind_properties(output: &str) -> HashMap<String, String> {
    output
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn logind_session(id: String, properties: &HashMap<String, String>) -> Option<LogindSession> {
    Some(LogindSession {
        id,
        user: properties.get("Name")?.clone(),
        uid: properties.get("User")?.parse().ok()?,
        kind: properties.get("Type")?.clone(),
        display: properties.get("Display").cloned().unwrap_or_default(),
        active: properties.get("Active").is_some_and(|value| value == "yes"),
        remote: properties.get("Remote").is_some_and(|value| value == "yes"),
        class: properties.get("Class")?.clone(),
        state: properties.get("State").cloned().unwrap_or_default(),
    })
}

fn graphical_sessions(sessions: Vec<LogindSession>) -> Vec<LogindSession> {
    let mut sessions: Vec<_> = sessions
        .into_iter()
        .filter(|session| {
            session.active
                && !session.remote
                && session.class == "user"
                && session.state == "active"
                && matches!(session.kind.as_str(), "wayland" | "x11")
        })
        .collect();
    sessions.sort_by_key(|session| session.kind != "wayland");
    sessions
}

#[cfg(test)]
fn select_graphical_session(sessions: Vec<LogindSession>) -> Option<LogindSession> {
    graphical_sessions(sessions).into_iter().next()
}

fn effective_uid() -> Option<u32> {
    std::fs::metadata("/proc/self")
        .ok()
        .map(|metadata| metadata.uid())
}

fn wayland_socket_available(display: &StdPath, runtime_dir: Option<&StdPath>) -> bool {
    let socket = if display.is_absolute() {
        display.to_owned()
    } else if let Some(runtime_dir) = runtime_dir.filter(|path| path.is_absolute()) {
        runtime_dir.join(display)
    } else {
        return false;
    };
    std::fs::metadata(socket).is_ok_and(|metadata| metadata.file_type().is_socket())
}

fn preferred_wayland_socket(runtime_dir: &StdPath, preferred: Option<&str>) -> Option<String> {
    if let Some(preferred) = preferred {
        if wayland_socket_available(StdPath::new(preferred), Some(runtime_dir)) {
            return Some(preferred.to_owned());
        }
    }
    let mut sockets: Vec<(SystemTime, String)> = std::fs::read_dir(runtime_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with("wayland-") || name.ends_with(".lock") {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if !metadata.file_type().is_socket() {
                return None;
            }
            Some((metadata.modified().ok()?, name))
        })
        .collect();
    sockets.sort_by_key(|entry| Reverse(entry.0));
    sockets.into_iter().next().map(|(_, name)| name)
}

fn newest_xauth(runtime_dir: &StdPath) -> Option<PathBuf> {
    let mut files: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(runtime_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with("xauth_") {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((metadata.modified().ok()?, entry.path()))
        })
        .collect();
    files.sort_by_key(|entry| Reverse(entry.0));
    files.into_iter().next().map(|(_, path)| path)
}

fn x11_authority(runtime_dir: &StdPath, home: Option<&StdPath>) -> Option<PathBuf> {
    if let Some(home) = home {
        let path = home.join(".Xauthority");
        if std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0) {
            return Some(path);
        }
    }
    let gdm = runtime_dir.join("gdm").join("Xauthority");
    if gdm.is_file() {
        return Some(gdm);
    }
    newest_xauth(runtime_dir)
}

fn x11_display_from_sockets() -> Option<String> {
    let mut sockets: Vec<(SystemTime, u32)> = std::fs::read_dir("/tmp/.X11-unix")
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let number = name.strip_prefix('X')?.parse::<u32>().ok()?;
            let metadata = entry.metadata().ok()?;
            if !metadata.file_type().is_socket() {
                return None;
            }
            Some((metadata.modified().ok()?, number))
        })
        .collect();
    sockets.sort_by_key(|entry| Reverse(entry.0));
    sockets
        .into_iter()
        .next()
        .map(|(_, number)| format!(":{number}"))
}

fn current_rdp_display() -> Option<RdpDisplay> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let wayland_display = std::env::var_os("WAYLAND_DISPLAY").map(PathBuf::from);
    let x11_display = std::env::var("DISPLAY")
        .ok()
        .filter(|display| !display.is_empty());
    let x11_authority = std::env::var_os("XAUTHORITY").map(PathBuf::from);

    if let Some(wayland_display) = wayland_display.as_deref() {
        if wayland_socket_available(wayland_display, runtime_dir.as_deref()) {
            let runtime_dir = if wayland_display.is_absolute() {
                wayland_display.parent()?.to_path_buf()
            } else {
                runtime_dir?
            };
            let display = wayland_display.file_name()?.to_string_lossy().into_owned();
            return Some(RdpDisplay::Wayland {
                runtime_dir,
                display,
                x11_display,
                x11_authority,
            });
        }
    }

    x11_display.map(|display| RdpDisplay::X11 {
        display,
        x11_authority,
    })
}

async fn logind_graphical_sessions() -> Vec<LogindSession> {
    let output = match RdpCommand::new("loginctl")
        .args(["list-sessions", "--no-legend"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    let ids: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect();
    let mut sessions = Vec::new();
    for id in ids {
        let output = match RdpCommand::new("loginctl")
            .args(["show-session", &id])
            .output()
            .await
        {
            Ok(output) if output.status.success() => output,
            _ => continue,
        };
        let properties = parse_logind_properties(&String::from_utf8_lossy(&output.stdout));
        if let Some(session) = logind_session(id, &properties) {
            sessions.push(session);
        }
    }
    sessions
}

async fn detected_rdp_session() -> Option<RdpSession> {
    if let Some(display) = current_rdp_display() {
        return Some(RdpSession {
            user: None,
            uid: effective_uid().unwrap_or(0),
            display,
        });
    }

    for session in graphical_sessions(logind_graphical_sessions().await) {
        let runtime_dir = PathBuf::from(format!("/run/user/{}", session.uid));
        let home = user_home(session.uid);
        let x11_authority = x11_authority(&runtime_dir, home.as_deref());
        let display = match session.kind.as_str() {
            "wayland" => {
                let Some(display) = preferred_wayland_socket(
                    &runtime_dir,
                    std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
                ) else {
                    continue;
                };
                RdpDisplay::Wayland {
                    runtime_dir,
                    display,
                    x11_display: x11_display_from_sockets(),
                    x11_authority,
                }
            }
            "x11" => {
                let display = if session.display.is_empty() {
                    let Some(display) = x11_display_from_sockets() else {
                        continue;
                    };
                    display
                } else {
                    session.display.clone()
                };
                RdpDisplay::X11 {
                    display,
                    x11_authority,
                }
            }
            _ => continue,
        };
        info!(
            session = %session.id,
            kind = %session.kind,
            user = %session.user,
            "Using desktop session for FreeRDP"
        );
        return Some(RdpSession {
            user: Some(session.user),
            uid: session.uid,
            display,
        });
    }
    None
}

fn user_home(uid: u32) -> Option<PathBuf> {
    std::fs::read_to_string("/etc/passwd")
        .ok()?
        .lines()
        .find_map(|line| {
            let mut fields = line.split(':');
            let _name = fields.next()?;
            let _password = fields.next()?;
            let id = fields.next()?.parse::<u32>().ok()?;
            let _gid = fields.next()?;
            let _gecos = fields.next()?;
            let home = fields.next()?;
            (id == uid).then(|| PathBuf::from(home))
        })
}

fn program_available(program: &str) -> bool {
    let mut directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    directories
        .extend(["/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from));
    directories
        .iter()
        .any(|directory| directory.join(program).is_file())
}

fn build_rdp_session_command(
    program: &str,
    session: &RdpSession,
    ip: &str,
    port: u16,
    username: &str,
    os: &str,
) -> RdpCommand {
    let run_as_user = match (session.user.as_deref(), effective_uid()) {
        (Some(user), Some(0)) if session.uid != 0 => Some(user),
        _ => None,
    };
    let cmd = if let Some(user) = run_as_user {
        let mut cmd = RdpCommand::new("runuser");
        cmd.arg("-u").arg(user).arg("--").arg("env");
        if let Some(home) = user_home(session.uid) {
            cmd.arg(format!("HOME={}", home.to_string_lossy()));
            cmd.arg(format!(
                "XDG_CONFIG_HOME={}",
                home.join(".config").to_string_lossy()
            ));
        }
        for (key, value) in session.display.environment() {
            cmd.arg(format!("{key}={value}"));
        }
        cmd.arg(program);
        cmd.args(rdp_arguments(ip, port, username, os));
        configure_rdp_process(&mut cmd);
        cmd
    } else {
        let mut cmd = build_rdp_command(program, ip, port, username, os);
        for (key, value) in session.display.environment() {
            cmd.env(key, value);
        }
        cmd
    };
    cmd
}

async fn launch_rdp_viewer(
    ip: &str,
    port: u16,
    username: &str,
    password: &str,
    os: &str,
) -> Result<String, String> {
    let session = detected_rdp_session().await.ok_or_else(|| {
        "Diskless Manager could not find an active Wayland or X11 desktop session on the server. Log in to the server desktop and try again.".to_string()
    })?;
    if session.user.is_some() && !program_available("runuser") {
        return Err(
            "The Diskless Manager service needs runuser to open FreeRDP in the desktop session."
                .into(),
        );
    }
    let programs = session.display.programs();
    for &program in programs {
        if !program_available(program) {
            continue;
        }
        let mut process =
            match build_rdp_session_command(program, &session, ip, port, username, os).spawn() {
                Ok(process) => process,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("Could not start FreeRDP: {error}")),
            };
        let mut stdin = process
            .stdin
            .take()
            .ok_or("FreeRDP password pipe unavailable")?;
        stdin
            .write_all(format!("{password}\n").as_bytes())
            .await
            .map_err(|_| "FreeRDP closed the password pipe during startup")?;
        drop(stdin);

        check_rdp_startup(&mut process).await?;
        tokio::spawn(async move {
            match process.wait().await {
                Ok(status) if !status.success() => error!(%status, "FreeRDP viewer exited"),
                Err(error) => error!(%error, "Failed to wait for FreeRDP viewer"),
                _ => {}
            }
        });
        return Ok(format!("{program} on {}", session.display.label()));
    }
    Err(format!(
        "No FreeRDP viewer for the server's {} session is installed. Tried {}.",
        session.display.label(),
        programs.join(", ")
    ))
}

/// Start the FreeRDP viewer on the Diskless Manager server's desktop.
pub async fn remote_desktop_client(
    State(state): State<AppState>,
    Path(client_id): Path<String>,
    Json(request): Json<RemoteDesktopRequest>,
) -> Result<Json<RemoteDesktopResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Remote desktop request for client {}", client_id);

    let client = load_control_client(&state, &client_id).await?;

    let ip = client.ip.clone();
    if ip.is_empty() {
        let error_msg = format!("IP address not found for '{}'", client.name);
        error!("{}", error_msg);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: error_msg,
                details: None,
            }),
        ));
    }

    let master_os = resolve_master_os(&state.db_pool, &client.master)
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: 400,
                    error,
                    details: None,
                }),
            )
        })?;
    let username = request.username.unwrap_or_else(|| {
        if master_os == "linux" {
            "diskless".into()
        } else {
            "Administrator".into()
        }
    });
    let password = request.password.unwrap_or_default();
    let port = request.port.unwrap_or(3389);
    if username.trim().is_empty()
        || username.len() > 256
        || username.contains(['\r', '\n', '\0'])
        || password.is_empty()
        || password.len() > 1024
        || password.contains(['\r', '\n', '\0'])
        || port == 0
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                status: 400,
                error:
                    "Enter an RDP username and password (password must not contain line breaks)."
                        .into(),
                details: None,
            }),
        ));
    }
    let protocol_used = "RDP".to_string();
    let result = launch_rdp_viewer(&ip, port, &username, &password, &master_os).await;
    let success = result.is_ok();
    let message = match result {
        Ok(program) => format!(
            "FreeRDP ({program}) started on the Diskless Manager server for {} ({}). Check the server desktop for the session.",
            client.name, ip
        ),
        Err(error) => error,
    };

    // Log the operation
    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let operation = ControlOperation {
        client_id: client.id.clone(),
        client_name: client.name.clone(),
        client_ip: client.ip.clone(),
        os_type: master_os.clone(),
        operation_type: "remote".to_string(),
        operation_mode: "interactive".to_string(),
        delay_minutes: None,
        timestamp: Utc::now(),
        administrator: "system".to_string(),
        result: if success {
            OperationResult::Success
        } else {
            OperationResult::Failed(message.clone())
        },
    };

    if let Err(e) = audit_logger.log_operation(&operation).await {
        error!("Failed to log remote desktop operation: {}", e);
    }

    if !success {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ErrorResponse {
                status: 502,
                error: message,
                details: None,
            }),
        ));
    }
    Ok(Json(RemoteDesktopResponse {
        success,
        protocol_used,
        message,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Handle cancel scheduled operation request
pub async fn cancel_operation(
    State(state): State<AppState>,
    Path(operation_id): Path<String>,
    request: Option<Json<CancelOperationRequest>>,
) -> Result<Json<ControlOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Cancel operation request for operation {}", operation_id);

    let reason = request
        .and_then(|Json(req)| req.reason)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let existing = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT result FROM scheduled_operations WHERE id = ? LIMIT 1",
    )
    .bind(&operation_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        error!(
            "Failed to query scheduled operation {}: {}",
            operation_id, e
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: "Failed to query scheduled operation".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let Some((current_result,)) = existing else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                status: StatusCode::NOT_FOUND.as_u16(),
                error: format!("Scheduled operation {} not found", operation_id),
                details: None,
            }),
        ));
    };

    if current_result
        .as_deref()
        .is_some_and(|result| result != "pending")
    {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                status: StatusCode::CONFLICT.as_u16(),

                error: format!(
                    "Scheduled operation {} is already finalized and cannot be cancelled",
                    operation_id
                ),
                details: None,
            }),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let result_value = reason
        .as_ref()
        .map(|r| format!("cancelled: {}", r))
        .unwrap_or_else(|| "cancelled".to_string());

    sqlx::query("UPDATE scheduled_operations SET result = ?, cancelled_at = ? WHERE id = ?")
        .bind(&result_value)
        .bind(&now)
        .bind(&operation_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            error!(
                "Failed to cancel scheduled operation {} in database: {}",
                operation_id, e
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: "Failed to cancel scheduled operation".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;

    Ok(Json(ControlOperationResponse {
        success: true,
        message: format!("Operation {} cancelled", operation_id),
        operation_id: Some(operation_id),
        timestamp: now,
    }))
}

/// Get audit logs with optional filters
pub async fn get_audit_logs(
    State(state): State<AppState>,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<AuditLogsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Audit logs request with filters: {:?}", query);

    let filter = AuditLogFilter {
        client_id: query.client_id,
        operation_type: query.operation_type,
        start_date: query.start_date,
        end_date: query.end_date,
        limit: query.limit,
        offset: query.offset,
    };

    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let logs = audit_logger.query_logs(&filter).await.map_err(|e| {
        error!("Failed to query audit logs: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: "Failed to query audit logs".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let total = audit_logger.count_logs(&filter).await.map_err(|e| {
        error!("Failed to count audit logs: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: "Failed to count audit logs".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    Ok(Json(AuditLogsResponse {
        logs,
        total: usize::try_from(total).unwrap_or(usize::MAX),
    }))
}

/// Get administrative audit events with optional filters.
pub async fn get_audit_events(
    State(state): State<AppState>,
    Query(query): Query<AuditEventQuery>,
) -> Result<Json<AuditEventsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let filter = AuditEventFilter {
        actor: query.actor,
        module: query.module,
        action: query.action,
        status: query.status,
        start_date: query.start_date,
        end_date: query.end_date,
        limit: query.limit,
        offset: query.offset,
    };

    let audit_logger = AuditLogger::new(Arc::new(state.db_pool.clone()));
    let events = audit_logger.query_events(&filter).await.map_err(|e| {
        error!("Failed to query audit events: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: "Failed to query audit events".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    let total = events.len();

    Ok(Json(AuditEventsResponse { events, total }))
}

/// Get scheduled operations with optional filters
pub async fn get_scheduled_operations(
    State(state): State<AppState>,
    Query(query): Query<ScheduledOperationsQuery>,
) -> Result<Json<ScheduledOperationsResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Scheduled operations request with filters: {:?}", query);

    // Query scheduled operations from database
    let sql = "SELECT id, client_id, operation_type, operation_mode, scheduled_time, created_at, result FROM scheduled_operations WHERE (result IS NULL OR result = 'pending') ORDER BY scheduled_time ASC";

    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
        ),
    >(sql)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        error!("Failed to query scheduled operations: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                status: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: "Failed to query scheduled operations".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // Filter results based on query parameters
    let mut operations: Vec<ScheduledOperation> = rows
        .into_iter()
        .map(
            |(
                id,
                client_id,
                operation_type,
                operation_mode,
                scheduled_time,
                created_at,
                result,
            )| {
                ScheduledOperation {
                    id,
                    client_id,
                    operation_type,
                    operation_mode,
                    scheduled_time,
                    created_at,
                    result,
                }
            },
        )
        .collect();

    // Apply client_id filter if provided
    if let Some(client_id) = &query.client_id {
        operations.retain(|op| op.client_id == *client_id);
    }

    // Apply status filter if provided
    if let Some(status) = &query.status {
        operations.retain(|op| op.result.as_ref() == Some(status));
    }

    // Apply limit and offset
    let offset = query.offset.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;

    let total = operations.len();
    operations = operations.into_iter().skip(offset).take(limit).collect();

    Ok(Json(ScheduledOperationsResponse { operations, total }))
}
