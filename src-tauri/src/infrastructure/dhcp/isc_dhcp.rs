use anyhow::Result;
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

fn runtime_settings() -> crate::core::config::Settings {
    let config = crate::config::get_config();
    serde_json::from_value::<crate::core::config::Settings>(config.settings).unwrap_or_else(
        |error| {
            tracing::warn!(
                error = %error,
                "failed to decode cached settings while publishing client iPXE; using defaults"
            );
            crate::core::config::Settings::default()
        },
    )
}

/// Generate or replace the MAC-specific iPXE menu for a client.
///
/// This is public within the crate so reconciliation/experimental boot flows
/// can repair menu files for clients that existed before per-client dispatch
/// was introduced.
pub(crate) async fn publish_client_ipxe(reservation: &BootReservation) -> Result<PathBuf> {
    let settings = runtime_settings();
    let root = PathBuf::from(&settings.http.root_dir);
    let relative = crate::infrastructure::pxe::client_mac_script_path(&reservation.mac);

    if !crate::infrastructure::pxe::is_managed_script_path(&root, &relative) {
        anyhow::bail!("refusing unsafe generated iPXE path: {relative}");
    }

    let path = root.join(&relative);
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("generated iPXE path has no parent"))?;
    let parent = parent
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("generated iPXE directory is not valid UTF-8"))?;
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("generated iPXE path is not valid UTF-8"))?;

    crate::services::run_sudo_command(["mkdir", "-p", parent]).await?;

    let script = crate::infrastructure::pxe::render_client_script(
        &reservation.client_name,
        &reservation.target_iqn,
        settings.http.port,
    );
    crate::services::write_with_sudo_tee(path_str, &script).await?;

    tracing::info!(
        client = %reservation.client_name,
        mac = %reservation.mac,
        path = %path.display(),
        "published per-client iPXE menu"
    );

    Ok(path)
}

async fn remove_generated_ipxe(path: &Path) -> Result<()> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("generated iPXE path is not valid UTF-8"))?;
    crate::services::run_sudo_command(["rm", "-f", "--", path]).await?;
    Ok(())
}

async fn cleanup_boot_artifacts(
    script_cleanup: impl Future<Output = Result<()>>,
    dhcp_cleanup: impl Future<Output = Result<()>>,
) -> Result<()> {
    // Both must be attempted: a DHCP failure must not leave a bootable menu,
    // and a file permission error must not keep the reservation live.
    let script = script_cleanup.await;
    let dhcp = dhcp_cleanup.await;
    match (script, dhcp) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error.context("iPXE cleanup failed")),
        (Ok(()), Err(error)) => Err(error.context("DHCP cleanup failed")),
        (Err(script), Err(dhcp)) => Err(anyhow::anyhow!(
            "iPXE cleanup failed: {script:#}; DHCP cleanup failed: {dhcp:#}"
        )),
    }
}

impl BootReservationPublisher for IscDhcpPublisher {
    fn publish<'a>(
        &'a self,
        reservation: &'a BootReservation,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // Publish the MAC-specific menu before making the DHCP reservation
            // live. A client should never be directed to autoexec.ipxe unless
            // its corresponding clients/<mac>.ipxe file already exists.
            let script_path = publish_client_ipxe(reservation).await?;

            let entry = super::create_dhcp_entry_for_server(
                &reservation.client_name,
                &reservation.mac,
                &reservation.ip,
                &reservation.target_iqn,
                &reservation.server_ip,
            );
            if let Err(error) = super::update_dhcp_config(&reservation.client_name, &entry, true)
                .await
                .map_err(anyhow::Error::msg)
            {
                return match remove_generated_ipxe(&script_path).await {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(anyhow::anyhow!(
                        "failed to publish DHCP reservation: {error:#}; iPXE cleanup also failed: {cleanup_error:#}"
                    )),
                };
            }

            if let Err(error) = crate::infrastructure::command::run_command_async([
                "systemctl",
                "restart",
                "isc-dhcp-server.service",
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
            let settings = runtime_settings();
            let root = PathBuf::from(&settings.http.root_dir);
            let script_cleanup = async {
                crate::domain::MacAddress::parse(&reservation.mac)?;
                let relative = crate::infrastructure::pxe::client_mac_script_path(&reservation.mac);
                if !crate::infrastructure::pxe::is_managed_script_path(&root, &relative) {
                    anyhow::bail!("refusing unsafe generated iPXE path: {relative}");
                }
                remove_generated_ipxe(&root.join(relative)).await
            };
            cleanup_boot_artifacts(script_cleanup, async {
                super::update_dhcp_config(&reservation.client_name, "", false)
                    .await
                    .map_err(anyhow::Error::msg)?;
                crate::infrastructure::command::run_command_async([
                    "systemctl",
                    "restart",
                    "isc-dhcp-server.service",
                ])
                .await
                .map_err(anyhow::Error::from)
            })
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rollback_removes_script_even_when_dhcp_cleanup_fails() {
        let root =
            std::env::temp_dir().join(format!("diskless-boot-rollback-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let script = root.join("001122334466.ipxe");
        let other = root.join("001122334455.ipxe");
        std::fs::write(&script, "client menu").unwrap();
        std::fs::write(&other, "other client menu").unwrap();
        let result = cleanup_boot_artifacts(
            async {
                tokio::fs::remove_file(&script)
                    .await
                    .map_err(anyhow::Error::from)
            },
            async { anyhow::bail!("DHCP restart failed") },
        )
        .await;
        let script_exists = script.exists();
        let other_content = std::fs::read_to_string(&other).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            !script_exists,
            "failed provisioning left a boot menu behind"
        );
        assert_eq!(other_content, "other client menu");
        assert!(format!("{:#}", result.unwrap_err()).contains("DHCP restart failed"));
    }

    #[tokio::test]
    async fn rollback_still_removes_reservation_when_script_cleanup_fails() {
        let reservation =
            std::env::temp_dir().join(format!("diskless-dhcp-rollback-{}", uuid::Uuid::new_v4()));
        std::fs::write(&reservation, "host PC001 {}").unwrap();
        let result =
            cleanup_boot_artifacts(async { anyhow::bail!("script permission denied") }, async {
                tokio::fs::remove_file(&reservation)
                    .await
                    .map_err(anyhow::Error::from)
            })
            .await;
        assert!(
            !reservation.exists(),
            "script cleanup failure skipped DHCP cleanup"
        );
        assert!(format!("{:#}", result.unwrap_err()).contains("script permission denied"));
    }

    #[tokio::test]
    async fn rollback_succeeds_when_both_artifacts_are_removed() {
        cleanup_boot_artifacts(async { Ok(()) }, async { Ok(()) })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn rollback_reports_both_cleanup_failures() {
        let error =
            cleanup_boot_artifacts(async { anyhow::bail!("script permission denied") }, async {
                anyhow::bail!("DHCP validation failed")
            })
            .await
            .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("script permission denied"), "{message}");
        assert!(message.contains("DHCP validation failed"), "{message}");
    }
}
