use app_lib::audit_logger::{
    AuditLogFilter, AuditLogger, ControlOperation, OperationResult,
};
use app_lib::error_logger::{ControlError, ErrorLogger};
use chrono::Utc;
use proptest::prelude::*;
use sqlx::sqlite::SqlitePool;
use std::sync::Arc;

// ============================================================================
// Helper Functions
// ============================================================================

async fn create_test_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");

    // Initialize tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS control_operations (
            id TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            client_name TEXT NOT NULL,
            client_ip TEXT NOT NULL,
            os_type TEXT NOT NULL,
            operation_type TEXT NOT NULL,
            operation_mode TEXT NOT NULL,
            delay_minutes INTEGER,
            administrator TEXT,
            result TEXT NOT NULL,
            result_message TEXT,
            duration_ms INTEGER,
            timestamp TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("Failed to create control_operations table");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS error_logs (
            id TEXT PRIMARY KEY,
            client_id TEXT,
            operation_type TEXT NOT NULL,
            error_type TEXT NOT NULL,
            error_message TEXT NOT NULL,
            error_code TEXT,
            stack_trace TEXT,
            timestamp TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("Failed to create error_logs table");

    pool
}

// ============================================================================
// Property 22: Control Operation Logging
// For any control operation, the audit log should contain the client name, 
// IP, OS type, and operation type.
// Validates: Requirements 4.6, 8.1
// ============================================================================

#[tokio::test]
async fn test_property_22_operation_logging_success() {
    let pool = create_test_db().await;
    let logger = AuditLogger::new(Arc::new(pool));

    let operation = ControlOperation {
        client_id: "client-1".to_string(),
        client_name: "test-client".to_string(),
        client_ip: "192.168.1.100".to_string(),
        os_type: "linux".to_string(),
        operation_type: "shutdown".to_string(),
        operation_mode: "graceful".to_string(),
        delay_minutes: None,
        timestamp: Utc::now(),
        administrator: "admin".to_string(),
        result: OperationResult::Success,
    };

    let id = logger
        .log_operation(&operation)
        .await
        .expect("Failed to log operation");

    assert!(!id.is_empty(), "Operation ID should not be empty");

    // Verify the operation was logged
    let filter = AuditLogFilter {
        client_id: Some("client-1".to_string()),
        operation_type: None,
        start_date: None,
        end_date: None,
        limit: None,
        offset: None,
    };

    let logs = logger
        .query_logs(&filter)
        .await
        .expect("Failed to query logs");

    assert_eq!(logs.len(), 1, "Should have one log entry");
    let entry = &logs[0];
    assert_eq!(entry.client_name, "test-client");
    assert_eq!(entry.client_ip, "192.168.1.100");
    assert_eq!(entry.os_type, "linux");
    assert_eq!(entry.operation_type, "shutdown");
}

#[tokio::test]
async fn test_property_22_operation_logging_failed() {
    let pool = create_test_db().await;
    let logger = AuditLogger::new(Arc::new(pool));

    let operation = ControlOperation {
        client_id: "client-2".to_string(),
        client_name: "failed-client".to_string(),
        client_ip: "192.168.1.101".to_string(),
        os_type: "windows".to_string(),
        operation_type: "reboot".to_string(),
        operation_mode: "force".to_string(),
        delay_minutes: Some(5),
        timestamp: Utc::now(),
        administrator: "admin".to_string(),
        result: OperationResult::Failed("SSH connection timeout".to_string()),
    };

    let id = logger
        .log_operation(&operation)
        .await
        .expect("Failed to log operation");

    assert!(!id.is_empty(), "Operation ID should not be empty");

    // Verify the operation was logged with error details
    let filter = AuditLogFilter {
        client_id: Some("client-2".to_string()),
        operation_type: None,
        start_date: None,
        end_date: None,
        limit: None,
        offset: None,
    };

    let logs = logger
        .query_logs(&filter)
        .await
        .expect("Failed to query logs");

    assert_eq!(logs.len(), 1, "Should have one log entry");
    let entry = &logs[0];
    assert_eq!(entry.result, "failed");
    assert_eq!(entry.result_message, Some("SSH connection timeout".to_string()));
}

// ============================================================================
// Property 29: Control Operation Result Logging
// For any control operation completion, the result (success or failure) 
// should be logged with details.
// Validates: Requirements 8.2
// ============================================================================

