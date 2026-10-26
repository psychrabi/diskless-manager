use anyhow::{Context, Result};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};

#[derive(Debug, Clone)]
pub struct BootReservation {
    pub client_name: String,
    pub mac: String,
    pub ip: String,
    pub target_iqn: String,
    pub server_ip: String,
    /// One-way iSCSI CHAP credentials embedded in the boot response. `None`
    /// preserves the legacy open attach.
    pub chap: Option<crate::infrastructure::iscsi::ChapCredentials>,
    /// Durable Super-session choice, resolved by server-side boot readiness.
    pub super_boot: Option<SuperBootReservation>,
    /// True when the client's assigned image is Windows, which is booted
    /// directly instead of behind the interactive iPXE menu.
    pub assigned_windows: bool,
}

#[derive(Debug, Clone)]
pub enum SuperBootReservation {
    Installer {
        media_type: crate::persistence::InstallerMediaType,
        firmware: crate::domain::PxeMode,
    },
    Deny,
}

pub trait BootReservationPublisher: Send + Sync {
    fn publish<'a>(
        &'a self,
        reservation: &'a BootReservation,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    fn remove<'a>(
        &'a self,
        reservation: &'a BootReservation,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

#[derive(Debug, Default)]
pub struct IscDhcpPublisher;

pub(crate) fn render_client_ipxe(reservation: &BootReservation) -> String {
    match reservation.super_boot.as_ref() {
        Some(SuperBootReservation::Installer {
            media_type,
            firmware,
        }) => crate::infrastructure::pxe::render_super_client_script(
            &reservation.client_name,
            &reservation.target_iqn,
            &reservation.server_ip,
            *media_type,
            firmware.clone(),
            reservation.chap.as_ref(),
        ),
        Some(SuperBootReservation::Deny) => crate::infrastructure::pxe::render_super_boot_denied(),
        None => crate::infrastructure::pxe::render_client_script_for_boot(
            &reservation.client_name,
            &reservation.target_iqn,
            80,
            true,
            reservation.chap.as_ref(),
            reservation.assigned_windows,
        ),
    }
}

pub(crate) async fn restore_client_ipxe_menu(
    settings: &crate::core::config::Settings,
    mac: &str,
    script: &str,
) -> Result<()> {
    let root = PathBuf::from(&settings.http.root_dir);
    let relative = crate::infrastructure::pxe::client_mac_script_path(mac);
    anyhow::ensure!(
        crate::infrastructure::pxe::is_managed_script_path(&root, &relative),
        "refusing unsafe generated iPXE path: {relative}"
    );
    let path = root.join(relative);
    write_client_ipxe_script(&path, script).await
}

async fn write_client_ipxe_script(path: &Path, script: &str) -> Result<()> {
    // Image/snapshot updates leave the menu byte-identical; skip the
    // privileged rewrite when the installed file already matches.
    let installed = match tokio::fs::read_to_string(path).await {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            crate::infrastructure::command::read_file_with_sudo(path)
                .map_err(|error| anyhow::anyhow!("cannot read installed iPXE menu: {error}"))?
        }
        Err(error) => return Err(error.into()),
    };
    if installed.as_deref() == Some(script) {
        tracing::debug!(
            path = %path.display(),
            "per-client iPXE menu unchanged; skipping rewrite"
        );
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("generated iPXE path has no parent"))?;
    let parent = parent
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("generated iPXE directory is not valid UTF-8"))?;
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("generated iPXE path is not valid UTF-8"))?;

    let temporary_path = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("client.ipxe"),
        uuid::Uuid::new_v4()
    ));
    let local_write = async {
        tokio::fs::create_dir_all(parent).await?;
        tokio::fs::write(&temporary_path, script).await?;
        tokio::fs::rename(&temporary_path, path).await?;
        Ok::<_, std::io::Error>(())
    }
    .await;
    if local_write.is_err() {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        crate::services::run_sudo_command(["mkdir", "-p", parent]).await?;
        let temporary_path_str = temporary_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("temporary iPXE path is not valid UTF-8"))?;
        if let Err(error) = crate::services::write_with_sudo_tee(temporary_path_str, script).await {
            return match crate::services::run_sudo_command(["rm", "-f", "--", temporary_path_str]).await {
                Ok(()) => Err(error).context("failed to stage generated iPXE menu"),
                Err(cleanup_error) => Err(error).context(format!(
                    "failed to stage generated iPXE menu and remove its temporary file: {cleanup_error:#}"
                )),
            };
        }
        if let Err(error) =
            crate::services::run_sudo_command(["mv", "-f", "--", temporary_path_str, path_str])
                .await
        {
            return match crate::services::run_sudo_command(["rm", "-f", "--", temporary_path_str]).await {
                Ok(()) => Err(error).context("failed to atomically publish generated iPXE menu"),
                Err(cleanup_error) => Err(error).context(format!(
                    "failed to publish generated iPXE menu and remove its temporary file: {cleanup_error:#}"
                )),
            };
        }
    }
    Ok(())
}

