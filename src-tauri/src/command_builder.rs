use crate::error::AppError;
use crate::os_detector::OsType;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Operation type for control commands
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationType {
    Shutdown,
    Reboot,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Shutdown => write!(f, "shutdown"),
            OperationType::Reboot => write!(f, "reboot"),
        }
    }
}

/// Command builder for generating OS-specific control commands
///
/// This builder generates appropriate shutdown and reboot commands based on the
/// target operating system. It supports both graceful and force operations, as well
/// as scheduled operations with delay parameters.
pub struct CommandBuilder;

impl CommandBuilder {
    /// Build a shutdown command for the specified OS
    ///
    /// # Arguments
    /// * `os_type` - The target operating system
    /// * `force` - If true, use force shutdown; if false, use graceful shutdown
    /// * `delay_minutes` - Optional delay in minutes before shutdown
    ///
    /// # Returns
    /// The appropriate shutdown command for the OS
    pub fn build_shutdown_command(
        os_type: OsType,
        force: bool,
        delay_minutes: Option<u32>,
    ) -> Result<String, AppError> {
        debug!(
            "Building shutdown command for {:?} (force={}, delay={:?})",
            os_type, force, delay_minutes
        );

        let command = match os_type {
            OsType::Linux => Self::build_linux_shutdown(force, delay_minutes)?,
            OsType::Windows => Self::build_windows_shutdown(force, delay_minutes)?,
            OsType::Unknown => {
                return Err(AppError::Validation(
                    "Cannot build command for unknown OS type".to_string(),
                ))
            }
        };

        info!("Built shutdown command for {:?}: {}", os_type, command);
        Ok(command)
    }

    /// Build a reboot command for the specified OS
    ///
    /// # Arguments
    /// * `os_type` - The target operating system
    /// * `force` - If true, use force reboot; if false, use graceful reboot
    /// * `delay_minutes` - Optional delay in minutes before reboot
    ///
    /// # Returns
    /// The appropriate reboot command for the OS
    pub fn build_reboot_command(
        os_type: OsType,
        force: bool,
        delay_minutes: Option<u32>,
    ) -> Result<String, AppError> {
        debug!(
            "Building reboot command for {:?} (force={}, delay={:?})",
            os_type, force, delay_minutes
        );

        let command = match os_type {
            OsType::Linux => Self::build_linux_reboot(force, delay_minutes)?,
            OsType::Windows => Self::build_windows_reboot(force, delay_minutes)?,
            OsType::Unknown => {
                return Err(AppError::Validation(
                    "Cannot build command for unknown OS type".to_string(),
                ))
            }
        };

        info!("Built reboot command for {:?}: {}", os_type, command);
        Ok(command)
    }

    /// Build a Linux shutdown command
    ///
    /// Graceful: `shutdown -h now` (allows processes to terminate)
    /// Force: `poweroff` (immediate shutdown)
    /// Scheduled: `shutdown -h +<minutes>` (shutdown after delay)
    fn build_linux_shutdown(force: bool, delay_minutes: Option<u32>) -> Result<String, AppError> {
        let command = match (force, delay_minutes) {
            // Graceful shutdown without delay
            (false, None) => "shutdown -h now".to_string(),
            // Force shutdown without delay
            (true, None) => "poweroff".to_string(),
            // Graceful shutdown with delay
            (false, Some(delay)) => {
                if delay == 0 {
                    "shutdown -h now".to_string()
                } else {
                    format!("shutdown -h +{}", delay)
                }
            }
            // Force shutdown with delay (use shutdown -h with force flag)
            (true, Some(delay)) => {
                if delay == 0 {
                    "poweroff".to_string()
                } else {
                    format!("shutdown -h -F +{}", delay)
                }
            }
        };

        Ok(command)
    }

