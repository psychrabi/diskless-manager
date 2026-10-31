//! Durable installation ownership and append-only progress. Tokens never serialize in management responses.
use anyhow::{bail, Result};
use chrono::Utc;
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

#[derive(Clone, FromRow, Serialize, Default)]
pub struct InstallationSession {
    pub id: String,
    pub client_id: String,
    pub kind: String,
    pub recipe: String,
    pub media_id: String,
    pub media_checksum: String,
    pub image_name: Option<String>,
    pub draft_image_id: Option<String>,
    pub size_gb: Option<i64>,
    pub expected_bytes: Option<i64>,
    #[serde(skip)]
    pub prior_master: Option<String>,
    #[serde(skip)]
    pub prior_snapshot: Option<String>,
    #[serde(skip)]
    pub target_iqn: String,
    #[serde(skip)]
    pub source_ip: String,
    pub firmware: String,
    #[serde(skip)]
    pub verification_target_iqn: Option<String>,
    pub verification_image_id: Option<String>,
    #[serde(skip)]
    pub verification_snapshot: Option<String>,
    pub attempt: i64,
    pub published_image_id: Option<String>,
    pub assign_on_ready: bool,
    pub assignment_state: String,
    pub assignment_error: Option<String>,
    pub phase_deadline: Option<String>,
    pub cleanup_state: String,
    pub legacy: bool,
    pub closed_at: Option<String>,
    #[serde(skip)]
    pub token: String,
    #[serde(skip)]
    pub config_json: String,
    pub state: String,
    pub progress: i64,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip)]
    pub image_id: String,
    #[serde(skip)]
    pub master: String,
    #[serde(skip)]
    pub expires_at: String,
}

#[derive(FromRow, Serialize)]
pub struct InstallationEvent {
    pub sequence: i64,
    pub state: String,
    pub progress: i64,
    pub message: String,
    pub actor: Option<String>,
    pub created_at: String,
}

#[derive(Clone)]
pub struct InstallationRepository {
    pool: SqlitePool,
}

const SESSION_SELECT: &str = "SELECT *, COALESCE(draft_image_id,published_image_id,'') AS image_id, COALESCE(prior_master,'') AS master, COALESCE(phase_deadline,'') AS expires_at FROM installation_sessions";