#[tokio::test]
async fn test_property_29_result_logging_all_states() {
    let pool = create_test_db().await;
    let logger = AuditLogger::new(Arc::new(pool));

    let results = vec![
        OperationResult::Success,
        OperationResult::Failed("Test error".to_string()),
        OperationResult::Timeout,
        OperationResult::Cancelled,
    ];

    for (idx, result) in results.iter().enumerate() {
        let operation = ControlOperation {
            client_id: format!("client-{}", idx),
            client_name: format!("client-{}", idx),
            client_ip: format!("192.168.1.{}", 100 + idx),
            os_type: "linux".to_string(),
            operation_type: "shutdown".to_string(),
            operation_mode: "graceful".to_string(),
            delay_minutes: None,
            timestamp: Utc::now(),
            administrator: "admin".to_string(),
            result: result.clone(),
        };

        logger
            .log_operation(&operation)
            .await
            .expect("Failed to log operation");
    }

    // Verify all results were logged
    let filter = AuditLogFilter {
        client_id: None,
        operation_type: None,
        start_date: None,
        end_date: None,
        limit: None,
        offset: None,
    };

    let logs = logger
        .query_logs(&filter)
        .await
        .expect("Failed to query logs");

    assert_eq!(logs.len(), 4, "Should have four log entries");
    assert_eq!(logs[0].result, "cancelled");
    assert_eq!(logs[1].result, "timeout");
    assert_eq!(logs[2].result, "failed");
    assert_eq!(logs[3].result, "success");
}

// ============================================================================
// Property 30: Error Details Logging
// For any failed control operation, the error message and error code 
// should be logged.
// Validates: Requirements 8.3
// ============================================================================

#[tokio::test]
async fn test_property_30_error_logging() {
    let pool = create_test_db().await;
    let error_logger = ErrorLogger::new(Arc::new(pool));

    let error = ControlError {
        client_id: Some("client-1".to_string()),
        operation_type: "shutdown".to_string(),
        error_type: "timeout".to_string(),
        error_message: "SSH connection timeout after 5 seconds".to_string(),
        error_code: Some("SSH_TIMEOUT".to_string()),
        timestamp: Utc::now(),
        stack_trace: Some("at ssh_executor.rs:123".to_string()),
    };

    let id = error_logger
        .log_error(&error)
        .await
        .expect("Failed to log error");

    assert!(!id.is_empty(), "Error ID should not be empty");

    // Verify the error was logged
    let errors = error_logger
        .get_client_errors("client-1")
        .await
        .expect("Failed to get client errors");

    assert_eq!(errors.len(), 1, "Should have one error entry");
    let entry = &errors[0];
    assert_eq!(entry.error_type, "timeout");
    assert_eq!(entry.error_message, "SSH connection timeout after 5 seconds");
    assert_eq!(entry.error_code, Some("SSH_TIMEOUT".to_string()));
    assert_eq!(entry.stack_trace, Some("at ssh_executor.rs:123".to_string()));
}

// ============================================================================
// Property 31: Audit Log Querying
// For any audit log query with filters (client, operation type, date range), 
// the system should return matching log entries.
// Validates: Requirements 8.5
// ============================================================================

#[tokio::test]
async fn test_property_31_query_by_client_id() {
    let pool = create_test_db().await;
    let logger = AuditLogger::new(Arc::new(pool));

    // Log operations for multiple clients
    for client_idx in 0..3 {
        for op_idx in 0..2 {
            let operation = ControlOperation {
                client_id: format!("client-{}", client_idx),
                client_name: format!("client-{}", client_idx),
                client_ip: format!("192.168.1.{}", 100 + client_idx),
                os_type: "linux".to_string(),
                operation_type: if op_idx == 0 {
                    "shutdown".to_string()
                } else {
                    "reboot".to_string()
                },
                operation_mode: "graceful".to_string(),
                delay_minutes: None,
                timestamp: Utc::now(),
                administrator: "admin".to_string(),
                result: OperationResult::Success,
            };

            logger
                .log_operation(&operation)
                .await
                .expect("Failed to log operation");
        }
    }

    // Query by client ID
    let filter = AuditLogFilter {
        client_id: Some("client-1".to_string()),
        operation_type: None,
        start_date: None,
        end_date: None,
        limit: None,
        offset: None,
    };

    let logs = logger
        .query_logs(&filter)
        .await
        .expect("Failed to query logs");

    assert_eq!(logs.len(), 2, "Should have two log entries for client-1");
    assert!(logs.iter().all(|l| l.client_id == "client-1"));
}