    /// Build a Linux reboot command
    ///
    /// Graceful: `reboot` (allows processes to terminate)
    /// Force: `reboot -f` (immediate reboot)
    /// Scheduled: `shutdown -r +<minutes>` (reboot after delay)
    fn build_linux_reboot(force: bool, delay_minutes: Option<u32>) -> Result<String, AppError> {
        let command = match (force, delay_minutes) {
            // Graceful reboot without delay
            (false, None) => "reboot".to_string(),
            // Force reboot without delay
            (true, None) => "reboot -f".to_string(),
            // Graceful reboot with delay
            (false, Some(delay)) => {
                if delay == 0 {
                    "reboot".to_string()
                } else {
                    format!("shutdown -r +{}", delay)
                }
            }
            // Force reboot with delay
            (true, Some(delay)) => {
                if delay == 0 {
                    "reboot -f".to_string()
                } else {
                    format!("shutdown -r -F +{}", delay)
                }
            }
        };

        Ok(command)
    }

    /// Build a Windows shutdown command
    ///
    /// Uses `shutdown` command with appropriate flags
    fn build_windows_shutdown(force: bool, delay_minutes: Option<u32>) -> Result<String, AppError> {
        let force_flag = if force { "/f" } else { "" };
        let delay_seconds = delay_minutes.unwrap_or(0) * 60;

        let command = if delay_seconds > 0 {
            format!("shutdown /s /t {} {}", delay_seconds, force_flag)
                .trim()
                .to_string()
        } else {
            format!("shutdown /s /t 0 {}", force_flag)
                .trim()
                .to_string()
        };

        Ok(command)
    }

    /// Build a Windows reboot command
    ///
    /// Uses `shutdown` command with reboot flag
    fn build_windows_reboot(force: bool, delay_minutes: Option<u32>) -> Result<String, AppError> {
        let force_flag = if force { "/f" } else { "" };
        let delay_seconds = delay_minutes.unwrap_or(0) * 60;

        let command = if delay_seconds > 0 {
            format!("shutdown /r /t {} {}", delay_seconds, force_flag)
                .trim()
                .to_string()
        } else {
            format!("shutdown /r /t 0 {}", force_flag)
                .trim()
                .to_string()
        };

        Ok(command)
    }

    /// Build a cancel command for a scheduled operation
    ///
    /// # Arguments
    /// * `os_type` - The target operating system
    ///
    /// # Returns
    /// The appropriate cancel command for the OS
    pub fn build_cancel_command(os_type: OsType) -> Result<String, AppError> {
        debug!("Building cancel command for {:?}", os_type);

        let command = match os_type {
            OsType::Linux => "shutdown -c".to_string(),
            OsType::Windows => "shutdown /a".to_string(),
            OsType::Unknown => {
                return Err(AppError::Validation(
                    "Cannot build command for unknown OS type".to_string(),
                ))
            }
        };

        info!("Built cancel command for {:?}: {}", os_type, command);
        Ok(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Linux Shutdown Tests
    #[test]
    fn test_linux_graceful_shutdown_no_delay() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Linux, false, None).unwrap();
        assert_eq!(cmd, "shutdown -h now");
    }

