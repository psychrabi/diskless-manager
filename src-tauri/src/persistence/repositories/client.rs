use crate::domain::{
    BootMode, Client, ClientBootImage, ClientId, ClientStatus, MacAddress, PxeMode,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use sqlx::{FromRow, SqlitePool};
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Debug, FromRow, serde::Deserialize)]
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
    boot_image: Option<String>,

    keep_writeback: Option<i64>,
    use_game_disk: Option<i64>,
    chap_user: Option<String>,
    chap_secret: Option<String>,
    chap_enabled: Option<i64>,
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
                boot_image,
                keep_writeback,
                use_game_disk,
                chap_user,
                chap_secret,
                chap_enabled
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
                boot_image,
                keep_writeback,
                use_game_disk,
                chap_user,
                chap_secret,
                chap_enabled
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

    pub async fn find_by_mac(&self, mac: &MacAddress) -> Result<Option<Client>> {
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
                boot_image,
                keep_writeback,
                use_game_disk,
                chap_user,
                chap_secret,
                chap_enabled
            FROM clients
            WHERE mac = ?
            "#,
        )
        .bind(mac.as_str())
        .fetch_optional(&self.pool)
        .await
        .context("failed to query client by MAC")?;

        row.map(Self::row_to_domain).transpose()
    }

    /// Returns clients ordered by name using the same validation as individual lookups.
    /// Database and stored metadata errors are returned without omitting invalid clients.
    pub async fn find_all(&self) -> Result<Vec<Client>> {
        // Transfer bounded batches instead of one worker message per client.
        // Array positions follow ClientRow's declaration; retain its domain validation.
        let mut batches = sqlx::query_scalar::<_, String>(
            r#"
            SELECT json_group_array(json_array(
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
                boot_image,
                keep_writeback,
                use_game_disk,
                chap_user,
                chap_secret,
                chap_enabled
            ) ORDER BY name ASC)
            FROM (
                SELECT id, name, mac, ip, master, enabled, created_at, updated_at,
                    snapshot, block_store, target_iqn, writeback, last_modified,
                    block_device, status, mode, pxe_mode, boot_image, keep_writeback, use_game_disk,
                    chap_user, chap_secret, chap_enabled,
                    (ROW_NUMBER() OVER (ORDER BY name ASC) - 1) / 128 AS batch
                FROM clients
            )
            GROUP BY batch
            ORDER BY batch ASC
            "#,
        )
        .fetch(&self.pool);
        let mut clients = Vec::new();
        while let Some(document) = batches
            .try_next()
            .await
            .context("failed to query clients")?
        {
            let rows: Vec<ClientRow> =
                serde_json::from_str(&document).context("failed to decode stored clients")?;
            drop(document);
            for row in rows {
                clients.push(Self::row_to_domain(row)?);
            }
        }
        Ok(clients)
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
                boot_image,
                keep_writeback,
                use_game_disk,
                chap_user,
                chap_secret,
                chap_enabled
            )
            VALUES (
                ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
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
        .bind(client.boot_image.as_str())
        .bind(if client.keep_writeback { 1 } else { 0 })
        .bind(if client.use_game_disk { 1 } else { 0 })
        .bind(&client.chap_user)
        .bind(&client.chap_secret)
        .bind(if client.chap_enabled { 1 } else { 0 })
        .execute(&self.pool)
        .await
        .context("failed to insert client")?;

        Ok(())
    }

    /// Stored per-client game master selection (possibly empty).
    pub async fn game_selection(&self, id: &ClientId) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT master_dataset FROM client_game_disks WHERE client_id = ?",
        )
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await?)
    }

    /// Every stored game selection in one query, keyed by client id.
    pub async fn all_game_selections(
        &self,
    ) -> Result<std::collections::HashMap<String, Vec<String>>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT client_id, master_dataset FROM client_game_disks",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut selections = std::collections::HashMap::new();
        for (client_id, master) in rows {
            selections
                .entry(client_id)
                .or_insert_with(Vec::new)
                .push(master);
        }
        Ok(selections)
    }

    /// Stored CHAP username/secret, if any.
    pub async fn chap_credentials(
        &self,
        id: &ClientId,
    ) -> Result<(Option<String>, Option<String>)> {
        let row: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT chap_user, chap_secret FROM clients WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.unwrap_or((None, None)))
    }

    /// Persist (possibly rotated) CHAP credentials.
    pub async fn set_chap_credentials(
        &self,
        id: &ClientId,
        username: &str,
        secret: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE clients SET chap_user = ?, chap_secret = ? WHERE id = ?")
            .bind(username)
            .bind(secret)
            .bind(id.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Replace a client's stored game master selection wholesale.
    pub async fn set_game_selection(&self, id: &ClientId, masters: &[String]) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM client_game_disks WHERE client_id = ?")
            .bind(id.as_str())
            .execute(&mut *transaction)
            .await?;
        for master in masters {
            sqlx::query("INSERT INTO client_game_disks (client_id, master_dataset) VALUES (?, ?)")
                .bind(id.as_str())
                .bind(master)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
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
                boot_image = ?,
                keep_writeback = ?,
                use_game_disk = ?,
                chap_user = ?,
                chap_secret = ?,
                chap_enabled = ?
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
        .bind(client.boot_image.as_str())
        .bind(if client.keep_writeback { 1 } else { 0 })
        .bind(if client.use_game_disk { 1 } else { 0 })
        .bind(&client.chap_user)
        .bind(&client.chap_secret)
        .bind(if client.chap_enabled { 1 } else { 0 })
        .bind(client.id.as_str())
        .execute(&self.pool)
        .await
        .context("failed to update client")?;

        Ok(())
    }

    pub async fn delete(&self, id: &ClientId) -> Result<bool> {
        // Most child tables have no ON DELETE CASCADE; remove their rows
        // before the parent row or the delete fails with a foreign key error.
        for table in [
            "boot_logs",
            "control_operations",
            "error_logs",
            "scheduled_operations",
            "os_type_cache",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE client_id = ?"))
                .bind(id.as_str())
                .execute(&self.pool)
                .await
                .with_context(|| format!("failed to delete {table} for client"))?;
        }
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
            boot_image: parse_boot_image(row.boot_image.as_deref()),

            keep_writeback: row.keep_writeback.unwrap_or(1) != 0,
            use_game_disk: row.use_game_disk.unwrap_or(0) != 0,
            // Selection lives in `client_game_disks`; populated by the
            // service layer after fetch.
            game_disks: Vec::new(),
            chap_user: row.chap_user,
            chap_secret: row.chap_secret,
            chap_enabled: row.chap_enabled.unwrap_or(0) != 0,
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

fn parse_boot_image(value: Option<&str>) -> ClientBootImage {
    match value.unwrap_or("windows").to_ascii_lowercase().as_str() {
        "winpe" => ClientBootImage::Winpe,
        "linux" => ClientBootImage::Linux,
        _ => ClientBootImage::Windows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CreateClient, PxeMode};

    #[tokio::test]
    async fn listing_preserves_lookup_decoding_and_order() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let repository = ClientRepository::new(pool.clone());
        assert!(repository.find_all().await.unwrap().is_empty());
        for (id, name, mac, ip) in [
            ("z", "Zulu", "AA-BB-CC-DD-EE-FF", "2001:db8::1"),
            ("a", "Alpha", "00:11:22:33:44:55", "192.168.1.2"),
        ] {
            sqlx::query("INSERT INTO clients (id,name,mac,ip,master,enabled,created_at,updated_at,snapshot,block_store,target_iqn,writeback,last_modified,block_device,status,mode,pxe_mode,keep_writeback,use_game_disk,chap_user,chap_secret,chap_enabled) VALUES (?,?,?,?,'diskless/windows',2,'2026-01-01T05:45:00+05:45','2026-01-01 00:00:00','diskless/windows@ready','store','iqn.test','writeback','2026-01-01 00:00:00','/dev/zvol/test','ONLINE','SUPER','legacy',2,2,'user','quoted\"secret',2)")
                .bind(id).bind(name).bind(mac).bind(ip).execute(&pool).await.unwrap();
        }
        for defaults in [false, true] {
            sqlx::query(
                "UPDATE clients SET enabled=0,keep_writeback=1,use_game_disk=0,chap_enabled=1",
            )
            .execute(&pool)
            .await
            .unwrap();
            if defaults {
                sqlx::query("UPDATE clients SET snapshot=NULL,block_store=NULL,target_iqn=NULL,writeback=NULL,last_modified=NULL,block_device=NULL,status=NULL,mode=NULL,pxe_mode=NULL,chap_user=NULL,chap_secret=NULL")
                    .execute(&pool).await.unwrap();
            }
            let expected = vec![
                repository.find_by_name("Alpha").await.unwrap().unwrap(),
                repository.find_by_name("Zulu").await.unwrap().unwrap(),
            ];
            let actual = repository.find_all().await.unwrap();
            assert_eq!(
                serde_json::to_value(&actual).unwrap(),
                serde_json::to_value(&expected).unwrap()
            );
            for (actual, expected) in actual.iter().zip(&expected) {
                assert_eq!(actual.chap_user, expected.chap_user);
                assert_eq!(actual.chap_secret, expected.chap_secret);
            }
        }
        for column in ["mac", "ip", "created_at", "updated_at", "last_modified"] {
            let previous: Option<String> =
                sqlx::query_scalar(&format!("SELECT {column} FROM clients WHERE id='a'"))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            sqlx::query(&format!(
                "UPDATE clients SET {column}='invalid' WHERE id='a'"
            ))
            .execute(&pool)
            .await
            .unwrap();
            assert!(
                repository.find_all().await.is_err(),
                "accepted invalid {column}"
            );
            assert!(repository.find_by_name("Alpha").await.is_err());
            sqlx::query(&format!("UPDATE clients SET {column}=? WHERE id='a'"))
                .bind(previous)
                .execute(&pool)
                .await
                .unwrap();
        }
        for column in ["enabled", "keep_writeback", "use_game_disk", "chap_enabled"] {
            sqlx::query(&format!("UPDATE clients SET {column}=12.5 WHERE id='a'"))
                .execute(&pool)
                .await
                .unwrap();
            assert!(
                repository.find_all().await.is_err(),
                "accepted REAL {column}"
            );
            assert!(repository.find_by_name("Alpha").await.is_err());
            sqlx::query(&format!("UPDATE clients SET {column}=2 WHERE id='a'"))
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE clients SET chap_secret=? WHERE id='a'")
            .bind(vec![65_u8, 66])
            .execute(&pool)
            .await
            .unwrap();
        assert!(repository.find_all().await.is_err());
        assert!(repository.find_by_name("Alpha").await.is_err());
    }

    #[tokio::test]
    async fn listing_preserves_order_across_batches() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("WITH RECURSIVE n(i) AS (SELECT 300 UNION ALL SELECT i-1 FROM n WHERE i>0) INSERT INTO clients (id,name,mac,ip,master,created_at,updated_at) SELECT 'pc-'||i,printf('PC%06d',i),printf('02:00:00:00:%02x:%02x',(i>>8)&255,i&255),printf('2001:db8::%x',i+1),'pending','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z' FROM n")
            .execute(&pool).await.unwrap();
        let clients = ClientRepository::new(pool).find_all().await.unwrap();
        assert_eq!(clients.len(), 301);
        for (index, client) in clients.iter().enumerate() {
            assert_eq!(client.name, format!("PC{index:06}"));
        }
    }

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
            boot_image: ClientBootImage::Windows,
            keep_writeback: true,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap_enabled: false,
        })
        .expect("client should be valid");

        assert_eq!(client.name, "PC001");
        assert_eq!(client.mac.as_str(), "aa:bb:cc:dd:ee:ff");
        assert!(client.enabled);
    }

    #[tokio::test]
    async fn round_trips_a_client_through_the_mac_lookup() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE clients (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                mac TEXT NOT NULL,
                ip TEXT NOT NULL,
                master TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                snapshot TEXT,
                block_store TEXT,
                target_iqn TEXT,
                writeback TEXT,
                last_modified TEXT,
                block_device TEXT,
                status TEXT,
                mode TEXT,
                pxe_mode TEXT,
                boot_image TEXT NOT NULL DEFAULT 'windows',
                keep_writeback INTEGER,
                use_game_disk INTEGER,
                chap_user TEXT,
                chap_secret TEXT,
                chap_enabled INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let repository = ClientRepository::new(pool.clone());
        let client = Client::create(CreateClient {
            name: "client-001122334455".to_string(),
            mac: "00:11:22:33:44:55".to_string(),
            ip: "192.168.1.150".to_string(),
            master: "pending".to_string(),
            snapshot: None,
            block_store: None,
            block_device: None,
            target_iqn: None,
            pxe_mode: PxeMode::Uefi,
            boot_image: ClientBootImage::Windows,
            keep_writeback: true,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap_enabled: false,
        })
        .expect("pending client should be valid");

        repository.insert(&client).await.unwrap();
        let found = repository
            .find_by_mac(&client.mac)
            .await
            .unwrap()
            .expect("registered client should be found by MAC");
        assert_eq!(found.id, client.id);
        assert_eq!(found.ip.to_string(), "192.168.1.150");
        assert!(found.target_iqn.is_none());
    }
}
