use app_lib::command_builder::CommandBuilder;
use app_lib::os_detector::OsType;
use proptest::prelude::*;

// ============================================================================
// Property 5: Graceful vs Force Shutdown
// For any shutdown request, if force=false, the command should be 
// "shutdown -h now", and if force=true, the command should be "poweroff".
// Validates: Requirements 1.6, 9.3, 9.4
// ============================================================================
#[test]
fn test_property_5_graceful_shutdown_linux() {
    let cmd = CommandBuilder::build_shutdown_command(OsType::Linux, false, None).unwrap();
    assert_eq!(cmd, "shutdown -h now", "Graceful shutdown should use 'shutdown -h now'");
}

#[test]
fn test_property_5_force_shutdown_linux() {
    let cmd = CommandBuilder::build_shutdown_command(OsType::Linux, true, None).unwrap();
    assert_eq!(cmd, "poweroff", "Force shutdown should use 'poweroff'");
}

#[test]
fn test_property_5_graceful_shutdown_windows() {
    let cmd = CommandBuilder::build_shutdown_command(OsType::Windows, false, None).unwrap();
    assert_eq!(cmd, "shutdown /s /t 0", "Graceful Windows shutdown should use 'shutdown /s /t 0'");
}

#[test]
fn test_property_5_force_shutdown_windows() {
    let cmd = CommandBuilder::build_shutdown_command(OsType::Windows, true, None).unwrap();
    assert_eq!(cmd, "shutdown /s /t 0 /f", "Force Windows shutdown should use 'shutdown /s /t 0 /f'");
}

proptest! {
    #[test]
    fn test_property_5_graceful_vs_force_invariant(
        os_type in prop_os_type(),
        delay in 0u32..1000u32
    ) {
        // Skip Unknown OS type
        if os_type != OsType::Unknown {
            let graceful_cmd = CommandBuilder::build_shutdown_command(os_type, false, Some(delay)).unwrap();
            let force_cmd = CommandBuilder::build_shutdown_command(os_type, true, Some(delay)).unwrap();

            // Graceful and force commands should be different
            assert_ne!(graceful_cmd, force_cmd, "Graceful and force commands should differ");

            // For Linux, graceful should not contain 'poweroff' and force should not contain 'shutdown -h now'
            if os_type == OsType::Linux {
                if delay == 0 {
                    assert_eq!(graceful_cmd, "shutdown -h now");
                    assert_eq!(force_cmd, "poweroff");
                } else {
                    assert!(graceful_cmd.contains("shutdown -h"), "Graceful should use shutdown");
                    assert!(force_cmd.contains("-F"), "Force should use -F flag");
                }
            }
        }
    }
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
    assert_eq!(cmd, "shutdown -r +5", "Scheduled reboot should include delay");
}

#[test]
fn test_property_10_scheduled_reboot_linux_force_with_delay() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, true, Some(5)).unwrap();
    assert_eq!(cmd, "shutdown -r -F +5", "Scheduled force reboot should include delay with -F flag");
}

#[test]
fn test_property_10_scheduled_reboot_windows_with_delay() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, Some(5)).unwrap();
    assert_eq!(cmd, "shutdown /r /t 300", "Windows scheduled reboot should convert minutes to seconds");
}

#[test]
fn test_property_10_scheduled_reboot_windows_force_with_delay() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, true, Some(5)).unwrap();
    assert_eq!(cmd, "shutdown /r /t 300 /f", "Windows scheduled force reboot should include /f flag");
}

#[test]
fn test_property_10_scheduled_reboot_zero_delay_linux() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, Some(0)).unwrap();
    assert_eq!(cmd, "reboot", "Zero delay should be treated as immediate reboot");
}

#[test]
fn test_property_10_scheduled_reboot_zero_delay_windows() {
    let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, Some(0)).unwrap();
    assert_eq!(cmd, "shutdown /r /t 0", "Windows zero delay should use /t 0");
}

