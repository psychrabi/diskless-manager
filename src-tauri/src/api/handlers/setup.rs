use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;
use std::path::Path;

use crate::{state::AppState, types::Claims};

#[derive(Debug, Serialize)]
pub struct SetupStatus {
    pub completed: bool,
    pub ready: bool,
    pub missing: Vec<&'static str>,
    pub missing_files: Vec<String>,
}

fn setup_status(checks: &[(&'static str, bool)], completed: bool) -> SetupStatus {
    let missing: Vec<_> = checks
        .iter()
        .filter_map(|(name, ready)| (!ready).then_some(*name))
        .collect();
    let ready = missing.is_empty();
    SetupStatus {
        completed,
        ready,
        missing,
        missing_files: Vec::new(),
    }
}

fn completion_marker(path: &Path) -> Result<bool, StatusCode> {
    match std::fs::read_to_string(path) {
        Ok(value) => Ok(value == "1\n"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn persist_completion(path: &Path, mut status: SetupStatus) -> Result<SetupStatus, StatusCode> {
    use std::io::Write;
    if !status.ready {
        return Err(StatusCode::CONFLICT);
    }
    let parent = path.parent().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut marker =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    marker
        .write_all(b"1\n")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    marker
        .as_file()
        .sync_all()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    marker
        .persist(path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    status.completed = true;
    Ok(status)
}

fn required_dependencies_ready(
    dependencies: &[crate::commands::system::DependencyStatus],
    nfs_enabled: bool,
) -> bool {
    !dependencies.is_empty()
        && dependencies
            .iter()
            .filter(|dependency| {
                !matches!(
                    dependency.name.as_str(),
                    "wakeonlan" | "wol" | "freerdp" | "freerdp3-x11" | "iftop"
                ) && (nfs_enabled
                    || !matches!(dependency.name.as_str(), "nfs-utils" | "nfs-kernel-server"))
            })
            .all(|dependency| dependency.installed)
}

fn nonempty_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}

pub(super) fn service_settings(
    settings: &crate::core::config::Settings,
    name: &str,
) -> serde_json::Value {
    let value = serde_json::to_value(settings).unwrap_or(serde_json::Value::Null);
    let mut snapshot = serde_json::json!({"config": value[name]});
    if matches!(name, "dhcp" | "samba" | "http") {
        snapshot["server"] = value["server"].clone();
    }
    snapshot
}

fn receipt_matches(
    receipt: &str,
    settings: &serde_json::Value,
    config: &serde_json::Value,
) -> bool {
    serde_json::from_str::<serde_json::Value>(receipt).is_ok_and(|value| {
        value.get("settings") == Some(settings) && value.get("config") == Some(config)
    })
}

fn config_snapshot(paths: &[std::path::PathBuf]) -> Result<serde_json::Value, StatusCode> {
    let mut files = Vec::new();
    for path in paths {
        let content = crate::infrastructure::command::read_file_with_sudo(path)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .filter(|content| !content.trim().is_empty())
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        files.push(serde_json::json!({"path": path, "content": content}));
    }
    if files.is_empty() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(serde_json::json!(files))
}

fn receipt_paths(name: &str, distro: crate::platform::Distro) -> Vec<std::path::PathBuf> {
    let primary = match name {
        "dhcp" => "/etc/dhcp/dhcpd.conf",
        "tftp" => distro.tftp_defaults_path(),
        "http" => distro.http_config_path(),
        "samba" => "/etc/samba/smb.conf",
        _ => return vec![],
    };
    let mut paths = vec![std::path::PathBuf::from(primary)];
    if name == "tftp" && matches!(distro, crate::platform::Distro::RedHat) {
        paths.push(std::path::PathBuf::from(
            "/etc/systemd/system/tftp.service.d/diskless-manager.conf",
        ));
    }
    paths
}

pub(super) fn service_config_snapshot(name: &str) -> Result<serde_json::Value, StatusCode> {
    config_snapshot(&receipt_paths(name, crate::platform::detect()))
}

fn boot_ready(settings: &crate::core::config::Settings) -> bool {
    let root = Path::new(&settings.tftp.root_dir);
    let files = [
        &settings.dhcp.boot_file_legacy,
        &settings.dhcp.boot_file_uefi32,
        &settings.dhcp.boot_file_uefi64,
        &settings.dhcp.boot_script,
    ];
    if files.iter().any(|file| {
        file.is_empty()
            || Path::new(file)
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || !nonempty_file(&root.join(file))
    }) {
        return false;
    }
    std::fs::read_to_string(root.join(&settings.dhcp.boot_script)).is_ok_and(|script| {
        script.starts_with("#!ipxe")
            && script
                .lines()
                .any(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
    })
}

pub async fn get_setup_status(
    State(state): State<AppState>,
) -> Result<Json<SetupStatus>, StatusCode> {
    Ok(Json(read_setup_status(&state).await?))
}

pub async fn complete_setup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SetupStatus>, StatusCode> {
    if claims.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }
    let _guard = state.client_mutations.lock().await;
    let status = read_setup_status(&state).await?;
    Ok(Json(persist_completion(
        &state.config_path.with_file_name("setup-completed"),
        status,
    )?))
}

async fn read_setup_status(state: &AppState) -> Result<SetupStatus, StatusCode> {
    let settings = state.settings.read().await.clone();
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM app_config")
        .fetch_all(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let pool_name = rows
        .iter()
        .find(|(key, _)| key == "zpool_name")
        .or_else(|| rows.iter().find(|(key, _)| key == "zfsPool"))
        .and_then(|(_, value)| serde_json::from_str::<String>(value).ok())
        .unwrap_or_else(|| "diskless".to_string());
    let authorization =
        tokio::task::spawn_blocking(crate::commands::system::is_privileged_access_configured)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let dependencies = crate::commands::system::check_dependencies()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let storage = if crate::validation::validate_pool_name(&pool_name).is_ok() {
        tokio::process::Command::new("zpool")
            .args(["list", "-H", "-o", "name", &pool_name])
            .output()
            .await
            .is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).trim() == pool_name
            })
    } else {
        false
    };
    let manager = crate::services::ServiceManager::new(settings.clone(), state.db_pool.clone());
    let settings_ready = ["server", "dhcp", "tftp", "http"]
        .iter()
        .all(|required| rows.iter().any(|(key, _)| key == required))
        && settings.validate().is_ok()
        && authorization
        && manager.dhcp.validate_settings().is_ok();
    let mut checks = vec![
        ("authorization", authorization),
        (
            "dependencies",
            required_dependencies_ready(&dependencies, settings.nfs.enabled),
        ),
        ("storage", storage),
        ("settings", settings_ready),
    ];
    for (name, enabled) in [
        ("dhcp", settings.dhcp.enabled),
        ("tftp", settings.tftp.enabled),
        ("http", settings.http.enabled),
        ("samba", settings.samba.enabled),
    ] {
        let applied = service_config_snapshot(name).is_ok_and(|config| {
            rows.iter()
                .find(|(key, _)| key == &format!("setup-service-{name}"))
                .is_some_and(|(_, receipt)| {
                    receipt_matches(receipt, &service_settings(&settings, name), &config)
                })
        });
        let ready = enabled
            && applied
            && manager
                .status(name)
                .await
                .is_ok_and(|status| status.running);
        checks.push((name, ready));
    }
    checks.push(("boot", boot_ready(&settings)));
    let mut status = setup_status(
        &checks,
        completion_marker(&state.config_path.with_file_name("setup-completed"))?,
    );
    status.missing_files = [
        &settings.dhcp.boot_file_legacy,
        &settings.dhcp.boot_file_uefi32,
        &settings.dhcp.boot_file_uefi64,
        &settings.dhcp.boot_script,
    ]
    .iter()
    .map(|name| Path::new(&settings.tftp.root_dir).join(name))
    .filter(|path| !nonempty_file(path))
    .map(|path| path.display().to_string())
    .collect();
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_tftp_config_changes_or_removal_invalidate_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let defaults = dir.path().join("tftpd");
        let effective = dir.path().join("override.conf");
        let paths = [defaults.clone(), effective.clone()];
        std::fs::write(&defaults, "OPTIONS=reviewed").unwrap();
        std::fs::write(&effective, "ExecStart=reviewed").unwrap();
        let settings = service_settings(&crate::core::config::Settings::default(), "tftp");
        let config = config_snapshot(&paths).unwrap();
        let receipt = serde_json::json!({"settings": settings, "config": config}).to_string();
        assert!(receipt_matches(&receipt, &settings, &config));
        std::fs::write(&effective, "ExecStart=changed").unwrap();
        assert!(!receipt_matches(
            &receipt,
            &settings,
            &config_snapshot(&paths).unwrap()
        ));
        std::fs::remove_file(&effective).unwrap();
        assert!(config_snapshot(&paths).is_err());
    }

    #[test]
    fn fedora_tftp_receipt_includes_the_effective_drop_in() {
        assert_eq!(
            receipt_paths("tftp", crate::platform::Distro::RedHat),
            [
                std::path::PathBuf::from("/etc/sysconfig/tftpd"),
                std::path::PathBuf::from(
                    "/etc/systemd/system/tftp.service.d/diskless-manager.conf"
                )
            ]
        );
    }

    #[test]
    fn http_receipt_tracks_the_server_hostname_consumed_by_apache() {
        let mut settings = crate::core::config::Settings::default();
        let before = service_settings(&settings, "http");
        settings.server.hostname = "new-hostname".into();
        assert_ne!(before, service_settings(&settings, "http"));
    }

    #[test]
    fn receipt_rejects_unreviewed_config_or_changed_settings() {
        let settings = service_settings(&crate::core::config::Settings::default(), "dhcp");
        let receipt = serde_json::json!({"settings": settings, "config": "applied"}).to_string();
        assert!(receipt_matches(
            &receipt,
            &settings,
            &serde_json::json!("applied")
        ));
        assert!(!receipt_matches(
            &receipt,
            &settings,
            &serde_json::json!("stock")
        ));
        assert!(!receipt_matches(
            &receipt,
            &serde_json::json!({"changed": true}),
            &serde_json::json!("applied")
        ));
        assert!(!receipt_matches(
            "null",
            &settings,
            &serde_json::json!("applied")
        ));
    }

    #[test]
    fn optional_tools_and_disabled_nfs_do_not_block_setup() {
        let dependencies = [
            crate::commands::system::DependencyStatus {
                name: "samba".into(),
                installed: true,
                version: None,
            },
            crate::commands::system::DependencyStatus {
                name: "iftop".into(),
                installed: false,
                version: None,
            },
            crate::commands::system::DependencyStatus {
                name: "nfs-utils".into(),
                installed: false,
                version: None,
            },
        ];
        assert!(required_dependencies_ready(&dependencies, false));
        assert!(!required_dependencies_ready(&dependencies, true));
        assert!(!required_dependencies_ready(&[], false));
    }

    #[test]
    fn boot_requires_a_real_script_and_every_selected_bootloader() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = crate::core::config::Settings::default();
        settings.tftp.root_dir = dir.path().to_string_lossy().into_owned();
        for name in ["undionly.kpxe", "ipxe.efi", "snponly.efi"] {
            std::fs::write(dir.path().join(name), "binary").unwrap();
        }
        assert!(!boot_ready(&settings));
        std::fs::write(dir.path().join("autoexec.ipxe"), "#!ipxe\n# only comment\n").unwrap();
        assert!(!boot_ready(&settings));
        std::fs::write(dir.path().join("autoexec.ipxe"), "#!ipxe\nshell\n").unwrap();
        assert!(boot_ready(&settings));
        settings.dhcp.boot_script = "../autoexec.ipxe".into();
        assert!(!boot_ready(&settings));
    }

    #[test]
    fn completed_marker_is_not_invalidated_by_later_readiness_changes() {
        let status = setup_status(
            &[("authorization", true), ("storage", false), ("boot", false)],
            true,
        );
        assert_eq!(status.missing, ["storage", "boot"]);
        assert!(!status.ready);
        assert!(status.completed);
        let ready = setup_status(&[("storage", true), ("boot", true)], false);
        assert!(ready.ready);
        assert!(!ready.completed);
    }

    #[test]
    fn completion_persists_only_ready_status_and_survives_a_new_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("setup-completed");
        let incomplete = setup_status(&[("storage", false)], false);
        assert_eq!(
            persist_completion(&path, incomplete).unwrap_err(),
            StatusCode::CONFLICT
        );
        assert!(!path.exists());
        let status = persist_completion(&path, setup_status(&[("storage", true)], false)).unwrap();
        assert!(status.completed);
        assert!(completion_marker(&path).unwrap());
        assert!(setup_status(&[("storage", true)], completion_marker(&path).unwrap()).completed);
    }

    #[test]
    fn malformed_marker_and_failed_writes_cannot_complete_setup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("setup-completed");
        std::fs::write(&path, "true").unwrap();
        assert!(!completion_marker(&path).unwrap());
        let impossible = dir.path().join("missing").join("setup-completed");
        assert_eq!(
            persist_completion(&impossible, setup_status(&[("storage", true)], false)).unwrap_err(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
