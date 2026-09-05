use crate::domain::{BootMode, Client, ClientId, ClientStatus, MacAddress, PxeMode};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Debug, FromRow)]
struct ClientRow {
    id: String,
    name: String,
    mac: String,
    ip: String,
    master: String,
    enabled: i64,

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
    pxe_mode: Option<String>,

    keep_writeback: Option<i64>,
    use_game_disk: Option<i64>,
}

#[derive(Clone)]
pub struct ClientRepository {
    pool: SqlitePool,
}

impl ClientRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn find_by_id(&self, id: &ClientId) -> Result<Option<Client>> {
        let row = sqlx::query_as::<_, ClientRow>(
            r#"
            SELECT
                id,
                name,
                mac,
                ip,
                master,
                enabled,
                created_at,
                updated_at,
                snapshot,
                block_store,
                target_iqn,
                writeback,
                last_modified,
                block_device,
                status,
                mode,
                pxe_mode,
                keep_writeback,
                use_game_disk
            FROM clients
            WHERE id = ?
            "#,
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .context("failed to query client by id")?;

        row.map(Self::row_to_domain).transpose()
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Client>> {
        let row = sqlx::query_as::<_, ClientRow>(
            r#"
            SELECT
                id,
                name,
                mac,
                ip,
                master,
                enabled,
                created_at,
                updated_at,
                snapshot,
                block_store,
                target_iqn,
                writeback,
                last_modified,
                block_device,
                status,
                mode,
                pxe_mode,
                keep_writeback,
                use_game_disk
            FROM clients
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .context("failed to query client by name")?;

        row.map(Self::row_to_domain).transpose()
    }

    pub async fn find_all(&self) -> Result<Vec<Client>> {
        let rows = sqlx::query_as::<_, ClientRow>(
            r#"
            SELECT
                id,
                name,
                mac,
                ip,
                master,
                enabled,
                created_at,
                updated_at,
                snapshot,
                block_store,
                target_iqn,
                writeback,
                last_modified,
                block_device,
                status,
                mode,
                pxe_mode,
                keep_writeback,
                use_game_disk
            FROM clients
            ORDER BY name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to query clients")?;

        rows.into_iter()
            .map(Self::row_to_domain)
            .collect::<Result<Vec<_>>>()
    }

    pub async fn insert(&self, client: &Client) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO clients (
                id,
                name,
                mac,
                ip,
                master,
                enabled,
                created_at,
                updated_at,
                snapshot,
                block_store,
                target_iqn,
                writeback,
                last_modified,
                block_device,
                status,
                mode,
                pxe_mode,
                keep_writeback,
                use_game_disk
            )
            VALUES (
                ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
            )
            "#,
        )
        .bind(client.id.as_str())
        .bind(&client.name)
        .bind(client.mac.as_str())
        .bind(client.ip.to_string())
        .bind(&client.master)
        .bind(if client.enabled { 1 } else { 0 })
        .bind(client.created_at.to_rfc3339())
        .bind(client.updated_at.to_rfc3339())
        .bind(&client.snapshot)
        .bind(&client.block_store)
        .bind(&client.target_iqn)
        .bind(&client.writeback)
        .bind(client.last_modified.map(|v| v.to_rfc3339()))
        .bind(&client.block_device)
        .bind(client.status.as_str())
        .bind(client.mode.as_str())
        .bind(client.pxe_mode.as_str())
        .bind(if client.keep_writeback { 1 } else { 0 })
        .bind(if client.use_game_disk { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .context("failed to insert client")?;

        Ok(())
    }

    pub async fn update(&self, client: &Client) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE clients
            SET
                name = ?,
                mac = ?,
                ip = ?,
                master = ?,
                enabled = ?,
                updated_at = ?,
                snapshot = ?,
                block_store = ?,
                target_iqn = ?,
                writeback = ?,
                last_modified = ?,
                block_device = ?,
                status = ?,
                mode = ?,
                pxe_mode = ?,
                keep_writeback = ?,
                use_game_disk = ?
            WHERE id = ?
            "#,
        )
        .bind(&client.name)
        .bind(client.mac.as_str())
        .bind(client.ip.to_string())
        .bind(&client.master)
        .bind(if client.enabled { 1 } else { 0 })
        .bind(client.updated_at.to_rfc3339())
        .bind(&client.snapshot)
        .bind(&client.block_store)
        .bind(&client.target_iqn)
        .bind(&client.writeback)
        .bind(client.last_modified.map(|v| v.to_rfc3339()))
        .bind(&client.block_device)
        .bind(client.status.as_str())
        .bind(client.mode.as_str())
        .bind(client.pxe_mode.as_str())
        .bind(if client.keep_writeback { 1 } else { 0 })
        .bind(if client.use_game_disk { 1 } else { 0 })
        .bind(client.id.as_str())
        .execute(&self.pool)
        .await
        .context("failed to update client")?;

        Ok(())
    }

    pub async fn delete(&self, id: &ClientId) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM clients
            WHERE id = ?
            "#,
        )
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .context("failed to delete client")?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn exists_by_name(&self, name: &str) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM clients
                WHERE name = ?
            )
            "#,
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .context("failed to check client name")?;

        Ok(exists != 0)
    }

    pub async fn exists_by_mac(&self, mac: &MacAddress) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM clients
                WHERE mac = ?
            )
            "#,
        )
        .bind(mac.as_str())
        .fetch_one(&self.pool)
        .await
        .context("failed to check client MAC")?;

        Ok(exists != 0)
    }

    pub async fn exists_by_ip(&self, ip: &IpAddr) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clients WHERE ip = ?)")
            .bind(ip.to_string())
            .fetch_one(&self.pool)
            .await
            .context("failed to check client IP address")?;
        Ok(exists != 0)
    }

    fn row_to_domain(row: ClientRow) -> Result<Client> {
        let id = ClientId::from_string(row.id).map_err(|error| anyhow::anyhow!(error))?;

        let mac = MacAddress::parse(&row.mac).map_err(|error| anyhow::anyhow!(error))?;

        let ip = IpAddr::from_str(&row.ip)
            .with_context(|| format!("invalid stored client IP: {}", row.ip))?;

        let created_at = parse_datetime(&row.created_at)?;
        let updated_at = parse_datetime(&row.updated_at)?;

        let last_modified = row
            .last_modified
            .as_deref()
            .map(parse_datetime)
            .transpose()?;

        Ok(Client {
            id,
            name: row.name,
            mac,
            ip,
            master: row.master,
            enabled: row.enabled != 0,

            created_at,
            updated_at,

            snapshot: row.snapshot,
            block_store: row.block_store,
            target_iqn: row.target_iqn,
            writeback: row.writeback,
            last_modified,
            block_device: row.block_device,

            status: parse_status(row.status.as_deref()),
            mode: parse_mode(row.mode.as_deref()),
            pxe_mode: parse_pxe_mode(row.pxe_mode.as_deref()),

            keep_writeback: row.keep_writeback.unwrap_or(1) != 0,
            use_game_disk: row.use_game_disk.unwrap_or(0) != 0,
        })
    }
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.with_timezone(&Utc));
    }

    if let Ok(value) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Ok(value.and_utc());
    }

    Err(anyhow::anyhow!("invalid stored datetime: {value}"))
}