#[tokio::test]
async fn test_property_31_query_by_operation_type() {
    let pool = create_test_db().await;
    let logger = AuditLogger::new(Arc::new(pool));

    // Log different operation types
    let operation_types = vec!["shutdown", "reboot", "remote"];
    for (idx, op_type) in operation_types.iter().enumerate() {
        let operation = ControlOperation {
            client_id: format!("client-{}", idx),
            client_name: format!("client-{}", idx),
            client_ip: format!("192.168.1.{}", 100 + idx),
            os_type: "linux".to_string(),
            operation_type: op_type.to_string(),
            operation_mode: "graceful".to_string(),
            delay_minutes: None,
            timestamp: Utc::now(),
            administrator: "admin".to_string(),
            result: OperationResult::Success,
        };

        logger
            .log_operation(&operation)
            .await
            .expect("Failed to log operation");
    }

    // Query by operation type
    let filter = AuditLogFilter {
        client_id: None,
        operation_type: Some("shutdown".to_string()),
        start_date: None,
        end_date: None,
        limit: None,
        offset: None,
    };

    let logs = logger
        .query_logs(&filter)
        .await
        .expect("Failed to query logs");

    assert_eq!(logs.len(), 1, "Should have one shutdown operation");
    assert_eq!(logs[0].operation_type, "shutdown");
}

#[tokio::test]
async fn test_property_31_query_with_limit() {
    let pool = create_test_db().await;
    let logger = AuditLogger::new(Arc::new(pool));

    // Log multiple operations
    for idx in 0..10 {
        let operation = ControlOperation {
            client_id: format!("client-{}", idx),
            client_name: format!("client-{}", idx),
            client_ip: format!("192.168.1.{}", 100 + idx),
            os_type: "linux".to_string(),
            operation_type: "shutdown".to_string(),
            operation_mode: "graceful".to_string(),
            delay_minutes: None,
            timestamp: Utc::now(),
            administrator: "admin".to_string(),
            result: OperationResult::Success,
        };

        logger
            .log_operation(&operation)
            .await
            .expect("Failed to log operation");
    }

    // Query with limit
    let filter = AuditLogFilter {
        client_id: None,
        operation_type: None,
        start_date: None,
        end_date: None,
        limit: Some(5),
        offset: None,
    };

    let logs = logger
        .query_logs(&filter)
        .await
        .expect("Failed to query logs");

    assert_eq!(logs.len(), 5, "Should return only 5 entries");
}

// ============================================================================
// Property-Based Tests
// ============================================================================

// Property-based tests for operation logging
#[tokio::test]
async fn test_prop_operation_logging_preserves_data_sample() {
    let pool = create_test_db().await;
    let logger = AuditLogger::new(Arc::new(pool));

    let test_cases = vec![
        ("client-1", "test-client-1", "192.168.1.100", "linux", "shutdown", "graceful", 0u32),
        ("client-2", "test-client-2", "192.168.1.101", "windows", "reboot", "force", 5u32),
        ("client-3", "test-client-3", "192.168.1.102", "linux", "remote", "graceful", 10u32),
    ];

    for (client_id, client_name, client_ip, os_type, operation_type, operation_mode, delay) in test_cases {
        let operation = ControlOperation {
            client_id: client_id.to_string(),
            client_name: client_name.to_string(),
            client_ip: client_ip.to_string(),
            os_type: os_type.to_string(),
            operation_type: operation_type.to_string(),
            operation_mode: operation_mode.to_string(),
            delay_minutes: if delay > 0 { Some(delay) } else { None },
            timestamp: Utc::now(),
            administrator: "admin".to_string(),
            result: OperationResult::Success,
        };

        logger
            .log_operation(&operation)
            .await
            .expect("Failed to log operation");

        // Verify all data was preserved
        let filter = AuditLogFilter {
            client_id: Some(client_id.to_string()),
            operation_type: None,
            start_date: None,
            end_date: None,
            limit: None,
            offset: None,
        };

        let logs = logger
            .query_logs(&filter)
            .await
            .expect("Failed to query logs");

        assert_eq!(logs.len(), 1);
        let entry = &logs[0];
        assert_eq!(entry.client_id, client_id);
        assert_eq!(entry.client_name, client_name);
        assert_eq!(entry.client_ip, client_ip);
        assert_eq!(entry.os_type, os_type);
        assert_eq!(entry.operation_type, operation_type);
        assert_eq!(entry.operation_mode, operation_mode);
        assert_eq!(entry.result, "success");
    }
}

