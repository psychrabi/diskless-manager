use crate::domain::{Client, ClientId, CreateClient, DomainError, UpdateClient};
use crate::infrastructure::iscsi::ChapCredentials;
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
        let client = self.repository.find_by_id(id).await?;
        match client {
            Some(mut client) => {
                client.game_disks = self.repository.game_selection(id).await?;
                Ok(Some(client))
            }
            None => Ok(None),
        }
    }

    /// Compatibility lookup for existing transport/control code that still
    /// receives client identifiers as raw strings.
    pub async fn get_by_string(&self, id: &str) -> Result<Option<Client>> {
        let id = ClientId::from_string(id.to_owned()).map_err(anyhow::Error::from)?;
        self.get(&id).await
    }

    pub async fn list(&self) -> Result<Vec<Client>> {
        let mut clients = self.repository.find_all().await?;
        for client in &mut clients {
            client.game_disks = self
                .repository
                .game_selection(&client.id)
                .await
                .with_context(|| {
                    format!("failed to load game selection for client '{}'", client.id)
                })?;
        }
        Ok(clients)
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

    /// Rotate the server-managed one-way CHAP credentials and persist the
    /// resulting enforcement state. Applying credentials to the live target
    /// and boot menu remains orchestration owned by the caller.
    pub async fn rotate_chap(&self, id: &str) -> Result<(Client, ChapCredentials)> {
        let client_id = ClientId::from_string(id.to_owned())?;
        let mut client = self
            .repository
            .find_by_id(&client_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("client not found: {id}"))?;

        let username = client
            .chap_user
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| ChapCredentials::username_for_client(&client.name));

        let mut credentials = ChapCredentials::generate(&client.name)?;
        credentials.username = username;

        client.chap_user = Some(credentials.username.clone());
        client.chap_secret = Some(credentials.password.clone());
        client.chap_enabled = true;

        self.repository
            .update(&client)
            .await
            .context("failed to persist rotated CHAP credentials")?;

        Ok((client, credentials))
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

    /// Compatibility update for transport paths that still carry raw string IDs.
    pub async fn update_by_string(&self, id: &str, request: UpdateClient) -> Result<Client> {
        let id = ClientId::from_string(id.to_owned()).map_err(anyhow::Error::from)?;
        self.update(&id, request).await
    }

    pub async fn game_selection(&self, id: &str) -> Result<Vec<String>> {
        let id = ClientId::from_string(id.to_owned()).map_err(anyhow::Error::from)?;
        self.repository.game_selection(&id).await
    }

    pub async fn set_game_selection(&self, id: &str, masters: &[String]) -> Result<()> {
        let id = ClientId::from_string(id.to_owned()).map_err(anyhow::Error::from)?;
        self.repository
            .set_game_selection(&id, masters)
            .await
            .context("failed to persist client game selection")
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
