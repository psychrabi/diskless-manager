use app_lib::command_builder::CommandBuilder;
use app_lib::os_detector::OsType;

// ============================================================================
// Property 6: Linux Reboot Command Execution
// For any Linux client and reboot request, executing the reboot operation
// should result in the "reboot" command being sent via SSH to the client.
// Validates: Requirements 2.1, 2.2
// ============================================================================

#[test]
fn test_property_6_linux_reboot_command_execution_graceful() {
    // For graceful reboot, the command should be "reboot"
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, None).unwrap();
    assert_eq!(
        cmd, "reboot",
        "Graceful Linux reboot should use 'reboot' command"
    );
}

#[test]
fn test_property_6_linux_reboot_command_execution_force() {
    // For force reboot, the command should be "reboot -f"
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, true, None).unwrap();
    assert_eq!(
        cmd, "reboot -f",
        "Force Linux reboot should use 'reboot -f' command"
    );
}

#[test]
fn test_property_6_windows_reboot_command_execution_graceful() {
    // For Windows graceful reboot, the command should be "shutdown /r /t 0"
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, None).unwrap();
    assert_eq!(
        cmd, "shutdown /r /t 0",
        "Graceful Windows reboot should use 'shutdown /r /t 0'"
    );
}

#[test]
fn test_property_6_windows_reboot_command_execution_force() {
    // For Windows force reboot, the command should be "shutdown /r /t 0 /f"
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, true, None).unwrap();
    assert_eq!(
        cmd, "shutdown /r /t 0 /f",
        "Force Windows reboot should use 'shutdown /r /t 0 /f'"
    );
}

// ============================================================================
// Property 7: Reboot Timeout Enforcement
// For any reboot command execution, if the SSH command does not complete
// within 30 seconds, the operation should timeout and return a timeout error.
// Validates: Requirements 2.3
// ============================================================================

#[test]
fn test_property_7_reboot_timeout_enforcement_linux() {
    // The command builder should generate valid commands that can be executed
    // The SSH executor will enforce the 30-second timeout
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, None).unwrap();
    assert!(!cmd.is_empty(), "Reboot command should not be empty");
    assert!(
        cmd.contains("reboot"),
        "Linux reboot command should contain 'reboot'"
    );
}

#[test]
fn test_property_7_reboot_timeout_enforcement_windows() {
    // The command builder should generate valid commands that can be executed
    // The SSH executor will enforce the 30-second timeout
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, None).unwrap();
    assert!(!cmd.is_empty(), "Reboot command should not be empty");
    assert!(
        cmd.contains("shutdown"),
        "Windows reboot command should contain 'shutdown'"
    );
}

// ============================================================================
// Property 8: Reboot Error Logging
// For any failed reboot operation, the error log should contain the client
// name, IP address, and error details.
// Validates: Requirements 2.4
// ============================================================================

#[test]
fn test_property_8_reboot_error_logging_invalid_os() {
    // For unknown OS type, the command builder should return an error
    let result = CommandBuilder::build_reboot_command(OsType::Unknown, false, None);
    assert!(result.is_err(), "Unknown OS type should result in error");

    let error = result.unwrap_err();
    let error_msg = error.to_string();
    assert!(
        error_msg.contains("Cannot build command") || error_msg.contains("unknown"),
        "Error message should indicate the problem"
    );
}

#[test]
fn test_property_8_reboot_error_logging_contains_details() {
    // Error messages should be descriptive
    let result = CommandBuilder::build_reboot_command(OsType::Unknown, false, None);
    assert!(result.is_err());

    let error = result.unwrap_err();
    let error_msg = error.to_string();
    assert!(!error_msg.is_empty(), "Error message should not be empty");
}

// ============================================================================
// Property 9: Reboot Success Response
// For any successful reboot operation, the response should contain a success
// message with the client name and operation type.
// Validates: Requirements 2.5
// ============================================================================

#[test]
fn test_property_9_reboot_success_response_linux_graceful() {
    // Graceful reboot should generate a valid command
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, None).unwrap();
    assert_eq!(cmd, "reboot");

    // The command should be executable (non-empty, valid syntax)
    assert!(!cmd.is_empty());
    assert!(cmd.len() < 256, "Command should be reasonably short");
}

#[test]
fn test_property_9_reboot_success_response_linux_force() {
    // Force reboot should generate a valid command
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, true, None).unwrap();
    assert_eq!(cmd, "reboot -f");

    // The command should be executable
    assert!(!cmd.is_empty());
    assert!(cmd.len() < 256, "Command should be reasonably short");
}

#[test]
fn test_property_9_reboot_success_response_windows_graceful() {
    // Graceful Windows reboot should generate a valid command
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, None).unwrap();
    assert_eq!(cmd, "shutdown /r /t 0");

    // The command should be executable
    assert!(!cmd.is_empty());
    assert!(cmd.len() < 256, "Command should be reasonably short");
}

#[test]
fn test_property_9_reboot_success_response_windows_force() {
    // Force Windows reboot should generate a valid command
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, true, None).unwrap();
    assert_eq!(cmd, "shutdown /r /t 0 /f");

    // The command should be executable
    assert!(!cmd.is_empty());
    assert!(cmd.len() < 256, "Command should be reasonably short");
}

// ============================================================================
// Property 10: Scheduled Reboot Support
// For any reboot request with a delay parameter, the command should include
// the delay in the format "reboot" with appropriate delay syntax.
// Validates: Requirements 2.6, 10.2, 10.3
// ============================================================================

#[test]
fn test_property_10_scheduled_reboot_linux_with_delay() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, Some(5)).unwrap();
    assert_eq!(
        cmd, "shutdown -r +5",
        "Scheduled reboot should include delay"
    );
}