fn parse_status(value: Option<&str>) -> ClientStatus {
    match value
        .unwrap_or("provisioning")
        .to_ascii_lowercase()
        .as_str()
    {
        "ready" => ClientStatus::Ready,
        "online" => ClientStatus::Online,
        "offline" => ClientStatus::Offline,
        "error" => ClientStatus::Error,
        "disabled" => ClientStatus::Disabled,
        _ => ClientStatus::Provisioning,
    }
}

fn parse_mode(value: Option<&str>) -> BootMode {
    match value.unwrap_or("normal").to_ascii_lowercase().as_str() {
        "super" => BootMode::Super,
        _ => BootMode::Normal,
    }
}

fn parse_pxe_mode(value: Option<&str>) -> PxeMode {
    match value.unwrap_or("uefi").to_ascii_lowercase().as_str() {
        "bios" | "legacy" => PxeMode::Bios,
        _ => PxeMode::Uefi,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CreateClient, PxeMode};

    #[test]
    fn parses_mac_addresses() {
        let mac = MacAddress::parse("AA-BB-CC-DD-EE-FF").expect("MAC should parse");

        assert_eq!(mac.as_str(), "aa:bb:cc:dd:ee:ff");
    }

    #[test]
    fn rejects_invalid_mac_addresses() {
        assert!(MacAddress::parse("invalid").is_err());
    }

    #[test]
    fn parses_datetime_rfc3339() {
        let value = parse_datetime("2026-08-16T08:00:00+00:00").expect("datetime should parse");

        assert_eq!(value.to_rfc3339(), "2026-08-16T08:00:00+00:00");
    }

    #[test]
    fn parses_legacy_datetime() {
        let value = parse_datetime("2026-08-16 08:00:00").expect("legacy datetime should parse");

        assert_eq!(
            value.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-08-16 08:00:00"
        );
    }

    #[test]
    fn creates_valid_client_domain_object() {
        let client = Client::create(CreateClient {
            name: "PC001".to_string(),
            mac: "AA:BB:CC:DD:EE:FF".to_string(),
            ip: "192.168.1.101".to_string(),
            master: "diskless/windows11".to_string(),

            snapshot: Some("diskless/windows11@snap-001".to_string()),

            block_store: None,
            block_device: None,
            target_iqn: None,

            pxe_mode: PxeMode::Uefi,
            keep_writeback: true,
            use_game_disk: false,
        })
        .expect("client should be valid");

        assert_eq!(client.name, "PC001");
        assert_eq!(client.mac.as_str(), "aa:bb:cc:dd:ee:ff");
        assert!(client.enabled);
    }
}
