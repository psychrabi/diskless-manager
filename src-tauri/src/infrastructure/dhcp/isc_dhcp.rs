use anyhow::Result;
use std::{future::Future, path::PathBuf, pin::Pin};

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
        client_name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

#[derive(Debug, Default)]
pub struct IscDhcpPublisher;

fn runtime_settings() -> crate::core::config::Settings {
    let config = crate::config::get_config();
    serde_json::from_value::<crate::core::config::Settings>(config.settings)
        .unwrap_or_else(|error| {
            tracing::warn!(
                error = %error,
                "failed to decode cached settings while publishing client iPXE; using defaults"
            );
            crate::core::config::Settings::default()
        })
}

async fn publish_client_ipxe(reservation: &BootReservation) -> Result<PathBuf> {
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

async fn remove_generated_ipxe(path: &PathBuf) {
    if let Some(path) = path.to_str() {
        if let Err(error) = crate::services::run_sudo_command(["rm", "-f", path]).await {
            tracing::warn!(path, error = %error, "failed to rollback generated client iPXE menu");
        }
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
                remove_generated_ipxe(&script_path).await;
                return Err(error);
            }

            if let Err(error) = crate::infrastructure::command::run_command_async([
                "systemctl",
                "restart",
                "isc-dhcp-server.service",
            ])
            .await
            {
                let cleanup = super::update_dhcp_config(&reservation.client_name, "", false).await;
                remove_generated_ipxe(&script_path).await;
                return match cleanup {
                    Ok(()) => Err(anyhow::anyhow!(
                        "failed to reload DHCP after publishing client: {error}"
                    )),
                    Err(cleanup_error) => Err(anyhow::anyhow!(
                        "failed to reload DHCP after publishing client: {error}; reservation cleanup also failed: {cleanup_error}"
                    )),
                };
            }

            Ok(())
        })
    }

    fn remove<'a>(
        &'a self,
        client_name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            super::update_dhcp_config(client_name, "", false)
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
    }
}
