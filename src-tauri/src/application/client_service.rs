use crate::domain::{Client, ClientId, CreateClient, DomainError, UpdateClient};
use crate::persistence::ClientRepository;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Clone)]
pub struct ClientService {
    repository: ClientRepository,
}

impl ClientService {
    pub fn new(repository: ClientRepository) -> Self {
        Self { repository }
    }

    pub async fn get(&self, id: &ClientId) -> Result<Option<Client>> {
        self.repository.find_by_id(id).await
    }

    pub async fn list(&self) -> Result<Vec<Client>> {
        self.repository.find_all().await
    }

    pub async fn create(&self, request: CreateClient) -> Result<Client> {
        let client = Client::create(request)?;

        if self.repository.exists_by_name(&client.name).await? {
            bail!("client name already exists: {}", client.name);
        }

        if self.repository.exists_by_mac(&client.mac).await? {
            bail!("client MAC address already exists: {}", client.mac);
        }

        self.repository
            .insert(&client)
            .await
            .context("failed to persist new client")?;

        Ok(client)
    }

    pub async fn update(&self, id: &ClientId, request: UpdateClient) -> Result<Client> {
        let mut client = self
            .repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("client not found: {id}"))?;

        if let Some(name) = request.name {
            let name = name.trim().to_string();

            if name.is_empty() {
                return Err(DomainError::EmptyClientName.into());
            }

            if name != client.name && self.repository.exists_by_name(&name).await? {
                bail!("client name already exists: {name}");
            }

            client.name = name;
        }

        if let Some(mac) = request.mac {
            let parsed = crate::domain::MacAddress::parse(&mac)?;

            if parsed != client.mac && self.repository.exists_by_mac(&parsed).await? {
                bail!("client MAC address already exists: {parsed}");
            }

            client.mac = parsed;
        }

        if let Some(ip) = request.ip {
            client.ip = IpAddr::from_str(ip.trim())
                .map_err(|_| anyhow::anyhow!("invalid IP address: {ip}"))?;
        }

        if let Some(master) = request.master {
            if master.trim().is_empty() {
                return Err(DomainError::EmptyMasterImage.into());
            }

            client.master = master;
        }

        if let Some(snapshot) = request.snapshot {
            client.snapshot = Some(snapshot);
        }

        if let Some(enabled) = request.enabled {
            if enabled {
                client.enable();
            } else {
                client.disable();
            }
        }

        if let Some(keep_writeback) = request.keep_writeback {
            client.keep_writeback = keep_writeback;
        }

        if let Some(use_game_disk) = request.use_game_disk {
            client.use_game_disk = use_game_disk;
        }

        if let Some(block_store) = request.block_store {
            client.block_store = Some(block_store);
        }

        if let Some(block_device) = request.block_device {
            client.block_device = Some(block_device);
        }

        if let Some(target_iqn) = request.target_iqn {
            client.target_iqn = Some(target_iqn);
        }

        if let Some(pxe_mode) = request.pxe_mode {
            client.pxe_mode = pxe_mode;
        }

        if let Some(mode) = request.mode {
            client.mode = mode;
        }

        client.updated_at = Utc::now();
        client.last_modified = Some(client.updated_at);

        self.repository
            .update(&client)
            .await
            .context("failed to persist client update")?;

        Ok(client)
    }

    pub async fn delete(&self, id: &ClientId) -> Result<()> {
        /*
         * IMPORTANT:
         *
         * This is intentionally database-only in Stage 1.
         *
         * We must NOT yet delete ZFS/iSCSI/PXE resources here.
         *
         * That responsibility will move into ProvisioningService/
         * DeprovisioningService in the next stage.
         */
        let deleted = self.repository.delete(id).await?;

        if !deleted {
            bail!("client not found: {id}");
        }

        Ok(())
    }
}
