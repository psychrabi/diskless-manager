use crate::{
    domain::ClientId,
    infrastructure::{
        nvmeof::{
            ensure_export, inspect_export, nqn_for_client, remove_export, NvmeOfExportStatus,
        },
        pxe::{nvme_tcp_uri, render_windows_nvmeof_boot},
    },
    persistence::{ClientRepository, SuperSessionRepository},
};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct NvmeOfBootPreparation {
    pub client_id: String,
    pub client_name: String,
    pub nqn: String,
    pub boot_uri: String,
    pub ipxe_script: String,
    pub export: NvmeOfExportStatus,
}

#[derive(Clone)]
pub struct NvmeOfBootService {
    clients: ClientRepository,
    sessions: SuperSessionRepository,
}

impl NvmeOfBootService {
    pub fn new(clients: ClientRepository, sessions: SuperSessionRepository) -> Self {
        Self { clients, sessions }
    }

    /// Prepare an existing client for an experimental Windows NVMe/TCP boot.
    ///
    /// This is intentionally opt-in and does not modify the client's normal
    /// iSCSI provisioning state. The same ZVOL can therefore be tested through
    /// NVMe/TCP and then returned to the existing iSCSI path without a database
    /// migration or destructive reprovisioning step.
    pub async fn prepare(&self, id: &ClientId, server_ip: &str) -> Result<NvmeOfBootPreparation> {
        let client = self
            .clients
            .find_nvmeof_client(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("client not found: {id}"))?;

        if client.enabled == 0 {
            bail!("client is disabled: {}", client.name);
        }

        let session = self.sessions.get_for_client(client.id.as_str()).await?;
        if session.as_ref().is_some_and(|session| {
            session.state != crate::persistence::SuperSessionState::Completed
        }) || (session.is_none() && client.is_super_mode())
        {
            bail!("cannot prepare an ordinary NVMe boot while client '{}' has an active or legacy Super session", client.name);
        }

        let block_device = client
            .block_device
            .as_deref()
            .or(client.block_store.as_deref())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("client '{}' has no provisioned block device", client.name)
            })?;

        let nqn = nqn_for_client(&client.name);
        let export = ensure_export(&nqn, Path::new(block_device))
            .map_err(anyhow::Error::msg)
            .with_context(|| {
                format!("failed to export client '{}' through NVMe/TCP", client.name)
            })?;

        Ok(NvmeOfBootPreparation {
            client_id: client.id.to_string(),
            client_name: client.name,
            boot_uri: nvme_tcp_uri(server_ip, &nqn),
            ipxe_script: render_windows_nvmeof_boot(&nqn),
            nqn,
            export,
        })
    }

    /// Inspect the experimental export for an existing client without changing
    /// either the NVMe target or the normal iSCSI configuration.
    pub async fn inspect(&self, id: &ClientId) -> Result<NvmeOfExportStatus> {
        let client = self
            .clients
            .find_nvmeof_client(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("client not found: {id}"))?;

        inspect_export(&nqn_for_client(&client.name)).map_err(anyhow::Error::msg)
    }

    /// Remove only the experimental NVMe/TCP export. The ZVOL and iSCSI target
    /// are deliberately left intact.
    pub async fn remove(&self, id: &ClientId) -> Result<()> {
        let client = self
            .clients
            .find_nvmeof_client(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("client not found: {id}"))?;

        remove_export(&nqn_for_client(&client.name)).map_err(anyhow::Error::msg)
    }
}