proptest! {
    #[test]
    fn test_property_10_scheduled_reboot_delay_conversion(
        os_type in prop_os_type(),
        delay in 1u32..1000u32
    ) {
        // Skip Unknown OS type
        if os_type != OsType::Unknown {
            let cmd = CommandBuilder::build_reboot_command(os_type, false, Some(delay)).unwrap();

            // For Linux, should contain the delay value
            if os_type == OsType::Linux {
                assert!(cmd.contains(&delay.to_string()), "Linux command should contain delay value");
            }

            // For Windows, should contain delay in seconds (minutes * 60)
            if os_type == OsType::Windows {
                let delay_seconds = delay * 60;
                assert!(cmd.contains(&delay_seconds.to_string()), "Windows command should contain delay in seconds");
            }
        }
    }
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
    assert_eq!(cmd, "shutdown /r /t 0", "Graceful Windows reboot should use 'shutdown /r /t 0'");
}

proptest! {
    #[test]
    fn test_property_32_graceful_reboot_invariant(
        os_type in prop_os_type()
    ) {
        // Skip Unknown OS type
        if os_type != OsType::Unknown {
            let cmd = CommandBuilder::build_reboot_command(os_type, false, None).unwrap();

            // Graceful reboot should not contain force flags
            if os_type == OsType::Linux {
                assert!(!cmd.contains("-f"), "Graceful reboot should not contain -f flag");
                assert_eq!(cmd, "reboot");
            }

            if os_type == OsType::Windows {
                assert!(!cmd.contains("/f"), "Graceful Windows reboot should not contain /f flag");
            }
        }
    }
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
    assert_eq!(cmd, "shutdown /r /t 0 /f", "Force Windows reboot should use 'shutdown /r /t 0 /f'");
}

proptest! {
    #[test]
    fn test_property_33_force_reboot_invariant(
        os_type in prop_os_type()
    ) {
        // Skip Unknown OS type
        if os_type != OsType::Unknown {
            let cmd = CommandBuilder::build_reboot_command(os_type, true, None).unwrap();

            // Force reboot should contain force flags
            if os_type == OsType::Linux {
                assert!(cmd.contains("-f"), "Force reboot should contain -f flag");
                assert_eq!(cmd, "reboot -f");
            }

            if os_type == OsType::Windows {
                assert!(cmd.contains("/f"), "Force Windows reboot should contain /f flag");
            }
        }
    }
}

// ============================================================================
// Additional Property Tests
// ============================================================================

#[test]
fn test_command_builder_linux_shutdown_commands() {
    // Test all combinations for Linux shutdown
    let graceful_no_delay = CommandBuilder::build_shutdown_command(OsType::Linux, false, None).unwrap();
    let force_no_delay = CommandBuilder::build_shutdown_command(OsType::Linux, true, None).unwrap();
    let graceful_with_delay = CommandBuilder::build_shutdown_command(OsType::Linux, false, Some(10)).unwrap();
    let force_with_delay = CommandBuilder::build_shutdown_command(OsType::Linux, true, Some(10)).unwrap();

    assert_eq!(graceful_no_delay, "shutdown -h now");
    assert_eq!(force_no_delay, "poweroff");
    assert_eq!(graceful_with_delay, "shutdown -h +10");
    assert_eq!(force_with_delay, "shutdown -h -F +10");
}

#[test]
fn test_command_builder_linux_reboot_commands() {
    // Test all combinations for Linux reboot
    let graceful_no_delay = CommandBuilder::build_reboot_command(OsType::Linux, false, None).unwrap();
    let force_no_delay = CommandBuilder::build_reboot_command(OsType::Linux, true, None).unwrap();
    let graceful_with_delay = CommandBuilder::build_reboot_command(OsType::Linux, false, Some(10)).unwrap();
    let force_with_delay = CommandBuilder::build_reboot_command(OsType::Linux, true, Some(10)).unwrap();

    assert_eq!(graceful_no_delay, "reboot");
    assert_eq!(force_no_delay, "reboot -f");
    assert_eq!(graceful_with_delay, "shutdown -r +10");
    assert_eq!(force_with_delay, "shutdown -r -F +10");
}

