use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String, // Using 'master' to match legacy model
    pub boot_mode: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateClientRequest {
    pub name: Option<String>,
    pub ip: Option<String>,
    pub master: Option<String>,
    pub snapshot: Option<String>,
    pub keep_writeback: Option<bool>,
    pub use_game_disk: Option<bool>,
    pub enabled: Option<bool>,
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
            boot_mode: "uefi".to_string(), // Default to UEFI for new clients
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            snapshot: req.snapshot,
            block_store: None,
            target_iqn: None,
            writeback: None,
            last_modified: Some(Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            block_device: None,
            status: None,
            mode: None,
            pxe_mode: Some("uefi".to_string()),
            keep_writeback: req.keep_writeback,
            use_game_disk: req.use_game_disk,
        })
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
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
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
    boot_mode: String,
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

    pub async fn list(&self) -> anyhow::Result<Vec<Client>> {
        let rows = sqlx::query_as::<_, ClientRow>(
            r#"
            SELECT id, name, mac, ip, master, boot_mode, enabled, created_at, updated_at,
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
                boot_mode: row.boot_mode,
                enabled: row.enabled,
                created_at: DateTime::parse_from_rfc3339(&row.created_at).expect("Failed to parse created_at").into(),
                updated_at: DateTime::parse_from_rfc3339(&row.updated_at).expect("Failed to parse updated_at").into(),
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
            SELECT id, name, mac, ip, master, boot_mode, enabled, created_at, updated_at,
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
            boot_mode: row.boot_mode,
            enabled: row.enabled,
            created_at: DateTime::parse_from_rfc3339(&row.created_at).expect("Failed to parse created_at").into(),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at).expect("Failed to parse updated_at").into(),
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

        sqlx::query(
            r#"
            INSERT INTO clients (id, name, mac, ip, master, boot_mode, enabled, created_at, updated_at,
                                snapshot, block_store, target_iqn, writeback, last_modified,
                                block_device, status, mode, keep_writeback, use_game_disk)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&client.id)
        .bind(&client.name)
        .bind(&client.mac)
        .bind(&client.ip)
        .bind(&client.master)
        .bind(&client.boot_mode)
        .bind(client.enabled)
        .bind(client.created_at.to_rfc3339())
        .bind(client.updated_at.to_rfc3339())
        .bind(&client.snapshot)
        .bind(&client.block_store)
        .bind(&client.target_iqn)
        .bind(&client.writeback)
        .bind(&client.last_modified)
        .bind(&client.block_device)
        .bind(&client.status)
        .bind(&client.mode)
        .bind(&client.keep_writeback)
        .bind(&client.use_game_disk)
        .execute(&self.pool)
        .await?;

        tracing::info!("Client '{}' created with MAC {}", client.name, client.mac);
        Ok(client)
    }

    pub async fn update(&self, id: &str, req: UpdateClientRequest) -> anyhow::Result<Client> {
        let mut client = self.get(id).await?;

        if let Some(name) = req.name {
            client.name = name;
        }
        if let Some(ip) = req.ip {
            client.ip = ip;
        }
        if let Some(master) = req.master {
            client.master = master;
        }
        if let Some(snapshot) = req.snapshot {
            client.snapshot = Some(snapshot);
        }
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

        sqlx::query(
            r#"
            UPDATE clients
            SET name = ?, ip = ?, master = ?, snapshot = ?, block_store = ?, target_iqn = ?,
                writeback = ?, last_modified = ?, block_device = ?, status = ?, mode = ?,
                keep_writeback = ?, use_game_disk = ?, enabled = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&client.name)
        .bind(&client.ip)
        .bind(&client.master)
        .bind(&client.snapshot)
        .bind(&client.block_store)
        .bind(&client.target_iqn)
        .bind(&client.writeback)
        .bind(&client.last_modified)
        .bind(&client.block_device)
        .bind(&client.status)
        .bind(&client.mode)
        .bind(&client.keep_writeback)
        .bind(&client.use_game_disk)
        .bind(client.enabled)
        .bind(client.updated_at.to_rfc3339())
        .bind(&client.id)
        .execute(&self.pool)
        .await?;

        tracing::info!("Client '{}' updated", client.name);
        Ok(client)
    }

    pub async fn delete(&self, id: &str) -> anyhow::Result<()> {
        let client = self.get(id).await?;

        sqlx::query("DELETE FROM clients WHERE id = ?")
            .bind(&client.id)
            .execute(&self.pool)
            .await?;

        tracing::info!("Client '{}' deleted", client.name);
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
