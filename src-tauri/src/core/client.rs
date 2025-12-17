use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub mac_address: String,
    pub ip_address: Option<String>,
    pub image_id: String,
    pub boot_mode: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateClientRequest {
    pub name: String,
    pub mac_address: String,
    pub ip_address: Option<String>,
    pub image_id: String,
    pub boot_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateClientRequest {
    pub name: Option<String>,
    pub ip_address: Option<String>,
    pub image_id: Option<String>,
    pub boot_mode: Option<String>,
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
        let mac = normalize_mac(&req.mac_address)?;

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            mac_address: mac,
            ip_address: req.ip_address,
            image_id: req.image_id,
            boot_mode: req.boot_mode.unwrap_or_else(|| "uefi".to_string()),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
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

impl ClientManager {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> anyhow::Result<Vec<Client>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                bool,
                String,
                String,
            ),
        >(
            r#"
            SELECT id, name, mac_address, ip_address, image_id, boot_mode, enabled, created_at, updated_at 
            FROM clients 
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let clients = rows
            .into_iter()
            .filter_map(
                |(
                    id,
                    name,
                    mac_address,
                    ip_address,
                    image_id,
                    boot_mode,
                    enabled,
                    created_at,
                    updated_at,
                )| {
                    Some(Client {
                        id,
                        name,
                        mac_address,
                        ip_address,
                        image_id,
                        boot_mode,
                        enabled,
                        created_at: DateTime::parse_from_rfc3339(&created_at)
                            .ok()?
                            .with_timezone(&Utc),
                        updated_at: DateTime::parse_from_rfc3339(&updated_at)
                            .ok()?
                            .with_timezone(&Utc),
                    })
                },
            )
            .collect();

        Ok(clients)
    }

    pub async fn get(&self, id: &str) -> anyhow::Result<Client> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                Option<String>,
                String,
                String,
                bool,
                String,
                String,
            ),
        >(
            r#"
            SELECT id, name, mac_address, ip_address, image_id, boot_mode, enabled, created_at, updated_at 
            FROM clients 
            WHERE id = ? OR name = ? OR mac_address = ?
            "#,
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Client not found: {}", id))?;

        let (
            id,
            name,
            mac_address,
            ip_address,
            image_id,
            boot_mode,
            enabled,
            created_at,
            updated_at,
        ) = row;

        Ok(Client {
            id,
            name,
            mac_address,
            ip_address,
            image_id,
            boot_mode,
            enabled,
            created_at: DateTime::parse_from_rfc3339(&created_at)?.with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&updated_at)?.with_timezone(&Utc),
        })
    }

    pub async fn create(&self, req: CreateClientRequest) -> anyhow::Result<Client> {
        let client = Client::new(req)?;

        sqlx::query(
            r#"
            INSERT INTO clients (id, name, mac_address, ip_address, image_id, boot_mode, enabled, created_at, updated_at) 
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&client.id)
        .bind(&client.name)
        .bind(&client.mac_address)
        .bind(&client.ip_address)
        .bind(&client.image_id)
        .bind(&client.boot_mode)
        .bind(client.enabled)
        .bind(client.created_at.to_rfc3339())
        .bind(client.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        tracing::info!(
            "Client '{}' created with MAC {}",
            client.name,
            client.mac_address
        );
        Ok(client)
    }

    pub async fn update(&self, id: &str, req: UpdateClientRequest) -> anyhow::Result<Client> {
        let mut client = self.get(id).await?;

        if let Some(name) = req.name {
            client.name = name;
        }
        if let Some(ip) = req.ip_address {
            client.ip_address = Some(ip);
        }
        if let Some(image_id) = req.image_id {
            client.image_id = image_id;
        }
        if let Some(boot_mode) = req.boot_mode {
            client.boot_mode = boot_mode;
        }
        if let Some(enabled) = req.enabled {
            client.enabled = enabled;
        }
        client.updated_at = Utc::now();

        sqlx::query(
            r#"
            UPDATE clients 
            SET name = ?, ip_address = ?, image_id = ?, boot_mode = ?, enabled = ?, updated_at = ? 
            WHERE id = ?
            "#,
        )
        .bind(&client.name)
        .bind(&client.ip_address)
        .bind(&client.image_id)
        .bind(&client.boot_mode)
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
