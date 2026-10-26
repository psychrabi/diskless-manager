//! Durable installation ownership and append-only progress. Tokens never serialize in management responses.
use anyhow::{bail, Result};
use chrono::Utc;
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

#[derive(Clone, FromRow, Serialize)]
pub struct InstallationSession {
    pub id: String,
    pub client_id: String,
    pub media_id: String,
    pub image_id: String,
    pub master: String,
    pub media_checksum: String,
    pub target_iqn: String,
    pub expected_bytes: i64,
    pub source_ip: String,
    pub firmware: String,
    pub recipe: String,
    #[serde(skip)]
    pub token: String,
    #[serde(skip)]
    pub config_json: String,
    pub state: String,
    pub progress: i64,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
}

#[derive(FromRow, Serialize)]
pub struct InstallationEvent {
    pub sequence: i64,
    pub state: String,
    pub progress: i64,
    pub message: String,
    pub created_at: String,
}

#[derive(Clone)]
pub struct InstallationRepository {
    pool: SqlitePool,
}
impl InstallationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    pub async fn get(&self, id: &str) -> Result<Option<InstallationSession>> {
        Ok(
            sqlx::query_as("SELECT * FROM installation_sessions WHERE id=?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
    pub async fn active(&self, client: &str) -> Result<Option<InstallationSession>> {
        Ok(sqlx::query_as(
            "SELECT * FROM installation_sessions WHERE client_id=? AND state != 'completed'",
        )
        .bind(client)
        .fetch_optional(&self.pool)
        .await?)
    }
    pub async fn list(&self, client: &str) -> Result<Vec<InstallationSession>> {
        Ok(sqlx::query_as(
            "SELECT * FROM installation_sessions WHERE client_id=? ORDER BY created_at DESC",
        )
        .bind(client)
        .fetch_all(&self.pool)
        .await?)
    }
    pub async fn events(&self, id: &str) -> Result<Vec<InstallationEvent>> {
        Ok(sqlx::query_as("SELECT sequence,state,progress,message,created_at FROM installation_events WHERE session_id=? ORDER BY sequence DESC LIMIT 200").bind(id).fetch_all(&self.pool).await?)
    }
    pub async fn insert(&self, s: &InstallationSession) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO installation_sessions (id,client_id,media_id,image_id,master,media_checksum,target_iqn,expected_bytes,source_ip,firmware,recipe,token,config_json,state,progress,error,created_at,updated_at,expires_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&s.id).bind(&s.client_id).bind(&s.media_id).bind(&s.image_id).bind(&s.master).bind(&s.media_checksum).bind(&s.target_iqn).bind(s.expected_bytes).bind(&s.source_ip).bind(&s.firmware).bind(&s.recipe).bind(&s.token).bind(&s.config_json).bind(&s.state).bind(s.progress).bind(&s.error).bind(&s.created_at).bind(&s.updated_at).bind(&s.expires_at).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,created_at) VALUES (?, 'ready', 0, 'Installation prepared; existing disk contents may be replaced', ?)").bind(&s.id).bind(&s.created_at).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    /// CAS keeps stale concurrent reports from reverting progress or resurrecting a cancelled operation.
    pub async fn transition(
        &self,
        s: &InstallationSession,
        state: &str,
        progress: i64,
        message: &str,
    ) -> Result<()> {
        let allowed = match s.state.as_str() {
            "ready" => matches!(
                state,
                "booting" | "cancelled" | "failed" | "recovery_required"
            ),
            "booting" => matches!(
                state,
                "booting" | "installing" | "failed" | "cancelled" | "recovery_required"
            ),
            "installing" => matches!(
                state,
                "installing" | "completed" | "failed" | "cancelled" | "recovery_required"
            ),
            "failed" | "cancelled" | "recovery_required" => state == "completed",
            _ => false,
        };
        if !allowed
            || !(0..=100).contains(&progress)
            || progress < s.progress
            || message.len() > 1024
        {
            bail!("invalid installation transition or progress");
        }
        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool.begin().await?;
        let changed=sqlx::query("UPDATE installation_sessions SET state=?, progress=?,error=?,updated_at=? WHERE id=? AND state=? AND progress=? AND updated_at=?")
            .bind(state).bind(progress).bind(if matches!(state,"failed"|"recovery_required") {Some(message)} else {None}).bind(&now).bind(&s.id).bind(&s.state).bind(s.progress).bind(&s.updated_at).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            bail!("installation session changed; refresh before retrying");
        }
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,created_at) VALUES (?,?,?,?,?)").bind(&s.id).bind(state).bind(progress).bind(message).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Boot/report credentials are revoked after restart until explicit administrator recovery.
    pub async fn recover_interrupted(&self) -> Result<u64> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,created_at) SELECT id,'recovery_required',progress,'Server restarted; verify client is powered off before closing recovery',? FROM installation_sessions WHERE state IN ('ready','booting','installing')").bind(&now).execute(&mut *tx).await?;
        let n=sqlx::query("UPDATE installation_sessions SET state='recovery_required',error='Server restarted during installation',updated_at=? WHERE state IN ('ready','booting','installing')").bind(now).execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn database() -> InstallationRepository {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql("PRAGMA foreign_keys=ON; CREATE TABLE clients(id TEXT PRIMARY KEY); CREATE TABLE images(id TEXT PRIMARY KEY); CREATE TABLE installer_media(id TEXT PRIMARY KEY); INSERT INTO clients VALUES ('client-a'),('client-b'); INSERT INTO images VALUES('image-a'); INSERT INTO installer_media VALUES('media-a');").execute(&pool).await.unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/0023_installer_sessions.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        InstallationRepository::new(pool)
    }
    fn session() -> InstallationSession {
        InstallationSession {
            id: "session-a".into(),
            client_id: "client-a".into(),
            media_id: "media-a".into(),
            image_id: "image-a".into(),
            master: "pool/image-a".into(),
            media_checksum: "checksum-a".into(),
            target_iqn: "iqn.example:client-a".into(),
            expected_bytes: 20 * 1024 * 1024 * 1024,
            source_ip: "192.0.2.10".into(),
            firmware: "uefi".into(),
            recipe: "ubuntu".into(),
            token: "secret-token".into(),
            config_json: "{\"password_hash\":\"secret-hash\"}".into(),
            state: "ready".into(),
            progress: 0,
            error: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            expires_at: (Utc::now() + chrono::Duration::hours(24)).to_rfc3339(),
        }
    }
    #[tokio::test]
    async fn cancellation_retains_ownership_and_stale_reports_cannot_revive_session() {
        let r = database().await;
        let s = session();
        r.insert(&s).await.unwrap();
        r.transition(&s, "booting", 0, "boot").await.unwrap();
        assert!(r
            .transition(&s, "failed", 0, "stale failure")
            .await
            .is_err());
        let s = r.get(&s.id).await.unwrap().unwrap();
        r.transition(&s, "installing", 30, "partitioning")
            .await
            .unwrap();
        let s = r.get(&s.id).await.unwrap().unwrap();
        assert!(r
            .transition(&s, "installing", 20, "reordered report")
            .await
            .is_err());
        r.transition(&s, "cancelled", 30, "stop").await.unwrap();
        let cancelled = r.active("client-a").await.unwrap().unwrap();
        assert!(r
            .transition(&cancelled, "installing", 40, "late report")
            .await
            .is_err());
        let next = InstallationSession {
            id: "session-b".into(),
            ..session()
        };
        assert!(r.insert(&next).await.is_err());
        r.transition(
            &cancelled,
            "completed",
            100,
            "offline administrator recovery",
        )
        .await
        .unwrap();
        r.insert(&next).await.unwrap();
        assert_eq!(r.events("session-a").await.unwrap().len(), 5);
    }
    #[tokio::test]
    async fn restart_revokes_boot_state_and_preserves_events_and_foreign_keys() {
        let r = database().await;
        let s = session();
        r.insert(&s).await.unwrap();
        assert_eq!(r.recover_interrupted().await.unwrap(), 1);
        assert_eq!(r.recover_interrupted().await.unwrap(), 0);
        let recovered = r.get(&s.id).await.unwrap().unwrap();
        assert_eq!(recovered.state, "recovery_required");
        assert_eq!(recovered.media_checksum, s.media_checksum);
        assert_eq!(r.events(&s.id).await.unwrap().len(), 2);
        assert!(
            sqlx::query("DELETE FROM installer_media WHERE id='media-a'")
                .execute(&r.pool)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("UPDATE installation_events SET message='changed'")
                .execute(&r.pool)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn unfinished_sessions_block_owner_deletion_but_completed_history_cascades_with_client() {
        let r = database().await;
        let s = session();
        r.insert(&s).await.unwrap();
        assert!(sqlx::query("DELETE FROM clients WHERE id='client-a'")
            .execute(&r.pool)
            .await
            .is_err());
        r.transition(&s, "cancelled", 0, "stop").await.unwrap();
        let cancelled = r.get(&s.id).await.unwrap().unwrap();
        r.transition(&cancelled, "completed", 100, "offline recovery")
            .await
            .unwrap();
        sqlx::query("DELETE FROM clients WHERE id='client-a'")
            .execute(&r.pool)
            .await
            .unwrap();
        assert!(r.get(&s.id).await.unwrap().is_none());
        assert!(r.events(&s.id).await.unwrap().is_empty());
    }
    #[test]
    fn management_serialization_omits_boot_credentials_and_linux_password_hash() {
        let value = serde_json::to_string(&session()).unwrap();
        assert!(!value.contains("secret-token"));
        assert!(!value.contains("secret-hash"));
        assert!(!value.contains("config_json"));
    }
}
