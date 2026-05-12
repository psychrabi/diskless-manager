use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// Represents a control error to be logged
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlError {
    pub client_id: Option<String>,
    pub operation_type: String,
    pub error_type: String,
    pub error_message: String,
    pub error_code: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub stack_trace: Option<String>,
}

/// Represents an error log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLogEntry {
    pub id: String,
    pub client_id: Option<String>,
    pub operation_type: String,
    pub error_type: String,
    pub error_message: String,
    pub error_code: Option<String>,
    pub stack_trace: Option<String>,
    pub timestamp: String,
}

/// Error logger for control operations
pub struct ErrorLogger {
    db: Arc<SqlitePool>,
}

impl ErrorLogger {
    /// Create a new error logger
    pub fn new(db: Arc<SqlitePool>) -> Self {
        Self { db }
    }

    /// Log an error
    pub async fn log_error(&self, error: &ControlError) -> anyhow::Result<String> {
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO error_logs (
                id, client_id, operation_type, error_type,
                error_message, error_code, stack_trace, timestamp
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&error.client_id)
        .bind(&error.operation_type)
        .bind(&error.error_type)
        .bind(&error.error_message)
        .bind(&error.error_code)
        .bind(&error.stack_trace)
        .bind(error.timestamp.to_rfc3339())
        .execute(self.db.as_ref())
        .await?;

        Ok(id)
    }

    /// Get recent errors
    pub async fn get_recent_errors(&self, limit: i64) -> anyhow::Result<Vec<ErrorLogEntry>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                String,
            ),
        >(
            r#"
            SELECT id, client_id, operation_type, error_type,
                   error_message, error_code, stack_trace, timestamp
            FROM error_logs
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(self.db.as_ref())
        .await?;

        let entries = rows
            .into_iter()
            .map(
                |(
                    id,
                    client_id,
                    operation_type,
                    error_type,
                    error_message,
                    error_code,
                    stack_trace,
                    timestamp,
                )| {
                    ErrorLogEntry {
                        id,
                        client_id,
                        operation_type,
                        error_type,
                        error_message,
                        error_code,
                        stack_trace,
                        timestamp,
                    }
                },
            )
            .collect();

        Ok(entries)
    }

    /// Get errors for a specific client
    pub async fn get_client_errors(&self, client_id: &str) -> anyhow::Result<Vec<ErrorLogEntry>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                String,
            ),
        >(
            r#"
            SELECT id, client_id, operation_type, error_type,
                   error_message, error_code, stack_trace, timestamp
            FROM error_logs
            WHERE client_id = ?
            ORDER BY timestamp DESC
            "#,
        )
        .bind(client_id)
        .fetch_all(self.db.as_ref())
        .await?;

        let entries = rows
            .into_iter()
            .map(
                |(
                    id,
                    client_id,
                    operation_type,
                    error_type,
                    error_message,
                    error_code,
                    stack_trace,
                    timestamp,
                )| {
                    ErrorLogEntry {
                        id,
                        client_id,
                        operation_type,
                        error_type,
                        error_message,
                        error_code,
                        stack_trace,
                        timestamp,
                    }
                },
            )
            .collect();

        Ok(entries)
    }

    /// Get errors by operation type
    pub async fn get_errors_by_type(
        &self,
        operation_type: &str,
    ) -> anyhow::Result<Vec<ErrorLogEntry>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                String,
            ),
        >(
            r#"
            SELECT id, client_id, operation_type, error_type,
                   error_message, error_code, stack_trace, timestamp
            FROM error_logs
            WHERE operation_type = ?
            ORDER BY timestamp DESC
            "#,
        )
        .bind(operation_type)
        .fetch_all(self.db.as_ref())
        .await?;

        let entries = rows
            .into_iter()
            .map(
                |(
                    id,
                    client_id,
                    operation_type,
                    error_type,
                    error_message,
                    error_code,
                    stack_trace,
                    timestamp,
                )| {
                    ErrorLogEntry {
                        id,
                        client_id,
                        operation_type,
                        error_type,
                        error_message,
                        error_code,
                        stack_trace,
                        timestamp,
                    }
                },
            )
            .collect();

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_error_creation() {
        let error = ControlError {
            client_id: Some("client-1".to_string()),
            operation_type: "shutdown".to_string(),
            error_type: "timeout".to_string(),
            error_message: "SSH connection timeout".to_string(),
            error_code: Some("SSH_TIMEOUT".to_string()),
            timestamp: Utc::now(),
            stack_trace: None,
        };

        assert_eq!(error.client_id, Some("client-1".to_string()));
        assert_eq!(error.operation_type, "shutdown");
        assert_eq!(error.error_type, "timeout");
    }
}