#[test]
fn test_command_builder_windows_shutdown_commands() {
    // Test all combinations for Windows shutdown
    let graceful_no_delay = CommandBuilder::build_shutdown_command(OsType::Windows, false, None).unwrap();
    let force_no_delay = CommandBuilder::build_shutdown_command(OsType::Windows, true, None).unwrap();
    let graceful_with_delay = CommandBuilder::build_shutdown_command(OsType::Windows, false, Some(10)).unwrap();
    let force_with_delay = CommandBuilder::build_shutdown_command(OsType::Windows, true, Some(10)).unwrap();

    assert_eq!(graceful_no_delay, "shutdown /s /t 0");
    assert_eq!(force_no_delay, "shutdown /s /t 0 /f");
    assert_eq!(graceful_with_delay, "shutdown /s /t 600");
    assert_eq!(force_with_delay, "shutdown /s /t 600 /f");
}

#[test]
fn test_command_builder_windows_reboot_commands() {
    // Test all combinations for Windows reboot
    let graceful_no_delay = CommandBuilder::build_reboot_command(OsType::Windows, false, None).unwrap();
    let force_no_delay = CommandBuilder::build_reboot_command(OsType::Windows, true, None).unwrap();
    let graceful_with_delay = CommandBuilder::build_reboot_command(OsType::Windows, false, Some(10)).unwrap();
    let force_with_delay = CommandBuilder::build_reboot_command(OsType::Windows, true, Some(10)).unwrap();

    assert_eq!(graceful_no_delay, "shutdown /r /t 0");
    assert_eq!(force_no_delay, "shutdown /r /t 0 /f");
    assert_eq!(graceful_with_delay, "shutdown /r /t 600");
    assert_eq!(force_with_delay, "shutdown /r /t 600 /f");
}

#[test]
fn test_command_builder_cancel_commands() {
    let linux_cancel = CommandBuilder::build_cancel_command(OsType::Linux).unwrap();
    let windows_cancel = CommandBuilder::build_cancel_command(OsType::Windows).unwrap();

    assert_eq!(linux_cancel, "shutdown -c");
    assert_eq!(windows_cancel, "shutdown /a");
}

#[test]
fn test_command_builder_unknown_os_error() {
    let result = CommandBuilder::build_shutdown_command(OsType::Unknown, false, None);
    assert!(result.is_err(), "Should error for unknown OS type");

    let result = CommandBuilder::build_reboot_command(OsType::Unknown, false, None);
    assert!(result.is_err(), "Should error for unknown OS type");

    let result = CommandBuilder::build_cancel_command(OsType::Unknown);
    assert!(result.is_err(), "Should error for unknown OS type");
}

proptest! {
    #[test]
    fn prop_shutdown_command_never_empty(
        os_type in prop_os_type(),
        force in any::<bool>(),
        delay in 0u32..1000u32
    ) {
        // Skip Unknown OS type
        if os_type != OsType::Unknown {
            let cmd = CommandBuilder::build_shutdown_command(os_type, force, Some(delay)).unwrap();
            assert!(!cmd.is_empty(), "Shutdown command should never be empty");
            assert!(cmd.len() > 0, "Shutdown command should have content");
        }
    }
}

proptest! {
    #[test]
    fn prop_reboot_command_never_empty(
        os_type in prop_os_type(),
        force in any::<bool>(),
        delay in 0u32..1000u32
    ) {
        // Skip Unknown OS type
        if os_type != OsType::Unknown {
            let cmd = CommandBuilder::build_reboot_command(os_type, force, Some(delay)).unwrap();
            assert!(!cmd.is_empty(), "Reboot command should never be empty");
            assert!(cmd.len() > 0, "Reboot command should have content");
        }
    }
}

proptest! {
    #[test]
    fn prop_cancel_command_never_empty(
        os_type in prop_os_type()
    ) {
        // Skip Unknown OS type
        if os_type != OsType::Unknown {
            let cmd = CommandBuilder::build_cancel_command(os_type).unwrap();
            assert!(!cmd.is_empty(), "Cancel command should never be empty");
            assert!(cmd.len() > 0, "Cancel command should have content");
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn prop_os_type() -> impl Strategy<Value = OsType> {
    prop_oneof![
        Just(OsType::Linux),
        Just(OsType::Windows),
        Just(OsType::Unknown),
    ]
}
