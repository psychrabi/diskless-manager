use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// Represents the result of a control operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationResult {
    Success,
    Failed(String),
    Timeout,
    Cancelled,
}

impl OperationResult {
    pub fn as_str(&self) -> &str {
        match self {
            OperationResult::Success => "success",
            OperationResult::Failed(_) => "failed",
            OperationResult::Timeout => "timeout",
            OperationResult::Cancelled => "cancelled",
        }
    }

    pub fn details(&self) -> Option<String> {
        match self {
            OperationResult::Failed(msg) => Some(msg.clone()),
            _ => None,
        }
    }
}

/// Represents a control operation to be logged
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlOperation {
    pub client_id: String,
    pub client_name: String,
    pub client_ip: String,
    pub os_type: String,
    pub operation_type: String, // "shutdown", "reboot", "remote"
    pub operation_mode: String, // "graceful", "force"
    pub delay_minutes: Option<u32>,
    pub timestamp: DateTime<Utc>,
    pub administrator: String,
    pub result: OperationResult,
}

/// Represents an audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub client_id: String,
    pub client_name: String,
    pub client_ip: String,
    pub os_type: String,
    pub operation_type: String,
    pub operation_mode: String,
    pub delay_minutes: Option<u32>,
    pub administrator: String,
    pub result: String,
    pub result_message: Option<String>,
    pub duration_ms: Option<i64>,
    pub timestamp: String,
}

/// Filter for querying audit logs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogFilter {
    pub client_id: Option<String>,
    pub operation_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Audit logger for control operations
pub struct AuditLogger {
    db: Arc<SqlitePool>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(db: Arc<SqlitePool>) -> Self {
        Self { db }
    }

    /// Log a control operation
    pub async fn log_operation(&self, operation: &ControlOperation) -> anyhow::Result<String> {
        let id = Uuid::new_v4().to_string();
        let result_str = operation.result.as_str();
        let result_message = operation.result.details();

        sqlx::query(
            r#"
            INSERT INTO control_operations (
                id, client_id, client_name, client_ip, os_type,
                operation_type, operation_mode, delay_minutes,
                administrator, result, result_message, timestamp
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&operation.client_id)
        .bind(&operation.client_name)
        .bind(&operation.client_ip)
        .bind(&operation.os_type)
        .bind(&operation.operation_type)
        .bind(&operation.operation_mode)
        .bind(operation.delay_minutes)
        .bind(&operation.administrator)
        .bind(result_str)
        .bind(result_message)
        .bind(operation.timestamp.to_rfc3339())
        .execute(self.db.as_ref())
        .await?;

        Ok(id)
    }

    /// Query audit logs with optional filters
    pub async fn query_logs(&self, filter: &AuditLogFilter) -> anyhow::Result<Vec<AuditLogEntry>> {
        let mut query = String::from(
            r#"
            SELECT id, client_id, client_name, client_ip, os_type,
                   operation_type, operation_mode, delay_minutes,
                   administrator, result, result_message, timestamp
            FROM control_operations
            WHERE 1=1
            "#,
        );

        let mut params: Vec<String> = Vec::new();

        if let Some(client_id) = &filter.client_id {
            query.push_str(" AND client_id = ?");
            params.push(client_id.clone());
        }

        if let Some(op_type) = &filter.operation_type {
            query.push_str(" AND operation_type = ?");
            params.push(op_type.clone());
        }

        if let Some(start_date) = &filter.start_date {
            query.push_str(" AND timestamp >= ?");
            params.push(start_date.clone());
        }

        if let Some(end_date) = &filter.end_date {
            query.push_str(" AND timestamp <= ?");
            params.push(end_date.clone());
        }

        query.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = filter.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = filter.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        let mut query_builder = sqlx::query_as::<_, (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<i32>,
            String,
            String,
            Option<String>,
            String,
        )>(&query);

        for param in params {
            query_builder = query_builder.bind(param);
        }

        let rows = query_builder.fetch_all(self.db.as_ref()).await?;

        let entries = rows
            .into_iter()
            .map(
                |(
                    id,
                    client_id,
                    client_name,
                    client_ip,
                    os_type,
                    operation_type,
                    operation_mode,
                    delay_minutes,
                    administrator,
                    result,
                    result_message,
                    timestamp,
                )| {
                    AuditLogEntry {
                        id,
                        client_id,
                        client_name,
                        client_ip,
                        os_type,
                        operation_type,
                        operation_mode,
                        delay_minutes: delay_minutes.map(|d| d as u32),
                        administrator,
                        result,
                        result_message,
                        duration_ms: None,
                        timestamp,
                    }
                },
            )
            .collect();

        Ok(entries)
    }

    /// Get operation history for a specific client
    pub async fn get_client_history(&self, client_id: &str) -> anyhow::Result<Vec<AuditLogEntry>> {
        let filter = AuditLogFilter {
            client_id: Some(client_id.to_string()),
            operation_type: None,
            start_date: None,
            end_date: None,
            limit: Some(100),
            offset: None,
        };

        self.query_logs(&filter).await
    }

    /// Get recent operations
    pub async fn get_recent_operations(&self, limit: i64) -> anyhow::Result<Vec<AuditLogEntry>> {
        let filter = AuditLogFilter {
            client_id: None,
            operation_type: None,
            start_date: None,
            end_date: None,
            limit: Some(limit),
            offset: None,
        };

        self.query_logs(&filter).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_result_as_str() {
        assert_eq!(OperationResult::Success.as_str(), "success");
        assert_eq!(
            OperationResult::Failed("test error".to_string()).as_str(),
            "failed"
        );
        assert_eq!(OperationResult::Timeout.as_str(), "timeout");
        assert_eq!(OperationResult::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn test_operation_result_details() {
        assert_eq!(OperationResult::Success.details(), None);
        assert_eq!(
            OperationResult::Failed("test error".to_string()).details(),
            Some("test error".to_string())
        );
        assert_eq!(OperationResult::Timeout.details(), None);
        assert_eq!(OperationResult::Cancelled.details(), None);
    }
}