async fn remove_generated_ipxe(path: &Path) -> Result<()> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("generated iPXE path is not valid UTF-8"))?;
    crate::services::run_sudo_command(["rm", "-f", "--", path]).await?;
    Ok(())
}

/// Remove a client's custom iPXE menu, usually as part of client deletion.
/// Boot authorization is enforced by the shared dispatcher.
pub(crate) async fn remove_client_ipxe_menu(
    settings: &crate::core::config::Settings,
    mac: &str,
) -> Result<()> {
    let root = PathBuf::from(&settings.http.root_dir);
    let relative = crate::infrastructure::pxe::client_mac_script_path(mac);

    if !crate::infrastructure::pxe::is_managed_script_path(&root, &relative) {
        anyhow::bail!("refusing unsafe generated iPXE path: {relative}");
    }

    let path = root.join(&relative);
    if !path.exists() {
        return Ok(());
    }
    remove_generated_ipxe(&path).await?;
    tracing::info!(mac = %mac, path = %path.display(), "removed per-client iPXE menu");
    Ok(())
}

pub(crate) async fn remove_generated_client_ipxe_menu(
    settings: &crate::core::config::Settings,
    mac: &str,
) -> Result<bool> {
    let root = PathBuf::from(&settings.http.root_dir);
    let relative = crate::infrastructure::pxe::client_mac_script_path(mac);
    anyhow::ensure!(
        crate::infrastructure::pxe::is_managed_script_path(&root, &relative),
        "refusing unsafe generated iPXE path: {relative}"
    );
    let path = root.join(relative);
    let contents = match tokio::fs::read_to_string(&path).await {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            crate::infrastructure::command::read_file_with_sudo(&path)
                .map_err(|error| anyhow::anyhow!("cannot inspect client iPXE menu: {error}"))?
        }
        Err(error) => return Err(error.into()),
    };
    if contents
        .as_deref()
        .is_some_and(crate::infrastructure::pxe::is_manager_generated_menu)
    {
        remove_generated_ipxe(&path).await?;
        tracing::info!(mac = %mac, path = %path.display(), "removed stale manager-generated iPXE menu");
        return Ok(true);
    }
    Ok(false)
}

impl BootReservationPublisher for IscDhcpPublisher {
    fn publish<'a>(
        &'a self,
        reservation: &'a BootReservation,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let entry = super::create_dhcp_entry_for_server(
                &reservation.client_name,
                &reservation.mac,
                &reservation.ip,
                &reservation.target_iqn,
                &reservation.server_ip,
            );
            super::update_dhcp_config(&reservation.client_name, &entry, true)
                .await
                .map_err(anyhow::Error::msg)?;

            if let Err(error) = crate::infrastructure::command::run_command_async([
                "systemctl",
                "restart",
                crate::platform::detect().dhcp_service(),
            ])
            .await
            {
                let cleanup = self.remove(reservation).await;
                return match cleanup {
                    Ok(()) => Err(anyhow::anyhow!(
                        "failed to reload DHCP after publishing client: {error}"
                    )),
                    Err(cleanup_error) => Err(anyhow::anyhow!(
                        "failed to reload DHCP after publishing client: {error}; reservation cleanup also failed: {cleanup_error:#}"
                    )),
                };
            }

            Ok(())
        })
    }

    fn remove<'a>(
        &'a self,
        reservation: &'a BootReservation,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            super::update_dhcp_config(&reservation.client_name, "", false)
                .await
                .map_err(anyhow::Error::msg)?;
            crate::infrastructure::command::run_command_async([
                "systemctl",
                "restart",
                crate::platform::detect().dhcp_service(),
            ])
            .await
            .map_err(anyhow::Error::from)
        })
    }
}
