use chrono::{DateTime, Utc};
use log::info;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use uuid::Uuid;

/// Core Client type with DateTime fields for business logic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String, // Using 'master' to match legacy model
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub snapshot: Option<String>,
    pub block_store: Option<String>,
    pub target_iqn: Option<String>,
    pub writeback: Option<String>,
    pub last_modified: Option<String>,
    pub block_device: Option<String>,
    pub status: Option<String>,
    pub mode: Option<String>,
    pub pxe_mode: Option<String>,
    pub keep_writeback: Option<bool>,
    pub use_game_disk: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateClientRequest {
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String,
    pub snapshot: Option<String>,
    pub keep_writeback: Option<bool>,
    pub use_game_disk: Option<bool>,
    pub block_store: Option<String>,
    pub block_device: Option<String>,
    pub target_iqn: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateClientRequest {
    pub name: Option<String>,
    pub mac: Option<String>,
    pub ip: Option<String>,
    pub master: Option<String>,
    pub snapshot: Option<String>,
    pub keep_writeback: Option<bool>,
    pub use_game_disk: Option<bool>,
    pub enabled: Option<bool>,
    pub block_store: Option<String>,
    pub block_device: Option<String>,
    pub target_iqn: Option<String>,
    pub action: Option<String>,
    pub make_super: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootLogEntry {
    pub id: String,
    pub client_id: String,
    pub image_id: Option<String>,
    pub boot_time: DateTime<Utc>,
    pub success: bool,
    pub duration_ms: Option<i64>,
    pub message: Option<String>,
}

impl Client {
    pub fn new(req: CreateClientRequest) -> anyhow::Result<Self> {
        let mac = normalize_mac(&req.mac)?;

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            mac,
            ip: req.ip,
            master: req.master,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            snapshot: req.snapshot,
            block_store: req.block_store,
            target_iqn: req.target_iqn,
            writeback: None,
            last_modified: Some(Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            block_device: req.block_device,
            status: None,
            mode: None,
            pxe_mode: Some("uefi".to_string()),
            keep_writeback: req.keep_writeback,
            use_game_disk: req.use_game_disk,
        })
    }

    /// Check if client is online
    pub fn is_online(&self) -> bool {
        matches!(self.status.as_deref(), Some("Online"))
    }

    /// Check if client is offline
    pub fn is_offline(&self) -> bool {
        matches!(self.status.as_deref(), Some("Offline"))
    }

    /// Check if client is in super mode
    pub fn is_super_mode(&self) -> bool {
        matches!(self.mode.as_deref(), Some("super"))
    }

    /// Get normalized MAC address (uppercase, colon-separated)
    pub fn normalized_mac(&self) -> String {
        self.mac.to_uppercase()
    }

    /// Check if client has a master image assigned
    pub fn has_master(&self) -> bool {
        !self.master.is_empty()
    }

    /// Check if client has a snapshot assigned
    pub fn has_snapshot(&self) -> bool {
        self.snapshot.is_some() && !self.snapshot.as_ref().map_or(false, |s| s.is_empty())
    }
}

fn normalize_mac(mac: &str) -> anyhow::Result<String> {
    let cleaned: String = mac
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_lowercase();

    if cleaned.len() != 12 {
        return Err(anyhow::anyhow!(
            "Invalid MAC address format. Expected 12 hex digits."
        ));
    }

    let formatted = cleaned
        .as_bytes()
        .chunks(2)
        .map(|chunk| std::str::from_utf8(chunk).expect("Invalid UTF-8 in MAC address"))
        .collect::<Vec<_>>()
        .join(":");

    Ok(formatted)
}

pub struct ClientManager {
    pool: SqlitePool,
}

#[derive(sqlx::FromRow)]
struct ClientRow {
    id: String,
    name: String,
    mac: String,
    ip: String,
    master: String,
    enabled: bool,
    created_at: String,
    updated_at: String,
    snapshot: Option<String>,
    block_store: Option<String>,
    target_iqn: Option<String>,
    writeback: Option<String>,
    last_modified: Option<String>,
    block_device: Option<String>,
    status: Option<String>,
    mode: Option<String>,
    keep_writeback: Option<bool>,
    use_game_disk: Option<bool>,
}

impl ClientManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Public helper function to upsert a client (INSERT OR REPLACE)
    /// Used by create(), update() methods and config.rs
    pub async fn upsert_client(pool: &SqlitePool, client: &Client) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO clients (
                id, name, mac, ip, master, snapshot, block_store, target_iqn,
                writeback, block_device, status, mode, pxe_mode, keep_writeback,
                use_game_disk, created_at, last_modified, enabled, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&client.id)
        .bind(&client.name)
        .bind(&client.mac)
        .bind(&client.ip)
        .bind(&client.master)
        .bind(&client.snapshot)
        .bind(&client.block_store)
        .bind(&client.target_iqn)
        .bind(&client.writeback)
        .bind(&client.block_device)
        .bind(&client.status)
        .bind(&client.mode)
        .bind(client.pxe_mode.as_ref().unwrap_or(&"uefi".to_string()))
        .bind(client.keep_writeback.unwrap_or(true))
        .bind(client.use_game_disk.unwrap_or(false))
        .bind(client.created_at.to_rfc3339())
        .bind(&client.last_modified)
        .bind(client.enabled)
        .bind(client.updated_at.to_rfc3339())
        .execute(pool)
        .await?;
        
        Ok(())
    }

    pub async fn list(&self) -> anyhow::Result<Vec<Client>> {
        let rows = sqlx::query_as::<_, ClientRow>(
            r#"
            SELECT id, name, mac, ip, master, enabled, created_at, updated_at,
                   snapshot, block_store, target_iqn, writeback, last_modified,
                   block_device, status, mode, keep_writeback, use_game_disk
            FROM clients
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let clients: Vec<Client> = rows
            .into_iter()
            .map(|row| Client {
                id: row.id,
                name: row.name,
                mac: row.mac,
                ip: row.ip,
                master: row.master,
                enabled: row.enabled,
                created_at: DateTime::parse_from_rfc3339(&row.created_at)
                    .unwrap_or_else(|_| {
                        // If parsing fails, use current time as fallback
                        chrono::Utc::now().into()
                    })
                    .into(),
                updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                    .unwrap_or_else(|_| {
                        // If parsing fails, use current time as fallback
                        chrono::Utc::now().into()
                    })
                    .into(),
                snapshot: row.snapshot,
                block_store: row.block_store,
                target_iqn: row.target_iqn,
                writeback: row.writeback,
                last_modified: row.last_modified,
                block_device: row.block_device,
                status: row.status,
                mode: row.mode,
                pxe_mode: Some("uefi".to_string()), // Default to UEFI
                keep_writeback: row.keep_writeback,
                use_game_disk: row.use_game_disk,
            })
            .collect();

        Ok(clients)
    }

    pub async fn get(&self, id: &str) -> anyhow::Result<Client> {
        let row = sqlx::query_as::<_, ClientRow>(
            r#"
            SELECT id, name, mac, ip, master, enabled, created_at, updated_at,
                   snapshot, block_store, target_iqn, writeback, last_modified,
                   block_device, status, mode, keep_writeback, use_game_disk
            FROM clients
            WHERE id = ? OR name = ? OR mac = ?
            "#,
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Client not found: {}", id))?;

        Ok(Client {
            id: row.id,
            name: row.name,
            mac: row.mac,
            ip: row.ip,
            master: row.master,

            enabled: row.enabled,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .unwrap_or_else(|_| {
                    // If parsing fails, use current time as fallback
                    chrono::Utc::now().into()
                })
                .into(),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .unwrap_or_else(|_| {
                    // If parsing fails, use current time as fallback
                    chrono::Utc::now().into()
                })
                .into(),
            snapshot: row.snapshot,
            block_store: row.block_store,
            target_iqn: row.target_iqn,
            writeback: row.writeback,
            last_modified: row.last_modified,
            block_device: row.block_device,
            status: row.status,
            mode: row.mode,
            pxe_mode: Some("uefi".to_string()), // Default to UEFI
            keep_writeback: row.keep_writeback,
            use_game_disk: row.use_game_disk,
        })
    }

    pub async fn create(&self, req: CreateClientRequest) -> anyhow::Result<Client> {
        let client = Client::new(req)?;

        Self::upsert_client(&self.pool, &client).await?;

        info!("Client '{}' created with MAC {}", client.name, client.mac);
        Ok(client)
    }

    pub async fn update(&self, id: &str, req: UpdateClientRequest) -> anyhow::Result<Client> {
        let mut client = self.get(id).await?;

        if let Some(name) = req.name {
            client.name = name;
        }
        if let Some(mac) = req.mac {
            client.mac = mac;
        }
        if let Some(ip) = req.ip {
            client.ip = ip;
        }
        if let Some(master) = req.master {
            client.master = master;
        }
        client.snapshot = req.snapshot;
        client.block_store = req.block_store;
        client.target_iqn = req.target_iqn;
        client.block_device = req.block_device;
        if let Some(keep_writeback) = req.keep_writeback {
            client.keep_writeback = Some(keep_writeback);
        }
        if let Some(use_game_disk) = req.use_game_disk {
            client.use_game_disk = Some(use_game_disk);
        }
        if let Some(enabled) = req.enabled {
            client.enabled = enabled;
        }
        client.updated_at = Utc::now();
        client.last_modified = Some(client.updated_at.format("%Y-%m-%d %H:%M:%S").to_string());

        Self::upsert_client(&self.pool, &client).await?;

        info!("Client '{}' updated", client.name);
        Ok(client)
    }

    pub async fn delete(&self, id: &str) -> anyhow::Result<()> {
        let client = self.get(id).await?;

        // Delete all related records first (foreign key constraints)
        // Order matters - delete child records before parent
        
        // Delete boot logs
        sqlx::query("DELETE FROM boot_logs WHERE client_id = ?")
            .bind(&client.id)
            .execute(&self.pool)
            .await?;

        // Delete control operations
        sqlx::query("DELETE FROM control_operations WHERE client_id = ?")
            .bind(&client.id)
            .execute(&self.pool)
            .await?;

        // Delete error logs
        sqlx::query("DELETE FROM error_logs WHERE client_id = ?")
            .bind(&client.id)
            .execute(&self.pool)
            .await?;

        // Delete scheduled operations
        sqlx::query("DELETE FROM scheduled_operations WHERE client_id = ?")
            .bind(&client.id)
            .execute(&self.pool)
            .await?;

        // Delete OS type cache
        sqlx::query("DELETE FROM os_type_cache WHERE client_id = ?")
            .bind(&client.id)
            .execute(&self.pool)
            .await?;

        // Finally, delete the client
        sqlx::query("DELETE FROM clients WHERE id = ?")
            .bind(&client.id)
            .execute(&self.pool)
            .await?;

        info!("Client '{}' and all related records deleted", client.name);
        Ok(())
    }

    pub async fn get_boot_history(
        &self,
        client_id: &str,
        limit: i32,
    ) -> anyhow::Result<Vec<BootLogEntry>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                String,
                bool,
                Option<i64>,
                Option<String>,
            ),
        >(
            r#"
            SELECT id, client_id, image_id, boot_time, success, duration_ms, message 
            FROM boot_logs 
            WHERE client_id = ? 
            ORDER BY boot_time DESC 
            LIMIT ?
            "#,
        )
        .bind(client_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let logs = rows
            .into_iter()
            .filter_map(
                |(id, client_id, image_id, boot_time, success, duration_ms, message)| {
                    Some(BootLogEntry {
                        id,
                        client_id,
                        image_id,
                        boot_time: DateTime::parse_from_rfc3339(&boot_time)
                            .ok()?
                            .with_timezone(&Utc),
                        success,
                        duration_ms,
                        message,
                    })
                },
            )
            .collect();

        Ok(logs)
    }
}
