use sqlx::SqlitePool;

/// Client-owned installer sessions and identity clones reserve the assigned storage.
pub async fn has_storage_reservation(pool: &SqlitePool, client_id: &str) -> anyhow::Result<bool> {
    let reserved: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM installation_sessions WHERE client_id = ? AND state != 'completed') OR EXISTS(SELECT 1 FROM client_identity_caches WHERE client_id = ?)",
    )
    .bind(client_id)
    .bind(client_id)
    .fetch_one(pool)
    .await?;
    Ok(reserved != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unfinished_installs_and_any_identity_cache_reserve_storage() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE installation_sessions(client_id TEXT, state TEXT); CREATE TABLE client_identity_caches(client_id TEXT);",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(!has_storage_reservation(&pool, "client").await.unwrap());
        sqlx::query("INSERT INTO installation_sessions VALUES ('client', 'failed')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(has_storage_reservation(&pool, "client").await.unwrap());
        sqlx::query("UPDATE installation_sessions SET state='completed'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(!has_storage_reservation(&pool, "client").await.unwrap());
        sqlx::query("INSERT INTO client_identity_caches VALUES ('client')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(has_storage_reservation(&pool, "client").await.unwrap());
    }
}