// Property-based tests for error logging
#[tokio::test]
async fn test_prop_error_logging_preserves_data_sample() {
    let pool = create_test_db().await;
    let error_logger = ErrorLogger::new(Arc::new(pool));

    let test_cases = vec![
        ("shutdown", "timeout", "SSH connection timeout", "SSH_TIMEOUT"),
        ("reboot", "connection_failed", "Failed to connect to client", "CONN_FAILED"),
        ("remote", "command_failed", "Command execution failed", "CMD_FAILED"),
    ];

    for (operation_type, error_type, error_message, error_code) in test_cases {
        let error = ControlError {
            client_id: Some("client-1".to_string()),
            operation_type: operation_type.to_string(),
            error_type: error_type.to_string(),
            error_message: error_message.to_string(),
            error_code: Some(error_code.to_string()),
            timestamp: Utc::now(),
            stack_trace: None,
        };

        error_logger
            .log_error(&error)
            .await
            .expect("Failed to log error");

        // Verify all data was preserved
        let errors = error_logger
            .get_errors_by_type(operation_type)
            .await
            .expect("Failed to get errors");

        assert!(errors.len() >= 1);
        let entry = errors.iter().find(|e| e.error_type == error_type).unwrap();
        assert_eq!(entry.operation_type, operation_type);
        assert_eq!(entry.error_type, error_type);
        assert_eq!(entry.error_message, error_message);
        assert_eq!(entry.error_code, Some(error_code.to_string()));
    }
}

// Property-based test for multiple operations
#[tokio::test]
async fn test_prop_multiple_operations_logged_independently() {
    let pool = create_test_db().await;
    let logger = AuditLogger::new(Arc::new(pool));

    let num_operations = 15;

    // Log multiple operations
    for idx in 0..num_operations {
        let operation = ControlOperation {
            client_id: format!("client-{}", idx),
            client_name: format!("client-{}", idx),
            client_ip: format!("192.168.1.{}", 100 + (idx % 155)),
            os_type: "linux".to_string(),
            operation_type: "shutdown".to_string(),
            operation_mode: "graceful".to_string(),
            delay_minutes: None,
            timestamp: Utc::now(),
            administrator: "admin".to_string(),
            result: OperationResult::Success,
        };

        logger
            .log_operation(&operation)
            .await
            .expect("Failed to log operation");
    }

    // Verify all operations were logged
    let filter = AuditLogFilter {
        client_id: None,
        operation_type: None,
        start_date: None,
        end_date: None,
        limit: None,
        offset: None,
    };

    let logs = logger
        .query_logs(&filter)
        .await
        .expect("Failed to query logs");

    assert_eq!(logs.len(), num_operations);
}

// ============================================================================
// Unit Tests for OperationResult
// ============================================================================

#[test]
fn test_operation_result_as_str_all_variants() {
    assert_eq!(OperationResult::Success.as_str(), "success");
    assert_eq!(
        OperationResult::Failed("error".to_string()).as_str(),
        "failed"
    );
    assert_eq!(OperationResult::Timeout.as_str(), "timeout");
    assert_eq!(OperationResult::Cancelled.as_str(), "cancelled");
}

#[test]
fn test_operation_result_details_extraction() {
    assert_eq!(OperationResult::Success.details(), None);
    assert_eq!(
        OperationResult::Failed("test error".to_string()).details(),
        Some("test error".to_string())
    );
    assert_eq!(OperationResult::Timeout.details(), None);
    assert_eq!(OperationResult::Cancelled.details(), None);
}

#[test]
fn test_operation_result_equality() {
    assert_eq!(OperationResult::Success, OperationResult::Success);
    assert_eq!(
        OperationResult::Failed("error".to_string()),
        OperationResult::Failed("error".to_string())
    );
    assert_ne!(
        OperationResult::Failed("error1".to_string()),
        OperationResult::Failed("error2".to_string())
    );
}