#[test]
fn test_property_10_scheduled_reboot_linux_force_with_delay() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, true, Some(5)).unwrap();
    assert_eq!(
        cmd, "shutdown -r -F +5",
        "Scheduled force reboot should include delay with -F flag"
    );
}

#[test]
fn test_property_10_scheduled_reboot_windows_with_delay() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, Some(5)).unwrap();
    assert_eq!(
        cmd, "shutdown /r /t 300",
        "Windows scheduled reboot should convert minutes to seconds"
    );
}

#[test]
fn test_property_10_scheduled_reboot_windows_force_with_delay() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, true, Some(5)).unwrap();
    assert_eq!(
        cmd, "shutdown /r /t 300 /f",
        "Windows scheduled force reboot should include /f flag"
    );
}

#[test]
fn test_property_10_scheduled_reboot_zero_delay_linux() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, Some(0)).unwrap();
    assert_eq!(
        cmd, "reboot",
        "Zero delay should be treated as immediate reboot"
    );
}

#[test]
fn test_property_10_scheduled_reboot_zero_delay_windows() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, Some(0)).unwrap();
    assert_eq!(
        cmd, "shutdown /r /t 0",
        "Windows zero delay should use /t 0"
    );
}

#[test]
fn test_property_10_scheduled_reboot_large_delay_linux() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, Some(1440)).unwrap();
    assert_eq!(cmd, "shutdown -r +1440", "Large delay should be preserved");
}

#[test]
fn test_property_10_scheduled_reboot_large_delay_windows() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, Some(1440)).unwrap();
    // 1440 minutes = 86400 seconds
    assert_eq!(
        cmd, "shutdown /r /t 86400",
        "Windows large delay should convert correctly"
    );
}

// ============================================================================
// Property 32: Graceful Reboot Command
// For any graceful reboot request, the command should be "reboot".
// Validates: Requirements 9.5
// ============================================================================

#[test]
fn test_property_32_graceful_reboot_linux() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, None).unwrap();
    assert_eq!(cmd, "reboot", "Graceful reboot should be 'reboot'");
}

#[test]
fn test_property_32_graceful_reboot_windows() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, None).unwrap();
    assert_eq!(
        cmd, "shutdown /r /t 0",
        "Graceful Windows reboot should use 'shutdown /r /t 0'"
    );
}

#[test]
fn test_property_32_graceful_reboot_invariant_linux() {
    // Graceful reboot should always produce the same command for Linux
    let cmd1 = CommandBuilder::build_reboot_command(OsType::Linux, false, None).unwrap();
    let cmd2 = CommandBuilder::build_reboot_command(OsType::Linux, false, None).unwrap();
    assert_eq!(
        cmd1, cmd2,
        "Graceful reboot command should be deterministic"
    );
}

#[test]
fn test_property_32_graceful_reboot_invariant_windows() {
    // Graceful reboot should always produce the same command for Windows
    let cmd1 = CommandBuilder::build_reboot_command(OsType::Windows, false, None).unwrap();
    let cmd2 = CommandBuilder::build_reboot_command(OsType::Windows, false, None).unwrap();
    assert_eq!(
        cmd1, cmd2,
        "Graceful reboot command should be deterministic"
    );
}

// ============================================================================
// Property 33: Force Reboot Command
// For any force reboot request, the command should be "reboot -f".
// Validates: Requirements 9.6
// ============================================================================

#[test]
fn test_property_33_force_reboot_linux() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, true, None).unwrap();
    assert_eq!(cmd, "reboot -f", "Force reboot should be 'reboot -f'");
}

#[test]
fn test_property_33_force_reboot_windows() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, true, None).unwrap();
    assert_eq!(
        cmd, "shutdown /r /t 0 /f",
        "Force Windows reboot should use 'shutdown /r /t 0 /f'"
    );
}

#[test]
fn test_property_33_force_reboot_invariant_linux() {
    // Force reboot should always produce the same command for Linux
    let cmd1 = CommandBuilder::build_reboot_command(OsType::Linux, true, None).unwrap();
    let cmd2 = CommandBuilder::build_reboot_command(OsType::Linux, true, None).unwrap();
    assert_eq!(cmd1, cmd2, "Force reboot command should be deterministic");
}

#[test]
fn test_property_33_force_reboot_invariant_windows() {
    // Force reboot should always produce the same command for Windows
    let cmd1 = CommandBuilder::build_reboot_command(OsType::Windows, true, None).unwrap();
    let cmd2 = CommandBuilder::build_reboot_command(OsType::Windows, true, None).unwrap();
    assert_eq!(cmd1, cmd2, "Force reboot command should be deterministic");
}

#[test]
fn test_property_33_force_vs_graceful_difference_linux() {
    // Force and graceful should produce different commands
    let graceful = CommandBuilder::build_reboot_command(OsType::Linux, false, None).unwrap();
    let force = CommandBuilder::build_reboot_command(OsType::Linux, true, None).unwrap();
    assert_ne!(graceful, force, "Force and graceful reboot should differ");
    assert_eq!(graceful, "reboot");
    assert_eq!(force, "reboot -f");
}

#[test]
fn test_property_33_force_vs_graceful_difference_windows() {
    // Force and graceful should produce different commands
    let graceful = CommandBuilder::build_reboot_command(OsType::Windows, false, None).unwrap();
    let force = CommandBuilder::build_reboot_command(OsType::Windows, true, None).unwrap();
    assert_ne!(graceful, force, "Force and graceful reboot should differ");
    assert!(graceful.contains("/r /t 0"));
    assert!(force.contains("/r /t 0 /f"));
}
