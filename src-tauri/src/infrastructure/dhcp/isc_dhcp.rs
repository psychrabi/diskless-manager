use anyhow::Result;
use std::{future::Future, pin::Pin};

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
                "isc-dhcp-server.service",
            ])
            .await
            {
                let cleanup = super::update_dhcp_config(&reservation.client_name, "", false).await;
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
