use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::{path::PathBuf, str::FromStr};

use crate::domain::client::PxeMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallerMediaType {
    WindowsIso,
    WinPe,
    LinuxIso,
    OtherIso,
}

impl InstallerMediaType {
    fn as_str(self) -> &'static str {
        match self {
            Self::WindowsIso => "windows_iso",
            Self::WinPe => "winpe",
            Self::LinuxIso => "linux_iso",
            Self::OtherIso => "other_iso",
        }
    }
}

impl FromStr for InstallerMediaType {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "windows_iso" => Ok(Self::WindowsIso),
            "winpe" => Ok(Self::WinPe),
            "linux_iso" => Ok(Self::LinuxIso),
            "other_iso" => Ok(Self::OtherIso),
            _ => anyhow::bail!("invalid installer media type '{value}'"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralValidationState {
    Pending,
    Valid,
    Invalid,
}

impl StructuralValidationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Valid => "valid",
            Self::Invalid => "invalid",
        }
    }
}

impl FromStr for StructuralValidationState {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "valid" => Ok(Self::Valid),
            "invalid" => Ok(Self::Invalid),
            _ => anyhow::bail!("invalid structural validation state '{value}'"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallerMedia {
    pub id: String,
    pub display_name: String,
    pub media_type: InstallerMediaType,
    pub managed_path: PathBuf,
    pub byte_size: u64,
    pub checksum: String,
    pub validation_state: StructuralValidationState,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct InstallerMediaRepository {
    pool: SqlitePool,
}

type InstallerMediaRow = (
    String,
    String,
    String,
    String,
    i64,
    String,
    String,
    i64,
    String,
    String,
);

impl InstallerMediaRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, media: &InstallerMedia) -> Result<()> {
        sqlx::query("INSERT INTO installer_media (id, display_name, media_type, managed_path, byte_size, checksum, validation_state, is_default, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&media.id)
            .bind(&media.display_name)
            .bind(media.media_type.as_str())
            .bind(media.managed_path.to_string_lossy().as_ref())
            .bind(i64::try_from(media.byte_size).context("installer media size exceeds SQLite range")?)
            .bind(&media.checksum)
            .bind(media.validation_state.as_str())
            .bind(i64::from(media.is_default))
            .bind(media.created_at.to_rfc3339())
            .bind(media.updated_at.to_rfc3339())
            .execute(&self.pool)
            .await
            .context("failed to insert installer media")?;
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<InstallerMedia>> {
        let rows = sqlx::query_as::<_, InstallerMediaRow>("SELECT id, display_name, media_type, managed_path, byte_size, checksum, validation_state, is_default, created_at, updated_at FROM installer_media ORDER BY display_name, id")
            .fetch_all(&self.pool).await?;
        rows.into_iter().map(Self::map_row).collect()
    }

    pub async fn get(&self, id: &str) -> Result<Option<InstallerMedia>> {
        let row = sqlx::query_as::<_, InstallerMediaRow>("SELECT id, display_name, media_type, managed_path, byte_size, checksum, validation_state, is_default, created_at, updated_at FROM installer_media WHERE id = ?")
            .bind(id).fetch_optional(&self.pool).await?;
        row.map(Self::map_row).transpose()
    }

    pub async fn get_default(&self) -> Result<Option<InstallerMedia>> {
        let row = sqlx::query_as::<_, InstallerMediaRow>("SELECT id, display_name, media_type, managed_path, byte_size, checksum, validation_state, is_default, created_at, updated_at FROM installer_media WHERE is_default = 1")
            .fetch_optional(&self.pool).await?;
        row.map(Self::map_row).transpose()
    }

    /// Select the single default atomically; the partial unique index is the final guard.
    pub async fn set_default(&self, id: &str) -> Result<bool> {
        let mut transaction = self.pool.begin().await?;
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM installer_media WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?;
        if exists.is_none() {
            transaction.rollback().await?;
            return Ok(false);
        }
        sqlx::query("UPDATE installer_media SET is_default = 0 WHERE is_default = 1")
            .execute(&mut *transaction)
            .await?;
        let updated =
            sqlx::query("UPDATE installer_media SET is_default = 1, updated_at = ? WHERE id = ?")
                .bind(Utc::now().to_rfc3339())
                .bind(id)
                .execute(&mut *transaction)
                .await?;
        transaction.commit().await?;
        Ok(updated.rows_affected() > 0)
    }

    /// Defaults and media referenced by an unfinished session cannot be removed.
    pub async fn delete_unused(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM installer_media WHERE id = ? AND is_default = 0 AND NOT EXISTS (SELECT 1 FROM super_sessions WHERE installer_media_id = ? AND state != 'completed')")
            .bind(id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn record_boot_verification(&self, record: &BootVerification) -> Result<()> {
        sqlx::query("INSERT INTO installer_media_boot_verifications (id, installer_media_id, checksum, firmware_mode, test_client_id, assigned_image_revision, observed_luns, boot_result, verified_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&record.id)
            .bind(&record.installer_media_id)
            .bind(&record.checksum)
            .bind(record.firmware_mode.as_str())
            .bind(&record.test_client_id)
            .bind(&record.assigned_image_revision)
            .bind(serde_json::to_string(&record.observed_luns)?)
            .bind(&record.boot_result)
            .bind(record.verified_at.to_rfc3339())
            .execute(&self.pool).await.context("failed to record installer-media boot verification")?;
        Ok(())
    }

    pub async fn has_boot_verification(&self, media_id: &str, firmware: &PxeMode) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM installer_media_boot_verifications AS verification JOIN installer_media AS media ON media.id = verification.installer_media_id AND media.checksum = verification.checksum WHERE media.id = ? AND verification.firmware_mode = ? AND verification.boot_result = 'passed'")
            .bind(media_id).bind(firmware.as_str()).fetch_one(&self.pool).await?;
        Ok(count > 0)
    }

    pub async fn list_boot_verifications(&self, media_id: &str) -> Result<Vec<BootVerification>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, String, String, String, String)>("SELECT id, installer_media_id, checksum, firmware_mode, test_client_id, assigned_image_revision, observed_luns, boot_result, verified_at FROM installer_media_boot_verifications WHERE installer_media_id = ? ORDER BY verified_at, id")
            .bind(media_id).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                let (
                    id,
                    installer_media_id,
                    checksum,
                    firmware,
                    test_client_id,
                    assigned_image_revision,
                    luns,
                    boot_result,
                    verified_at,
                ) = row;
                Ok(BootVerification {
                    id,
                    installer_media_id,
                    checksum,
                    firmware_mode: match firmware.as_str() {
                        "uefi" => PxeMode::Uefi,
                        "bios" => PxeMode::Bios,
                        _ => anyhow::bail!("invalid firmware mode '{firmware}'"),
                    },
                    test_client_id,
                    assigned_image_revision,
                    observed_luns: serde_json::from_str(&luns)
                        .context("invalid observed LUN list")?,
                    boot_result,
                    verified_at: parse_datetime(&verified_at)?,
                })
            })
            .collect()
    }

    fn map_row(row: InstallerMediaRow) -> Result<InstallerMedia> {
        let (
            id,
            display_name,
            media_type,
            path,
            byte_size,
            checksum,
            validation_state,
            is_default,
            created_at,
            updated_at,
        ) = row;
        Ok(InstallerMedia {
            id,
            display_name,
            media_type: media_type.parse()?,
            managed_path: PathBuf::from(path),
            byte_size: u64::try_from(byte_size).context("negative installer media size")?,
            checksum,
            validation_state: validation_state.parse()?,
            is_default: is_default != 0,
            created_at: parse_datetime(&created_at)?,
            updated_at: parse_datetime(&updated_at)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootVerification {
    pub id: String,
    pub installer_media_id: String,
    pub checksum: String,
    pub firmware_mode: PxeMode,
    pub test_client_id: String,
    pub assigned_image_revision: String,
    pub observed_luns: Vec<u8>,
    pub boot_result: String,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuperSessionState {
    Preparing,
    Active,
    Saving,
    Discarding,
    RecoveryRequired,
    Completed,
}

impl SuperSessionState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Active => "active",
            Self::Saving => "saving",
            Self::Discarding => "discarding",
            Self::RecoveryRequired => "recovery_required",
            Self::Completed => "completed",
        }
    }

    pub fn allows(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Preparing,
                Self::Active | Self::RecoveryRequired | Self::Completed
            ) | (
                Self::Active,
                Self::Saving | Self::Discarding | Self::RecoveryRequired
            ) | (Self::Saving, Self::RecoveryRequired | Self::Completed)
                | (Self::Discarding, Self::RecoveryRequired | Self::Completed)
                | (
                    Self::RecoveryRequired,
                    Self::Preparing
                        | Self::Active
                        | Self::Saving
                        | Self::Discarding
                        | Self::Completed
                )
        )
    }
}

impl FromStr for SuperSessionState {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "preparing" => Ok(Self::Preparing),
            "active" => Ok(Self::Active),
            "saving" => Ok(Self::Saving),
            "discarding" => Ok(Self::Discarding),
            "recovery_required" => Ok(Self::RecoveryRequired),
            "completed" => Ok(Self::Completed),
            _ => anyhow::bail!("invalid Super-session state '{value}'"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuperSession {
    pub id: String,
    pub client_id: String,
    pub prior_image_id: String,
    pub prior_snapshot: Option<String>,
    /// Exact source snapshot used to create the working clone.
    pub working_origin_snapshot: Option<String>,
    pub working_clone: String,
    pub installer_media_id: Option<String>,
    pub verification_only: bool,
    pub state: SuperSessionState,
    pub resume_state: Option<SuperSessionState>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct SuperSessionRepository {
    pool: SqlitePool,
}

type SuperSessionRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    i64,
    String,
    Option<String>,
    String,
    String,
);

impl SuperSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn insert(&self, session: &SuperSession) -> Result<()> {
        sqlx::query("INSERT INTO super_sessions (id, client_id, prior_image_id, prior_snapshot, working_origin_snapshot, working_clone, installer_media_id, verification_only, state, resume_state, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&session.id).bind(&session.client_id).bind(&session.prior_image_id).bind(&session.prior_snapshot)
            .bind(&session.working_origin_snapshot).bind(&session.working_clone).bind(&session.installer_media_id)
            .bind(i64::from(session.verification_only)).bind(session.state.as_str())
            .bind(session.resume_state.map(SuperSessionState::as_str))
            .bind(session.created_at.to_rfc3339()).bind(session.updated_at.to_rfc3339())
            .execute(&self.pool).await.context("failed to insert Super-session")?;
        Ok(())
    }

    pub async fn get_for_client(&self, client_id: &str) -> Result<Option<SuperSession>> {
        let row = sqlx::query_as::<_, SuperSessionRow>("SELECT id, client_id, prior_image_id, prior_snapshot, working_origin_snapshot, working_clone, installer_media_id, verification_only, state, resume_state, created_at, updated_at FROM super_sessions WHERE client_id = ? ORDER BY (state != 'completed') DESC, created_at DESC, id DESC LIMIT 1")
            .bind(client_id).fetch_optional(&self.pool).await?;
        row.map(Self::map_row).transpose()
    }

    pub async fn list_incomplete(&self) -> Result<Vec<SuperSession>> {
        let rows = sqlx::query_as::<_, SuperSessionRow>("SELECT id, client_id, prior_image_id, prior_snapshot, working_origin_snapshot, working_clone, installer_media_id, verification_only, state, resume_state, created_at, updated_at FROM super_sessions WHERE state != 'completed' ORDER BY created_at, id")
            .fetch_all(&self.pool).await?;
        rows.into_iter().map(Self::map_row).collect()
    }

    pub async fn references_media(&self, media_id: &str) -> Result<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM super_sessions WHERE installer_media_id = ? AND state != 'completed')",
        )
        .bind(media_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    pub async fn session_ids_referencing_media(&self, media_id: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT id FROM super_sessions WHERE installer_media_id = ? ORDER BY id",
        )
        .bind(media_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn restore_completed_media_references(
        &self,
        media_id: &str,
        session_ids: &[String],
    ) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        for session_id in session_ids {
            sqlx::query("UPDATE super_sessions SET installer_media_id = ? WHERE id = ? AND state = 'completed' AND installer_media_id IS NULL")
                .bind(media_id)
                .bind(session_id)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn transition_state(
        &self,
        id: &str,
        expected: SuperSessionState,
        state: SuperSessionState,
    ) -> Result<bool> {
        if !expected.allows(state) {
            return Ok(false);
        }
        let result =
            sqlx::query("UPDATE super_sessions SET state = ?, resume_state = CASE WHEN ? = 'recovery_required' THEN ? ELSE NULL END, updated_at = ? WHERE id = ? AND state = ?")
                .bind(state.as_str())
                .bind(state.as_str())
                .bind(expected.as_str())
                .bind(Utc::now().to_rfc3339())
                .bind(id)
                .bind(expected.as_str())
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_working_clone(
        &self,
        id: &str,
        expected_state: SuperSessionState,
        working_clone: &str,
    ) -> Result<bool> {
        let result = sqlx::query("UPDATE super_sessions SET working_clone = ?, updated_at = ? WHERE id = ? AND state = ?")
            .bind(working_clone)
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .bind(expected_state.as_str())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn activate_with_client_reservation(
        &self,
        id: &str,
        client_id: &str,
    ) -> Result<bool> {
        let mut transaction = self.pool.begin().await?;
        let now = Utc::now().to_rfc3339();
        let session = sqlx::query("UPDATE super_sessions SET state = 'active', resume_state = NULL, updated_at = ? WHERE id = ? AND client_id = ? AND state = 'preparing'")
            .bind(&now).bind(id).bind(client_id).execute(&mut *transaction).await?;
        if session.rows_affected() != 1 {
            transaction.rollback().await?;
            return Ok(false);
        }
        let client = sqlx::query("UPDATE clients SET mode = 'super', updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(client_id)
            .execute(&mut *transaction)
            .await?;
        if client.rows_affected() != 1 {
            transaction.rollback().await?;
            return Ok(false);
        }
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn set_client_boot_mode(&self, client_id: &str, mode: &str) -> Result<bool> {
        anyhow::ensure!(matches!(mode, "normal" | "super"), "invalid boot mode");
        let result = sqlx::query("UPDATE clients SET mode = ?, updated_at = ? WHERE id = ?")
            .bind(mode)
            .bind(Utc::now().to_rfc3339())
            .bind(client_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    fn map_row(row: SuperSessionRow) -> Result<SuperSession> {
        let (
            id,
            client_id,
            prior_image_id,
            prior_snapshot,
            working_origin_snapshot,
            working_clone,
            installer_media_id,
            verification_only,
            state,
            resume_state,
            created_at,
            updated_at,
        ) = row;
        Ok(SuperSession {
            id,
            client_id,
            prior_image_id,
            prior_snapshot,
            working_origin_snapshot,
            working_clone,
            installer_media_id,
            verification_only: verification_only != 0,
            state: state.parse()?,
            resume_state: resume_state.map(|value| value.parse()).transpose()?,
            created_at: parse_datetime(&created_at)?,
            updated_at: parse_datetime(&updated_at)?,
        })
    }
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::{
        BootVerification, InstallerMedia, InstallerMediaRepository, InstallerMediaType,
        StructuralValidationState, SuperSession, SuperSessionRepository, SuperSessionState,
    };
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;

    async fn database() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn seed_client_and_image(pool: &sqlx::SqlitePool) {
        sqlx::query("INSERT INTO clients (id, name, mac, ip, master, created_at, updated_at) VALUES ('client-1', 'Client', '00:11:22:33:44:55', '192.0.2.1', 'image-1', 'now', 'now')")
            .execute(pool).await.unwrap();
        sqlx::query("INSERT INTO images (id, name, os_type, size_gb, path, created_at, updated_at) VALUES ('image-1', 'Image', 'linux', 20, '/tmp/image', 'now', 'now')")
            .execute(pool).await.unwrap();
    }

    fn media(id: &str, media_type: InstallerMediaType, is_default: bool) -> InstallerMedia {
        let now = Utc::now();
        InstallerMedia {
            id: id.to_string(),
            display_name: id.to_string(),
            media_type,
            managed_path: PathBuf::from(format!("/managed/{id}.iso")),
            byte_size: 1024,
            checksum: format!("sha-{id}"),
            validation_state: StructuralValidationState::Valid,
            is_default,
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn boot_verification_is_append_only_and_matches_checksum_and_firmware() {
        let pool = database().await;
        let repository = InstallerMediaRepository::new(pool);
        repository
            .insert(&media("linux-a", InstallerMediaType::LinuxIso, false))
            .await
            .unwrap();
        let verification = BootVerification {
            id: "verify-1".into(),
            installer_media_id: "linux-a".into(),
            checksum: "sha-linux-a".into(),
            firmware_mode: crate::domain::client::PxeMode::Uefi,
            test_client_id: "test-client".into(),
            assigned_image_revision: "image@rev-1".into(),
            observed_luns: vec![0, 1],
            boot_result: "passed".into(),
            verified_at: Utc::now(),
        };
        repository
            .record_boot_verification(&verification)
            .await
            .unwrap();

        assert!(repository
            .has_boot_verification("linux-a", &crate::domain::client::PxeMode::Uefi)
            .await
            .unwrap());
        assert!(!repository
            .has_boot_verification("linux-a", &crate::domain::client::PxeMode::Bios)
            .await
            .unwrap());
        sqlx::query("UPDATE installer_media SET checksum = 'sha-current' WHERE id = 'linux-a'")
            .execute(&repository.pool)
            .await
            .unwrap();
        assert!(!repository
            .has_boot_verification("linux-a", &crate::domain::client::PxeMode::Uefi)
            .await
            .unwrap());
        assert_eq!(
            repository
                .list_boot_verifications("linux-a")
                .await
                .unwrap()
                .len(),
            1
        );
        let pool = &repository.pool;
        assert!(sqlx::query(
            "DELETE FROM installer_media_boot_verifications WHERE id = 'verify-1'"
        )
        .execute(pool)
        .await
        .is_err());
    }

    #[tokio::test]
    async fn one_global_default_can_be_changed_across_media_types() {
        let pool = database().await;
        let repository = InstallerMediaRepository::new(pool);
        repository
            .insert(&media("win-a", InstallerMediaType::WinPe, true))
            .await
            .unwrap();
        repository
            .insert(&media("linux-a", InstallerMediaType::LinuxIso, false))
            .await
            .unwrap();
        assert!(repository
            .insert(&media("win-b", InstallerMediaType::WindowsIso, true))
            .await
            .is_err());

        assert!(repository.set_default("linux-a").await.unwrap());
        assert_eq!(
            repository.get_default().await.unwrap().unwrap().id,
            "linux-a"
        );
        let previous_is_default: i64 =
            sqlx::query_scalar("SELECT is_default FROM installer_media WHERE id = 'win-a'")
                .fetch_one(&repository.pool)
                .await
                .unwrap();
        assert_eq!(previous_is_default, 0);
    }

    #[tokio::test]
    async fn session_records_round_trip_and_remain_resumable() {
        let pool = database().await;
        seed_client_and_image(&pool).await;
        let media_repository = InstallerMediaRepository::new(pool.clone());
        media_repository
            .insert(&media("linux-a", InstallerMediaType::LinuxIso, true))
            .await
            .unwrap();
        let repository = SuperSessionRepository::new(pool);
        let now = Utc::now();
        let session = SuperSession {
            id: "session-1".into(),
            client_id: "client-1".into(),
            prior_image_id: "image-1".into(),
            prior_snapshot: Some("base@v1".into()),
            working_origin_snapshot: Some("base@v1".into()),
            working_clone: "pool/clients/client-1/super-session-1".into(),
            installer_media_id: Some("linux-a".into()),
            verification_only: true,
            state: SuperSessionState::Active,
            resume_state: None,
            created_at: now,
            updated_at: now,
        };
        repository.insert(&session).await.unwrap();
        repository
            .transition_state(
                "session-1",
                SuperSessionState::Active,
                SuperSessionState::Saving,
            )
            .await
            .unwrap();

        let resumed = repository
            .get_for_client("client-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resumed.id, "session-1");
        assert_eq!(resumed.state, SuperSessionState::Saving);
        assert_eq!(resumed.prior_snapshot.as_deref(), Some("base@v1"));
        assert_eq!(resumed.working_origin_snapshot.as_deref(), Some("base@v1"));
        assert!(resumed.verification_only);
        let duplicate = SuperSession {
            id: "session-2".into(),
            working_clone: "another-clone".into(),
            ..session
        };
        assert!(repository.insert(&duplicate).await.is_err());
    }

    #[tokio::test]
    async fn completed_session_does_not_prevent_a_later_session_for_same_client() {
        let pool = database().await;
        seed_client_and_image(&pool).await;
        InstallerMediaRepository::new(pool.clone())
            .insert(&media("linux-a", InstallerMediaType::LinuxIso, false))
            .await
            .unwrap();
        let repository = SuperSessionRepository::new(pool);
        let now = Utc::now();
        let first = SuperSession {
            id: "session-1".into(),
            client_id: "client-1".into(),
            prior_image_id: "image-1".into(),
            prior_snapshot: None,
            working_origin_snapshot: None,
            working_clone: "clone-1".into(),
            installer_media_id: Some("linux-a".into()),
            verification_only: false,
            state: SuperSessionState::Completed,
            resume_state: None,
            created_at: now,
            updated_at: now,
        };
        repository.insert(&first).await.unwrap();
        let second = SuperSession {
            id: "session-2".into(),
            working_clone: "clone-2".into(),
            state: SuperSessionState::Preparing,
            ..first
        };
        repository.insert(&second).await.unwrap();
        assert_eq!(
            repository
                .get_for_client("client-1")
                .await
                .unwrap()
                .unwrap()
                .id,
            "session-2"
        );
    }

    #[tokio::test]
    async fn invalid_session_transition_does_not_overwrite_active_state() {
        let pool = database().await;
        seed_client_and_image(&pool).await;
        InstallerMediaRepository::new(pool.clone())
            .insert(&media("linux-a", InstallerMediaType::LinuxIso, false))
            .await
            .unwrap();
        let repository = SuperSessionRepository::new(pool);
        let now = Utc::now();
        repository
            .insert(&SuperSession {
                id: "session-1".into(),
                client_id: "client-1".into(),
                prior_image_id: "image-1".into(),
                prior_snapshot: None,
                working_origin_snapshot: None,
                working_clone: "clone-1".into(),
                installer_media_id: Some("linux-a".into()),
                verification_only: false,
                state: SuperSessionState::Active,
                resume_state: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        assert!(!repository
            .transition_state(
                "session-1",
                SuperSessionState::Active,
                SuperSessionState::Completed
            )
            .await
            .unwrap());
        assert_eq!(
            repository
                .get_for_client("client-1")
                .await
                .unwrap()
                .unwrap()
                .state,
            SuperSessionState::Active
        );
        assert!(repository
            .transition_state(
                "session-1",
                SuperSessionState::Active,
                SuperSessionState::RecoveryRequired
            )
            .await
            .unwrap());
        let recovery = repository
            .get_for_client("client-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(recovery.resume_state, Some(SuperSessionState::Active));
    }

    #[tokio::test]
    async fn activation_publishes_session_and_client_mode_atomically() {
        let pool = database().await;
        seed_client_and_image(&pool).await;
        InstallerMediaRepository::new(pool.clone())
            .insert(&media("linux-a", InstallerMediaType::LinuxIso, false))
            .await
            .unwrap();
        let repository = SuperSessionRepository::new(pool.clone());
        let now = Utc::now();
        repository
            .insert(&SuperSession {
                id: "session-1".into(),
                client_id: "client-1".into(),
                prior_image_id: "image-1".into(),
                prior_snapshot: None,
                working_origin_snapshot: Some("pool/image@source".into()),
                working_clone: "clone-1".into(),
                installer_media_id: Some("linux-a".into()),
                verification_only: false,
                state: SuperSessionState::Preparing,
                resume_state: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        assert!(repository
            .activate_with_client_reservation("session-1", "client-1")
            .await
            .unwrap());
        assert!(!repository
            .activate_with_client_reservation("session-1", "client-1")
            .await
            .unwrap());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT mode FROM clients WHERE id = 'client-1'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "super"
        );
        assert_eq!(
            repository
                .get_for_client("client-1")
                .await
                .unwrap()
                .unwrap()
                .state,
            SuperSessionState::Active
        );
    }

    #[test]
    fn media_type_codes_are_stable() {
        for (code, expected) in [
            ("windows_iso", InstallerMediaType::WindowsIso),
            ("winpe", InstallerMediaType::WinPe),
            ("linux_iso", InstallerMediaType::LinuxIso),
            ("other_iso", InstallerMediaType::OtherIso),
        ] {
            assert_eq!(code.parse::<InstallerMediaType>().unwrap(), expected);
        }
        assert!("linux_live".parse::<InstallerMediaType>().is_err());
    }

    #[tokio::test]
    async fn migration_maps_legacy_media_and_clears_ambiguous_defaults() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE installer_media (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                media_type TEXT NOT NULL CHECK (media_type IN ('winpe', 'linux_live')),
                managed_path TEXT NOT NULL UNIQUE,
                byte_size INTEGER NOT NULL CHECK (byte_size > 0),
                checksum TEXT NOT NULL,
                validation_state TEXT NOT NULL CHECK (validation_state IN ('pending', 'valid', 'invalid')),
                is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE UNIQUE INDEX idx_installer_media_default_type ON installer_media(media_type) WHERE is_default = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE client_media_refs (id TEXT PRIMARY KEY, media_id TEXT REFERENCES installer_media(id))")
            .execute(&pool)
            .await
            .unwrap();
        for (id, media_type, is_default) in [
            ("win-a", "winpe", 1),
            ("linux-a", "linux_live", 1),
            ("linux-b", "linux_live", 0),
        ] {
            sqlx::query("INSERT INTO installer_media (id, display_name, media_type, managed_path, byte_size, checksum, validation_state, is_default, created_at, updated_at) VALUES (?, ?, ?, ?, 1024, 'sha', 'valid', ?, 'now', 'now')")
                .bind(id)
                .bind(id)
                .bind(media_type)
                .bind(format!("/managed/{id}.iso"))
                .bind(is_default)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO client_media_refs (id, media_id) VALUES ('ref-1', 'linux-a')")
            .execute(&pool)
            .await
            .unwrap();

        let mut transaction = pool.begin().await.unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/0022_installer_media_types.sql"
        ))
        .execute(&mut *transaction)
        .await
        .unwrap();
        transaction.rollback().await.unwrap();
        let original_type: String =
            sqlx::query_scalar("SELECT media_type FROM installer_media WHERE id = 'linux-a'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            original_type, "linux_live",
            "migration must roll back atomically"
        );

        let mut transaction = pool.begin().await.unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/0022_installer_media_types.sql"
        ))
        .execute(&mut *transaction)
        .await
        .unwrap();
        transaction.commit().await.unwrap();

        let rows: Vec<(String, String, i64)> =
            sqlx::query_as("SELECT id, media_type, is_default FROM installer_media ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![
                ("linux-a".into(), "linux_iso".into(), 0),
                ("linux-b".into(), "linux_iso".into(), 0),
                ("win-a".into(), "winpe".into(), 0),
            ]
        );
        let refs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM client_media_refs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(refs, 1);
        let violations = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(violations.is_empty());
        assert!(sqlx::query("INSERT INTO installer_media (id, display_name, media_type, managed_path, byte_size, checksum, validation_state, is_default, created_at, updated_at) VALUES ('old', 'old', 'linux_live', '/managed/old.iso', 1024, 'sha', 'valid', 0, 'now', 'now')")
            .execute(&pool)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn repositories_enforce_client_image_and_media_foreign_keys() {
        let pool = database().await;
        seed_client_and_image(&pool).await;
        InstallerMediaRepository::new(pool.clone())
            .insert(&media("media-1", InstallerMediaType::LinuxIso, false))
            .await
            .unwrap();
        let repository = SuperSessionRepository::new(pool);
        let now = Utc::now();
        for (id, client_id, prior_image_id, installer_media_id) in [
            ("bad-client", "missing-client", "image-1", "media-1"),
            ("bad-image", "client-1", "missing-image", "media-1"),
            ("bad-media", "client-1", "image-1", "missing-media"),
        ] {
            let result = repository
                .insert(&SuperSession {
                    id: id.into(),
                    client_id: client_id.into(),
                    prior_image_id: prior_image_id.into(),
                    prior_snapshot: None,
                    working_origin_snapshot: None,
                    working_clone: format!("clone-{id}"),
                    installer_media_id: Some(installer_media_id.into()),
                    verification_only: false,
                    state: SuperSessionState::Active,
                    resume_state: None,
                    created_at: now,
                    updated_at: now,
                })
                .await;
            assert!(result.is_err(), "foreign key should reject {id}");
        }
    }
}
