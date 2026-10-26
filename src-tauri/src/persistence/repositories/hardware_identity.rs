use anyhow::Result;
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct HardwareProfile {
    pub client_id: String,
    pub profile_key: String,
    pub boot_nic_ids: String,
    pub architecture: String,
    pub firmware: String,
    pub observed_at: String,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct IdentityCache {
    pub client_id: String,
    pub id: String,
    pub profile_key: String,
    pub source_revision: String,
    pub dataset: String,
    pub snapshot: String,
    pub replacement: String,
    pub prior_dataset: Option<String>,
    pub state: String,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct HardwareIdentityRepository {
    pub(crate) pool: SqlitePool,
}

impl HardwareIdentityRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn profile(&self, client_id: &str) -> Result<Option<HardwareProfile>> {
        Ok(
            sqlx::query_as("SELECT * FROM hardware_profiles WHERE client_id = ?")
                .bind(client_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn cache(&self, client_id: &str) -> Result<Option<IdentityCache>> {
        Ok(
            sqlx::query_as("SELECT * FROM client_identity_caches WHERE client_id = ?")
                .bind(client_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn observe(&self, profile: &HardwareProfile) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO hardware_profiles(client_id,profile_key,boot_nic_ids,architecture,firmware,observed_at) VALUES (?,?,?,?,?,?) ON CONFLICT(client_id) DO UPDATE SET profile_key=excluded.profile_key,boot_nic_ids=excluded.boot_nic_ids,architecture=excluded.architecture,firmware=excluded.firmware,observed_at=excluded.observed_at")
            .bind(&profile.client_id).bind(&profile.profile_key).bind(&profile.boot_nic_ids)
            .bind(&profile.architecture).bind(&profile.firmware).bind(&profile.observed_at)
            .execute(&mut *tx).await?;
        sqlx::query("UPDATE client_identity_caches SET state='stale', updated_at=? WHERE client_id=? AND profile_key<>? AND state NOT IN ('applying','deleting') AND NOT (state='failed' AND prior_dataset IS NOT NULL)")
            .bind(&profile.observed_at).bind(&profile.client_id).bind(&profile.profile_key).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn insert(&self, cache: &IdentityCache) -> Result<()> {
        sqlx::query("INSERT INTO client_identity_caches(client_id,id,profile_key,source_revision,dataset,snapshot,replacement,state,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?)")
            .bind(&cache.client_id).bind(&cache.id).bind(&cache.profile_key).bind(&cache.source_revision)
            .bind(&cache.dataset).bind(&cache.snapshot).bind(&cache.replacement).bind(&cache.state)
            .bind(&cache.created_at).bind(&cache.updated_at).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn state(&self, client_id: &str, state: &str, error: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE client_identity_caches SET state=?, last_error=?,updated_at=? WHERE client_id=?")
            .bind(state).bind(error).bind(chrono::Utc::now().to_rfc3339()).bind(client_id).execute(&self.pool).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn observation_invalidates_cache_atomically_and_separates_clients() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql("PRAGMA foreign_keys=ON; CREATE TABLE clients(id TEXT PRIMARY KEY); INSERT INTO clients VALUES('one'),('two');").execute(&pool).await.unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/0024_hardware_identity.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let repository = HardwareIdentityRepository::new(pool);
        let profile = HardwareProfile {
            client_id: "one".into(),
            profile_key: "profile-a".into(),
            boot_nic_ids: "[]".into(),
            architecture: "x86_64".into(),
            firmware: "uefi".into(),
            observed_at: "now".into(),
        };
        repository.observe(&profile).await.unwrap();
        let cache = IdentityCache {
            client_id: "one".into(),
            id: "cache-one".into(),
            profile_key: "profile-a".into(),
            source_revision: "pool/windows@v1".into(),
            dataset: "pool/one".into(),
            snapshot: "identity_a".into(),
            replacement: "pool/one_identity_a".into(),
            prior_dataset: None,
            state: "ready".into(),
            last_error: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        };
        repository.insert(&cache).await.unwrap();
        assert!(
            repository.insert(&cache).await.is_err(),
            "a concurrent capture cannot replace the immutable baseline"
        );
        assert!(repository.cache("two").await.unwrap().is_none());
        repository.observe(&profile).await.unwrap();
        assert_eq!(
            repository.cache("one").await.unwrap().unwrap().state,
            "ready"
        );
        let mut changed = profile;
        changed.profile_key = "profile-b".into();
        repository.observe(&changed).await.unwrap();
        let retained = repository.cache("one").await.unwrap().unwrap();
        assert_eq!(retained.state, "stale");
        assert_eq!(retained.snapshot, "identity_a");
        assert_eq!(retained.source_revision, "pool/windows@v1");
        changed.profile_key = "profile-a".into();
        repository.observe(&changed).await.unwrap();
        assert_eq!(
            repository.cache("one").await.unwrap().unwrap().state,
            "stale",
            "a changed profile never silently revives a stale baseline"
        );
    }
}
