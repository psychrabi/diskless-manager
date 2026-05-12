use app_lib::ssh_executor::{CommandResult, SshConfig, SshExecutor};

#[test]
fn test_ssh_config_default_values() {
    let config = SshConfig::default();
    assert_eq!(
        config.connection_timeout, 5,
        "Connection timeout should be 5 seconds"
    );
    assert_eq!(
        config.command_timeout, 30,
        "Command timeout should be 30 seconds"
    );
    assert_eq!(config.username, "root", "Default username should be root");
    assert!(
        config.disable_host_key_verification,
        "Host key verification should be disabled by default"
    );
    assert_eq!(config.max_retries, 1, "Max retries should be 1");
}

#[test]
fn test_ssh_config_custom_values() {
    let config = SshConfig {
        connection_timeout: 10,
        command_timeout: 60,
        username: "admin".to_string(),
        disable_host_key_verification: false,
        max_retries: 3,
    };

    assert_eq!(config.connection_timeout, 10);
    assert_eq!(config.command_timeout, 60);
    assert_eq!(config.username, "admin");
    assert!(!config.disable_host_key_verification);
    assert_eq!(config.max_retries, 3);
}

#[test]
fn test_command_result_creation() {
    let result = CommandResult {
        exit_code: 0,
        stdout: "test output".to_string(),
        stderr: String::new(),
        duration_ms: 100,
    };

    assert_eq!(result.exit_code, 0, "Exit code should be 0");
    assert_eq!(result.stdout, "test output", "Stdout should match");
    assert!(result.stderr.is_empty(), "Stderr should be empty");
    assert_eq!(result.duration_ms, 100, "Duration should be 100ms");
}

#[test]
fn test_command_result_with_error() {
    let result = CommandResult {
        exit_code: 1,
        stdout: String::new(),
        stderr: "error message".to_string(),
        duration_ms: 50,
    };

    assert_eq!(result.exit_code, 1, "Exit code should be 1");
    assert!(result.stdout.is_empty(), "Stdout should be empty");
    assert_eq!(result.stderr, "error message", "Stderr should match");
    assert_eq!(result.duration_ms, 50, "Duration should be 50ms");
}

#[test]
fn test_ssh_executor_default_creation() {
    let executor = SshExecutor::new();
    assert_eq!(executor.config.connection_timeout, 5);
    assert_eq!(executor.config.command_timeout, 30);
    assert_eq!(executor.config.username, "root");
}

#[test]
fn test_ssh_executor_with_custom_config() {
    let config = SshConfig {
        connection_timeout: 15,
        command_timeout: 45,
        username: "testuser".to_string(),
        disable_host_key_verification: true,
        max_retries: 2,
    };

    let executor = SshExecutor::with_config(config);
    assert_eq!(executor.config.connection_timeout, 15);
    assert_eq!(executor.config.command_timeout, 45);
    assert_eq!(executor.config.username, "testuser");
    assert_eq!(executor.config.max_retries, 2);
}

#[test]
fn test_ssh_executor_default_trait() {
    let executor = SshExecutor::default();
    assert_eq!(executor.config.connection_timeout, 5);
    assert_eq!(executor.config.command_timeout, 30);
}

#[test]
fn test_command_result_clone() {
    let result1 = CommandResult {
        exit_code: 0,
        stdout: "output".to_string(),
        stderr: String::new(),
        duration_ms: 100,
    };

    let result2 = result1.clone();
    assert_eq!(result1.exit_code, result2.exit_code);
    assert_eq!(result1.stdout, result2.stdout);
    assert_eq!(result1.duration_ms, result2.duration_ms);
}

#[test]
fn test_ssh_config_clone() {
    let config1 = SshConfig {
        connection_timeout: 5,
        command_timeout: 30,
        username: "root".to_string(),
        disable_host_key_verification: true,
        max_retries: 1,
    };

    let config2 = config1.clone();
    assert_eq!(config1.connection_timeout, config2.connection_timeout);
    assert_eq!(config1.username, config2.username);
    assert_eq!(config1.max_retries, config2.max_retries);
}

#[test]
fn test_ssh_executor_clear_pool() {
    let executor = SshExecutor::new();
    // This should not panic
    executor.clear_pool();
}

#[test]
fn test_command_result_with_large_output() {
    let large_output = "x".repeat(10000);
    let result = CommandResult {
        exit_code: 0,
        stdout: large_output.clone(),
        stderr: String::new(),
        duration_ms: 500,
    };

    assert_eq!(result.stdout.len(), 10000);
    assert_eq!(result.stdout, large_output);
}

#[test]
fn test_command_result_with_multiline_output() {
    let multiline = "line1\nline2\nline3\n".to_string();
    let result = CommandResult {
        exit_code: 0,
        stdout: multiline.clone(),
        stderr: String::new(),
        duration_ms: 100,
    };

    assert_eq!(result.stdout, multiline);
    assert!(result.stdout.contains("line1"));
    assert!(result.stdout.contains("line2"));
    assert!(result.stdout.contains("line3"));
}

#[test]
fn test_ssh_config_with_different_usernames() {
    let configs = vec![
        (
            "root",
            SshConfig {
                username: "root".to_string(),
                ..Default::default()
            },
        ),
        (
            "admin",
            SshConfig {
                username: "admin".to_string(),
                ..Default::default()
            },
        ),
        (
            "ubuntu",
            SshConfig {
                username: "ubuntu".to_string(),
                ..Default::default()
            },
        ),
    ];

    for (expected_user, config) in configs {
        assert_eq!(config.username, expected_user);
    }
}

#[test]
fn test_ssh_executor_pool_operations() {
    let executor = SshExecutor::new();

    // Clear pool should work without errors
    executor.clear_pool();
    executor.clear_pool(); // Should be idempotent

    // Verify executor is still usable after pool clear
    assert_eq!(executor.config.connection_timeout, 5);
}
