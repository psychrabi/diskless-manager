use crate::domain::{BootLogEntry, ClientId};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct BootLogRepository {
    pool: SqlitePool,
}

impl BootLogRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_for_client(
        &self,
        client_id: &ClientId,
        limit: i32,
    ) -> Result<Vec<BootLogEntry>> {
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
        .bind(client_id.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("failed to query client boot history")?;

        Ok(rows
            .into_iter()
            .filter_map(
                |(id, client_id, image_id, boot_time, success, duration_ms, message)| {
                    let boot_time = DateTime::parse_from_rfc3339(&boot_time)
                        .ok()?
                        .with_timezone(&Utc);

                    Some(BootLogEntry {
                        id,
                        client_id,
                        image_id,
                        boot_time,
                        success,
                        duration_ms,
                        message,
                    })
                },
            )
            .collect())
    }
}