impl InstallationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    pub async fn get(&self, id: &str) -> Result<Option<InstallationSession>> {
        Ok(sqlx::query_as(&format!("{SESSION_SELECT} WHERE id=?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }
    pub async fn verification_target(
        &self,
        target_iqn: &str,
    ) -> Result<Option<InstallationSession>> {
        Ok(sqlx::query_as(&format!("{SESSION_SELECT} WHERE verification_target_iqn=? AND state IN ('verifying','published') AND legacy=0"))
            .bind(target_iqn).fetch_optional(&self.pool).await?)
    }
    pub async fn active(&self, client: &str) -> Result<Option<InstallationSession>> {
        Ok(sqlx::query_as(&format!(
            "{SESSION_SELECT} WHERE client_id=? AND closed_at IS NULL"
        ))
        .bind(client)
        .fetch_optional(&self.pool)
        .await?)
    }
    pub async fn list(&self, client: &str) -> Result<Vec<InstallationSession>> {
        Ok(sqlx::query_as(&format!(
            "{SESSION_SELECT} WHERE client_id=? ORDER BY created_at DESC"
        ))
        .bind(client)
        .fetch_all(&self.pool)
        .await?)
    }
    pub async fn saving_sessions(&self) -> Result<Vec<InstallationSession>> {
        Ok(sqlx::query_as(&format!(
            "{SESSION_SELECT} WHERE state='saving' AND closed_at IS NULL AND legacy=0"
        ))
        .fetch_all(&self.pool)
        .await?)
    }
    pub async fn published_sessions(&self) -> Result<Vec<InstallationSession>> {
        Ok(sqlx::query_as(&format!(
            "{SESSION_SELECT} WHERE state='published' AND closed_at IS NULL AND legacy=0"
        ))
        .fetch_all(&self.pool)
        .await?)
    }
    pub async fn expired_sessions(&self) -> Result<Vec<InstallationSession>> {
        Ok(sqlx::query_as(&format!("{SESSION_SELECT} WHERE closed_at IS NULL AND legacy=0 AND phase_deadline IS NOT NULL AND phase_deadline<=? AND state IN ('preparing','waiting','installing','adapting','saving','verifying','manual')"))
            .bind(Utc::now().to_rfc3339()).fetch_all(&self.pool).await?)
    }
    pub async fn finish_published_cleanup(
        &self,
        s: &InstallationSession,
        assignment_error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let assignment_state = if !s.assign_on_ready {
            "not_requested"
        } else if assignment_error.is_some() {
            "failed"
        } else {
            "applied"
        };
        let error = assignment_error.map(|message| message.chars().take(512).collect::<String>());
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE installation_sessions SET cleanup_state='complete',closed_at=?,assignment_state=?,assignment_error=?,updated_at=? WHERE id=? AND state='published' AND closed_at IS NULL AND updated_at=?")
            .bind(&now).bind(assignment_state).bind(error).bind(&now).bind(&s.id).bind(&s.updated_at).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            bail!("published cleanup changed; refresh before releasing session ownership");
        }
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) VALUES (?,'published',100,?,'server',?)")
            .bind(&s.id).bind(if assignment_error.is_some() { "Published image cleanup completed; requested assignment failed and requires administrator action" } else { "Published image cleanup completed" }).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Opt-in assignment after publication. Only the owning published session may
    /// move the client's boot assignment while it still reserves the client.
    pub async fn assign_published_image(&self, session_id: &str, image_name: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let block_device = format!("/dev/zvol/{image_name}");
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE clients SET master=?,snapshot=NULL,writeback=NULL,block_device=?,updated_at=? WHERE id=(SELECT client_id FROM installation_sessions WHERE id=? AND state='published' AND closed_at IS NULL AND assign_on_ready=1 AND assignment_state='pending')")
            .bind(image_name).bind(&block_device).bind(&now).bind(session_id).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            bail!("published image assignment changed; refresh before assigning");
        }
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) VALUES (?,'published',100,'Published image assigned to the client','server',?)")
            .bind(session_id).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn record_published_cleanup_error(&self, id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE installation_sessions SET error='Published image cleanup failed; resources remain reserved. Retry cleanup after the client is powered off.',updated_at=? WHERE id=? AND state='published' AND closed_at IS NULL")
            .bind(&now).bind(id).execute(&self.pool).await?;
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) SELECT id,state,progress,'Published cleanup step failed; session ownership remains reserved','server',? FROM installation_sessions WHERE id=? AND state='published' AND closed_at IS NULL")
            .bind(now).bind(id).execute(&self.pool).await?;
        Ok(())
    }
    /// Record a cleanup failure on a stopped session without changing its
    /// state. A transition to `recovery_required` is rejected when the session
    /// is already in that state, and that rejection would otherwise replace
    /// the real cleanup error with an unrelated transition error.
    pub async fn record_cleanup_failure(&self, id: &str, message: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let message = message.chars().take(1024).collect::<String>();
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE installation_sessions SET error=?,cleanup_state='failed',updated_at=? WHERE id=? AND closed_at IS NULL")
            .bind(&message)
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) SELECT id,state,progress,?,'server',? FROM installation_sessions WHERE id=? AND closed_at IS NULL")
            .bind(&message)
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn events(&self, id: &str) -> Result<Vec<InstallationEvent>> {
        Ok(sqlx::query_as("SELECT sequence,state,progress,message,actor,created_at FROM installation_events WHERE session_id=? ORDER BY sequence DESC LIMIT 200").bind(id).fetch_all(&self.pool).await?)
    }
    /// Append a bounded, secret-free event without changing session state.
    pub async fn record_event(
        &self,
        s: &InstallationSession,
        message: &str,
        actor: &str,
    ) -> Result<()> {
        let message = message.chars().take(1024).collect::<String>();
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) SELECT id,state,progress,?,?,? FROM installation_sessions WHERE id=? AND closed_at IS NULL")
            .bind(message)
            .bind(actor)
            .bind(Utc::now().to_rfc3339())
            .bind(&s.id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn insert(&self, s: &InstallationSession) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO installation_sessions (id,client_id,kind,recipe,media_id,media_checksum,image_name,draft_image_id,size_gb,expected_bytes,prior_master,prior_snapshot,target_iqn,source_ip,firmware,verification_target_iqn,verification_image_id,verification_snapshot,attempt,published_image_id,assign_on_ready,assignment_state,assignment_error,state,progress,error,phase_deadline,cleanup_state,legacy,closed_at,token,config_json,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&s.id).bind(&s.client_id).bind(&s.kind).bind(&s.recipe).bind(&s.media_id).bind(&s.media_checksum).bind(&s.image_name).bind(&s.draft_image_id).bind(s.size_gb).bind(s.expected_bytes).bind(&s.prior_master).bind(&s.prior_snapshot).bind(&s.target_iqn).bind(&s.source_ip).bind(&s.firmware).bind(&s.verification_target_iqn).bind(&s.verification_image_id).bind(&s.verification_snapshot).bind(s.attempt).bind(&s.published_image_id).bind(s.assign_on_ready).bind(&s.assignment_state).bind(&s.assignment_error).bind(&s.state).bind(s.progress).bind(&s.error).bind(&s.phase_deadline).bind(&s.cleanup_state).bind(s.legacy).bind(&s.closed_at).bind(&s.token).bind(&s.config_json).bind(&s.created_at).bind(&s.updated_at).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,created_at) VALUES (?,?,?,?,?)").bind(&s.id).bind(&s.state).bind(s.progress).bind("Installation session reserved").bind(&s.created_at).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn set_draft(&self, id: &str, image_id: &str, bytes: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query("UPDATE installation_sessions SET draft_image_id=?,expected_bytes=?,updated_at=? WHERE id=? AND state='preparing' AND draft_image_id IS NULL")
            .bind(image_id).bind(bytes).bind(now).bind(id).execute(&self.pool).await?.rows_affected();
        if changed != 1 {
            bail!("installation preparation changed before draft ownership was recorded");
        }
        Ok(())
    }
    pub async fn set_expected_bytes(&self, id: &str, image_id: &str, bytes: i64) -> Result<()> {
        let changed = sqlx::query("UPDATE installation_sessions SET expected_bytes=?,updated_at=? WHERE id=? AND state='preparing' AND draft_image_id=?")
            .bind(bytes).bind(Utc::now().to_rfc3339()).bind(id).bind(image_id).execute(&self.pool).await?.rows_affected();
        if changed != 1 {
            bail!("installation draft changed before its exported size was recorded");
        }
        Ok(())
    }
    pub async fn clear_draft(&self, id: &str, image_id: &str) -> Result<()> {
        let changed = sqlx::query("UPDATE installation_sessions SET draft_image_id=NULL,expected_bytes=NULL,updated_at=? WHERE id=? AND state='preparing' AND draft_image_id=?")
            .bind(Utc::now().to_rfc3339()).bind(id).bind(image_id).execute(&self.pool).await?.rows_affected();
        if changed != 1 {
            bail!("installation draft ownership changed during rollback");
        }
        Ok(())
    }
    pub async fn fail_preparation(
        &self,
        id: &str,
        message: &str,
        cleanup_complete: bool,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let state = if cleanup_complete {
            "failed"
        } else {
            "recovery_required"
        };
        let closed_at = cleanup_complete.then_some(now.as_str());
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE installation_sessions SET state=?,error=?,cleanup_state=?,closed_at=?,token='',updated_at=? WHERE id=? AND state='preparing'")
            .bind(state).bind(message.chars().take(1024).collect::<String>()).bind(if cleanup_complete {"complete"} else {"failed"}).bind(closed_at).bind(&now).bind(id).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            bail!("installation preparation changed before failure was recorded");
        }
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) VALUES (?, ?, 0, ?, 'server', ?)")
            .bind(id).bind(state).bind("Installation preparation failed; review recovery state").bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn record_verification_snapshot(
        &self,
        s: &InstallationSession,
        snapshot: &str,
    ) -> Result<()> {
        let now = Utc::now();
        let deadline = (now + chrono::Duration::minutes(30)).to_rfc3339();
        let now = now.to_rfc3339();
        let changed = sqlx::query("UPDATE installation_sessions SET verification_snapshot=?,attempt=attempt+1,phase_deadline=?,updated_at=? WHERE id=? AND state='saving' AND updated_at=?")
            .bind(snapshot).bind(&deadline).bind(&now).bind(&s.id).bind(&s.updated_at).execute(&self.pool).await?.rows_affected();
        if changed != 1 {
            bail!("installation save changed before verification snapshot was recorded");
        }
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) VALUES (?,'saving',90,'Draft verification snapshot created','server',?)")
            .bind(&s.id).bind(now).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn record_verification_candidate(
        &self,
        s: &InstallationSession,
        image_id: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query("UPDATE installation_sessions SET verification_image_id=?,updated_at=? WHERE id=? AND state='saving' AND verification_snapshot IS NOT NULL AND updated_at=?")
            .bind(image_id).bind(now).bind(&s.id).bind(&s.updated_at).execute(&self.pool).await?.rows_affected();
        if changed != 1 {
            bail!("installation save changed before candidate ownership was recorded");
        }
        Ok(())
    }
    pub async fn set_verification_target(
        &self,
        s: &InstallationSession,
        target: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let changed = sqlx::query("UPDATE installation_sessions SET verification_target_iqn=?,updated_at=? WHERE id=? AND state='saving' AND verification_image_id IS NOT NULL AND verification_target_iqn IS NULL AND updated_at=?")
            .bind(target).bind(now).bind(&s.id).bind(&s.updated_at).execute(&self.pool).await?.rows_affected();
        if changed != 1 {
            bail!("installation save changed before verification target was recorded");
        }
        Ok(())
    }
    pub async fn record_save_error(&self, id: &str, message: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let safe = message.chars().take(512).collect::<String>();
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE installation_sessions SET error=?,updated_at=? WHERE id=? AND state='saving' AND closed_at IS NULL")
            .bind(&safe).bind(&now).bind(id).execute(&mut *tx).await?.rows_affected();
        if changed == 1 {
            sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) SELECT id,state,progress,'Draft save failed; retry is available','server',? FROM installation_sessions WHERE id=?")
                .bind(now).bind(id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn retry_verification(&self, s: &InstallationSession) -> Result<()> {
        let now = Utc::now();
        let deadline = (now + chrono::Duration::minutes(30)).to_rfc3339();
        let now = now.to_rfc3339();
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE installation_sessions SET state='saving',progress=88,error=NULL,attempt=attempt+1,verification_image_id=NULL,verification_target_iqn=NULL,phase_deadline=?,updated_at=? WHERE id=? AND state='verification_failed' AND closed_at IS NULL AND updated_at=?")
            .bind(&deadline).bind(&now).bind(&s.id).bind(&s.updated_at).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            bail!("installation attempt changed; refresh before retrying");
        }
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,actor,created_at) VALUES (?,'saving',88,'Administrator started a new verification attempt','admin',?)")
            .bind(&s.id).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn refresh_save_deadline(&self, id: &str) -> Result<()> {
        let deadline = (Utc::now() + chrono::Duration::minutes(30)).to_rfc3339();
        sqlx::query("UPDATE installation_sessions SET phase_deadline=?,error=NULL,updated_at=? WHERE id=? AND state='saving' AND closed_at IS NULL")
            .bind(deadline).bind(Utc::now().to_rfc3339()).bind(id).execute(&self.pool).await?;
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
            "preparing" => matches!(state, "waiting" | "manual" | "failed" | "recovery_required"),
            "waiting" => matches!(
                state,
                "installing" | "cancelled" | "failed" | "recovery_required"
            ),
            "installing" => matches!(
                state,
                "installing" | "adapting" | "saving" | "failed" | "cancelled" | "recovery_required"
            ),
            "adapting" => matches!(
                state,
                "saving" | "failed" | "cancelled" | "recovery_required"
            ),
            "saving" => matches!(state, "verifying" | "recovery_required"),
            "verifying" => matches!(
                state,
                "published" | "verification_failed" | "recovery_required"
            ),
            "manual" => matches!(state, "cancelled" | "completed" | "recovery_required"),
            "verification_failed" => {
                matches!(state, "cancelled" | "completed" | "recovery_required")
            }
            "failed" | "cancelled" => matches!(state, "completed" | "recovery_required"),
            "recovery_required" => state == "completed",
            "published" => state == "published",
            _ => false,
        };
        if !allowed
            || !(0..=100).contains(&progress)
            || progress < s.progress
            || message.len() > 1024
        {
            bail!("invalid installation transition or progress");
        }
        let deadline = match state {
            "waiting" => Some((Utc::now() + chrono::Duration::hours(1)).to_rfc3339()),
            "installing" => Some((Utc::now() + chrono::Duration::hours(4)).to_rfc3339()),
            "adapting" => Some((Utc::now() + chrono::Duration::hours(1)).to_rfc3339()),
            "saving" | "verifying" => {
                Some((Utc::now() + chrono::Duration::minutes(30)).to_rfc3339())
            }
            "manual" => Some((Utc::now() + chrono::Duration::hours(24)).to_rfc3339()),
            _ => None,
        };
        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool.begin().await?;
        let closed_at = (state == "completed").then_some(now.as_str());
        let changed=sqlx::query("UPDATE installation_sessions SET state=?, progress=?,error=?,closed_at=COALESCE(?,closed_at),cleanup_state=CASE WHEN ?='completed' THEN 'complete' ELSE cleanup_state END,phase_deadline=COALESCE(?,phase_deadline),token=CASE WHEN ? IN ('failed','cancelled','recovery_required','completed','published') THEN '' ELSE token END,updated_at=? WHERE id=? AND state=? AND progress=? AND updated_at=?")
            .bind(state).bind(progress).bind(if matches!(state,"failed"|"recovery_required") {Some(message)} else {None}).bind(closed_at).bind(state).bind(deadline).bind(state).bind(&now).bind(&s.id).bind(&s.state).bind(s.progress).bind(&s.updated_at).execute(&mut *tx).await?.rows_affected();
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
        sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,created_at) SELECT id,'recovery_required',progress,'Server restarted; administrator recovery is required before cleanup',? FROM installation_sessions WHERE state IN ('preparing','waiting','installing','adapting','manual') AND legacy=0").bind(&now).execute(&mut *tx).await?;
        let n=sqlx::query("UPDATE installation_sessions SET state='recovery_required',error='Server restarted; verify the client is powered off before recovery',token='',updated_at=? WHERE state IN ('preparing','waiting','installing','adapting','manual') AND legacy=0").bind(now).execute(&mut *tx).await?.rows_affected();
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
        sqlx::raw_sql("PRAGMA foreign_keys=ON; CREATE TABLE clients(id TEXT PRIMARY KEY,installer_media_id TEXT); CREATE TABLE images(id TEXT PRIMARY KEY); CREATE TABLE installer_media(id TEXT PRIMARY KEY); INSERT INTO clients(id) VALUES ('client-a'),('client-b'); INSERT INTO images VALUES('image-a'); INSERT INTO installer_media VALUES('media-a');").execute(&pool).await.unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/0023_installer_sessions.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/0028_verified_image_factory.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        InstallationRepository::new(pool)
    }

    async fn legacy_database() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql("PRAGMA foreign_keys=ON; CREATE TABLE clients(id TEXT PRIMARY KEY,installer_media_id TEXT); CREATE TABLE images(id TEXT PRIMARY KEY); CREATE TABLE installer_media(id TEXT PRIMARY KEY); INSERT INTO clients(id,installer_media_id) VALUES ('client-a','media-a'),('client-b',NULL); INSERT INTO images VALUES('image-a'); INSERT INTO installer_media VALUES('media-a');")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/0023_installer_sessions.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn verified_factory_migration_archives_legacy_rows_without_live_credentials() {
        let pool = legacy_database().await;
        for (id, client, recipe, state) in [
            ("session-a", "client-a", "ubuntu", "completed"),
            ("session-b", "client-b", "interactive", "installing"),
        ] {
            sqlx::query("INSERT INTO installation_sessions (id,client_id,media_id,image_id,master,media_checksum,target_iqn,expected_bytes,source_ip,firmware,recipe,token,config_json,state,progress,created_at,updated_at,expires_at) VALUES (?,?, 'media-a','image-a','pool/image-a','checksum-a','iqn.example:client-a',1073741824,'192.0.2.10','uefi',?,'secret-token','{}',?,20,'now','now','later')")
                .bind(id).bind(client).bind(recipe).bind(state).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO installation_events(session_id,state,progress,message,created_at) VALUES (?,? ,20,'old event','now')")
                .bind(id).bind(state).execute(&pool).await.unwrap();
        }
        sqlx::raw_sql(include_str!(
            "../../../migrations/0028_verified_image_factory.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();

        let rows = sqlx::query_as::<_, (String, String, String, i64, Option<String>, String, Option<String>)>(
            "SELECT id,kind,state,legacy,closed_at,token,published_image_id FROM installation_sessions ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows[0].0, "session-a");
        assert_eq!(rows[0].1, "factory");
        assert_eq!(rows[0].2, "completed");
        assert_eq!(rows[0].3, 1);
        assert!(rows[0].4.is_some());
        assert!(rows[0].5.is_empty());
        assert_eq!(rows[1].1, "manual");
        assert_eq!(rows[1].2, "recovery_required");
        assert_eq!(rows[1].3, 1);
        assert!(rows[1].4.is_none());
        assert!(rows[1].5.is_empty());
        assert!(rows[1].6.is_none());
        let events: i64 = sqlx::query_scalar("SELECT count(*) FROM installation_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(events, 2);
        let violations = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(
            violations.is_empty(),
            "legacy migration left foreign-key violations"
        );
        let client_media: Option<String> =
            sqlx::query_scalar("SELECT installer_media_id FROM clients WHERE id='client-a'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(
            client_media.is_none(),
            "legacy client install mode must be cleared"
        );
    }
    fn session() -> InstallationSession {
        InstallationSession {
            id: "session-a".into(),
            client_id: "client-a".into(),
            kind: "factory".into(),
            recipe: "ubuntu".into(),
            media_id: "media-a".into(),
            media_checksum: "checksum-a".into(),
            draft_image_id: Some("image-a".into()),
            prior_master: Some("pool/image-a".into()),
            target_iqn: "iqn.example:client-a".into(),
            expected_bytes: Some(20 * 1024 * 1024 * 1024),
            source_ip: "192.0.2.10".into(),
            firmware: "uefi".into(),
            token: "secret-token".into(),
            config_json: "{\"password_hash\":\"secret-hash\"}".into(),
            state: "preparing".into(),
            progress: 0,
            error: None,
            assignment_state: "not_requested".into(),
            cleanup_state: "pending".into(),
            phase_deadline: Some((Utc::now() + chrono::Duration::hours(1)).to_rfc3339()),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            expires_at: (Utc::now() + chrono::Duration::hours(24)).to_rfc3339(),
            ..Default::default()
        }
    }
    /// Session lookups are scoped by id: a stale client operation must never
    /// resolve to another client's session, and an unknown id is missing.
    #[tokio::test]
    async fn getting_a_session_returns_only_the_requested_row() {
        let r = database().await;
        let first = session();
        r.insert(&first).await.unwrap();
        let mut second = session();
        second.id = "session-b".into();
        second.client_id = "client-b".into();
        second.token = "other-token".into();
        r.insert(&second).await.unwrap();

        let loaded = r.get("session-b").await.unwrap().expect("session-b exists");
        assert_eq!(loaded.id, "session-b");
        assert_eq!(loaded.client_id, "client-b");
        assert_eq!(loaded.token, "other-token");
        assert!(
            r.get("session-missing").await.unwrap().is_none(),
            "an unknown id must not resolve to another session"
        );
    }

    #[tokio::test]
    async fn cancellation_retains_ownership_and_stale_reports_cannot_revive_session() {
        let r = database().await;
        let s = session();
        r.insert(&s).await.unwrap();
        r.transition(&s, "waiting", 0, "boot").await.unwrap();
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
    async fn verification_failure_can_be_cancelled_without_publishing() {
        let r = database().await;
        let s = session();
        r.insert(&s).await.unwrap();
        sqlx::query("UPDATE installation_sessions SET state='verification_failed',attempt=1 WHERE id='session-a'")
            .execute(&r.pool)
            .await
            .unwrap();
        let s = r.get(&s.id).await.unwrap().unwrap();
        r.transition(&s, "cancelled", s.progress, "administrator cancelled")
            .await
            .unwrap();
        let s = r.get(&s.id).await.unwrap().unwrap();
        assert_eq!(s.state, "cancelled");
        assert!(s.published_image_id.is_none());
        r.transition(&s, "completed", 100, "offline recovery")
            .await
            .unwrap();
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
        r.transition(&s, "waiting", 0, "ready").await.unwrap();
        let waiting = r.get(&s.id).await.unwrap().unwrap();
        assert!(sqlx::query("DELETE FROM clients WHERE id='client-a'")
            .execute(&r.pool)
            .await
            .is_err());
        r.transition(&waiting, "cancelled", 0, "stop")
            .await
            .unwrap();
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