    #[test]
    fn test_linux_force_shutdown_no_delay() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Linux, true, None).unwrap();
        assert_eq!(cmd, "poweroff");
    }

    #[test]
    fn test_linux_graceful_shutdown_with_delay() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Linux, false, Some(5)).unwrap();
        assert_eq!(cmd, "shutdown -h +5");
    }

    #[test]
    fn test_linux_force_shutdown_with_delay() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Linux, true, Some(5)).unwrap();
        assert_eq!(cmd, "shutdown -h -F +5");
    }

    #[test]
    fn test_linux_shutdown_with_zero_delay() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Linux, false, Some(0)).unwrap();
        assert_eq!(cmd, "shutdown -h now");
    }

    // Linux Reboot Tests
    #[test]
    fn test_linux_graceful_reboot_no_delay() {
        let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, None).unwrap();
        assert_eq!(cmd, "reboot");
    }

    #[test]
    fn test_linux_force_reboot_no_delay() {
        let cmd = CommandBuilder::build_reboot_command(OsType::Linux, true, None).unwrap();
        assert_eq!(cmd, "reboot -f");
    }

    #[test]
    fn test_linux_graceful_reboot_with_delay() {
        let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, Some(5)).unwrap();
        assert_eq!(cmd, "shutdown -r +5");
    }

    #[test]
    fn test_linux_force_reboot_with_delay() {
        let cmd = CommandBuilder::build_reboot_command(OsType::Linux, true, Some(5)).unwrap();
        assert_eq!(cmd, "shutdown -r -F +5");
    }

    #[test]
    fn test_linux_reboot_with_zero_delay() {
        let cmd = CommandBuilder::build_reboot_command(OsType::Linux, false, Some(0)).unwrap();
        assert_eq!(cmd, "reboot");
    }

    // Windows Shutdown Tests
    #[test]
    fn test_windows_graceful_shutdown_no_delay() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Windows, false, None).unwrap();
        assert_eq!(cmd, "shutdown /s /t 0");
    }

    #[test]
    fn test_windows_force_shutdown_no_delay() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Windows, true, None).unwrap();
        assert_eq!(cmd, "shutdown /s /t 0 /f");
    }

    #[test]
    fn test_windows_graceful_shutdown_with_delay() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Windows, false, Some(5)).unwrap();
        assert_eq!(cmd, "shutdown /s /t 300");
    }

    #[test]
    fn test_windows_force_shutdown_with_delay() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Windows, true, Some(5)).unwrap();
        assert_eq!(cmd, "shutdown /s /t 300 /f");
    }

    // Windows Reboot Tests
    #[test]
    fn test_windows_graceful_reboot_no_delay() {
        let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, None).unwrap();
        assert_eq!(cmd, "shutdown /r /t 0");
    }

    #[test]
    fn test_windows_force_reboot_no_delay() {
        let cmd = CommandBuilder::build_reboot_command(OsType::Windows, true, None).unwrap();
        assert_eq!(cmd, "shutdown /r /t 0 /f");
    }

    #[test]
    fn test_windows_graceful_reboot_with_delay() {
        let cmd = CommandBuilder::build_reboot_command(OsType::Windows, false, Some(5)).unwrap();
        assert_eq!(cmd, "shutdown /r /t 300");
    }

    #[test]
    fn test_windows_force_reboot_with_delay() {
        let cmd = CommandBuilder::build_reboot_command(OsType::Windows, true, Some(5)).unwrap();
        assert_eq!(cmd, "shutdown /r /t 300 /f");
    }

    // Cancel Command Tests
    #[test]
    fn test_linux_cancel_command() {
        let cmd = CommandBuilder::build_cancel_command(OsType::Linux).unwrap();
        assert_eq!(cmd, "shutdown -c");
    }

    #[test]
    fn test_windows_cancel_command() {
        let cmd = CommandBuilder::build_cancel_command(OsType::Windows).unwrap();
        assert_eq!(cmd, "shutdown /a");
    }

    // Error Cases
    #[test]
    fn test_unknown_os_shutdown_error() {
        let result = CommandBuilder::build_shutdown_command(OsType::Unknown, false, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_os_reboot_error() {
        let result = CommandBuilder::build_reboot_command(OsType::Unknown, false, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_os_cancel_error() {
        let result = CommandBuilder::build_cancel_command(OsType::Unknown);
        assert!(result.is_err());
    }

    // Edge Cases
    #[test]
    fn test_large_delay_value() {
        let cmd = CommandBuilder::build_shutdown_command(OsType::Linux, false, Some(1440)).unwrap();
        assert_eq!(cmd, "shutdown -h +1440");
    }

    #[test]
    fn test_windows_large_delay_value() {
        let cmd =
            CommandBuilder::build_shutdown_command(OsType::Windows, false, Some(1440)).unwrap();
        assert_eq!(cmd, "shutdown /s /t 86400");
    }
}
