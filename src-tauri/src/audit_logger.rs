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

/// An administrative audit event.
///
/// Routing metadata is recorded after authentication; request bodies and
/// credentials are intentionally never stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub actor: String,
    pub actor_role: Option<String>,
    pub ip: Option<String>,
    pub module: String,
    pub action: String,
    pub method: String,
    pub path: String,
    pub status: String,
    pub status_code: i64,
    pub details: Option<String>,
    pub timestamp: String,
}

/// A new event before the logger assigns its id and timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAuditEvent {
    pub actor: String,
    pub actor_role: Option<String>,
    pub ip: Option<String>,
    pub module: String,
    pub action: String,
    pub method: String,
    pub path: String,
    pub status: String,
    pub status_code: i64,
    pub details: Option<String>,
}

/// Filter for querying administrative audit events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditEventFilter {
    pub actor: Option<String>,
    pub module: Option<String>,
    pub action: Option<String>,
    pub status: Option<String>,
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

        let mut query_builder = sqlx::query_as::<
            _,
            (
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
            ),
        >(&query);

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

    /// Record an administrative audit event.
    pub async fn log_event(&self, event: &NewAuditEvent) -> anyhow::Result<String> {
        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO audit_events (
                id, actor, actor_role, ip, module, action, method, path,
                status, status_code, details, timestamp
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&event.actor)
        .bind(&event.actor_role)
        .bind(&event.ip)
        .bind(&event.module)
        .bind(&event.action)
        .bind(&event.method)
        .bind(&event.path)
        .bind(&event.status)
        .bind(event.status_code)
        .bind(&event.details)
        .bind(timestamp)
        .execute(self.db.as_ref())
        .await?;

        Ok(id)
    }

    /// Query administrative audit events, newest first.
    pub async fn query_events(&self, filter: &AuditEventFilter) -> anyhow::Result<Vec<AuditEvent>> {
        let limit = filter.limit.unwrap_or(100).clamp(1, 500);
        let offset = filter.offset.unwrap_or(0).max(0);

        let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            r#"
            SELECT id, actor, actor_role, ip, module, action, method, path,
                   status, status_code, details, timestamp
            FROM audit_events
            WHERE 1=1
            "#,
        );

        if let Some(actor) = &filter.actor {
            query.push(" AND actor = ").push_bind(actor);
        }
        if let Some(module) = &filter.module {
            query.push(" AND module = ").push_bind(module);
        }
        if let Some(action) = &filter.action {
            query.push(" AND action = ").push_bind(action);
        }
        if let Some(status) = &filter.status {
            query.push(" AND status = ").push_bind(status);
        }
        if let Some(start_date) = &filter.start_date {
            query.push(" AND timestamp >= ").push_bind(start_date);
        }
        if let Some(end_date) = &filter.end_date {
            query.push(" AND timestamp <= ").push_bind(end_date);
        }

        query
            .push(" ORDER BY timestamp DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let rows = query
            .build_query_as::<(
                String,
                String,
                Option<String>,
                Option<String>,
                String,
                String,
                String,
                String,
                String,
                i64,
                Option<String>,
                String,
            )>()
            .fetch_all(self.db.as_ref())
            .await?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    actor,
                    actor_role,
                    ip,
                    module,
                    action,
                    method,
                    path,
                    status,
                    status_code,
                    details,
                    timestamp,
                )| AuditEvent {
                    id,
                    actor,
                    actor_role,
                    ip,
                    module,
                    action,
                    method,
                    path,
                    status,
                    status_code,
                    details,
                    timestamp,
                },
            )
            .collect())
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

    #[tokio::test]
    async fn audit_events_are_recorded_and_filtered_without_request_bodies() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let logger = AuditLogger::new(Arc::new(pool));

        logger
            .log_event(&NewAuditEvent {
                actor: "admin".into(),
                actor_role: Some("admin".into()),
                ip: Some("192.0.2.10".into()),
                module: "clients".into(),
                action: "shutdown".into(),
                method: "POST".into(),
                path: "/api/clients/client-1/shutdown".into(),
                status: "failed".into(),
                status_code: 409,
                details: Some("Conflict".into()),
            })
            .await
            .unwrap();
        logger
            .log_event(&NewAuditEvent {
                actor: "operator".into(),
                actor_role: Some("user".into()),
                ip: None,
                module: "images".into(),
                action: "rollback".into(),
                method: "POST".into(),
                path: "/api/images/pool/image/snapshots/stable/rollback".into(),
                status: "success".into(),
                status_code: 200,
                details: None,
            })
            .await
            .unwrap();

        let events = logger
            .query_events(&AuditEventFilter {
                actor: Some("admin".into()),
                ..AuditEventFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "shutdown");
        assert_eq!(events[0].status_code, 409);
        assert_eq!(events[0].ip.as_deref(), Some("192.0.2.10"));

        let none = logger
            .query_events(&AuditEventFilter {
                status: Some("timeout".into()),
                ..AuditEventFilter::default()
            })
            .await
            .unwrap();
        assert!(none.is_empty());
    }
}
