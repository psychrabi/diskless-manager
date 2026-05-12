use crate::core::client::Client;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Operating system type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsType {
    Linux,
    Windows,
    Unknown,
}

impl std::fmt::Display for OsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsType::Linux => write!(f, "linux"),
            OsType::Windows => write!(f, "windows"),
            OsType::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for OsType {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "linux" => Ok(OsType::Linux),
            "windows" => Ok(OsType::Windows),
            "unknown" => Ok(OsType::Unknown),
            _ => Err(AppError::Validation(format!("Unknown OS type: {}", s))),
        }
    }
}

/// OS detector that retrieves OS type from master image metadata
///
/// This detector uses the OS type stored in the master image assigned to a client,
/// avoiding the need for runtime detection. The OS type is set when the master image
/// is created and is cached in the image metadata.
pub struct OsDetector;

impl OsDetector {
    /// Get OS type for a client from its assigned master image
    ///
    /// # Arguments
    /// * `client` - The client to get OS type for
    /// * `master_os` - The OS type from the master image (e.g., "linux", "windows")
    ///
    /// # Returns
    /// The OS type of the client based on its master image
    pub fn get_os_type(client: &Client, master_os: Option<&str>) -> OsType {
        debug!("Getting OS type for client {}", client.name);

        match master_os {
            Some(os_str) => match os_str.to_lowercase().as_str() {
                "linux" => {
                    info!("Client {} has Linux master image", client.name);
                    OsType::Linux
                }
                "windows" => {
                    info!("Client {} has Windows master image", client.name);
                    OsType::Windows
                }
                _ => {
                    warn!("Client {} has unknown OS type: {}", client.name, os_str);
                    OsType::Unknown
                }
            },
            None => {
                warn!(
                    "Client {} has no master image OS type specified",
                    client.name
                );
                OsType::Unknown
            }
        }
    }

    /// Parse OS type string to OsType enum
    pub fn parse_os_type(os_str: &str) -> OsType {
        match os_str.to_lowercase().as_str() {
            "linux" => OsType::Linux,
            "windows" => OsType::Windows,
            _ => OsType::Unknown,
        }
    }

    /// Determine OS type with fallback logic
    ///
    /// If the master image OS type is unknown, attempts fallback to Windows
    /// (for backward compatibility with existing Windows clients)
    pub fn get_os_type_with_fallback(client: &Client, master_os: Option<&str>) -> OsType {
        let os_type = Self::get_os_type(client, master_os);

        match os_type {
            OsType::Unknown => {
                info!(
                    "Client {} has unknown OS type, falling back to Windows for compatibility",
                    client.name
                );
                OsType::Windows
            }
            _ => os_type,
        }
    }
}

impl Default for OsDetector {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_os_type_display() {
        assert_eq!(OsType::Linux.to_string(), "linux");
        assert_eq!(OsType::Windows.to_string(), "windows");
        assert_eq!(OsType::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_os_type_from_str() {
        assert_eq!("linux".parse::<OsType>().unwrap(), OsType::Linux);
        assert_eq!("windows".parse::<OsType>().unwrap(), OsType::Windows);
        assert_eq!("unknown".parse::<OsType>().unwrap(), OsType::Unknown);
        assert_eq!("LINUX".parse::<OsType>().unwrap(), OsType::Linux);
        assert!("invalid".parse::<OsType>().is_err());
    }

    #[test]
    fn test_parse_os_type() {
        assert_eq!(OsDetector::parse_os_type("linux"), OsType::Linux);
        assert_eq!(OsDetector::parse_os_type("LINUX"), OsType::Linux);
        assert_eq!(OsDetector::parse_os_type("windows"), OsType::Windows);
        assert_eq!(OsDetector::parse_os_type("WINDOWS"), OsType::Windows);
        assert_eq!(OsDetector::parse_os_type("unknown"), OsType::Unknown);
        assert_eq!(OsDetector::parse_os_type("invalid"), OsType::Unknown);
    }

    #[test]
    fn test_get_os_type_from_master() {
        let client = Client {
            id: "1".to_string(),
            name: "test-client".to_string(),
            mac: "00:11:22:33:44:55".to_string(),
            ip: "192.168.1.100".to_string(),
            master: "linux-master".to_string(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            snapshot: None,
            block_store: None,
            target_iqn: None,
            writeback: None,
            last_modified: None,
            block_device: None,
            status: Some("Online".to_string()),
            mode: None,
            pxe_mode: None,
            keep_writeback: None,
            use_game_disk: None,
        };

        // Test Linux OS type
        assert_eq!(
            OsDetector::get_os_type(&client, Some("linux")),
            OsType::Linux
        );

        // Test Windows OS type
        assert_eq!(
            OsDetector::get_os_type(&client, Some("windows")),
            OsType::Windows
        );

        // Test Unknown OS type
        assert_eq!(
            OsDetector::get_os_type(&client, Some("unknown")),
            OsType::Unknown
        );

        // Test None OS type
        assert_eq!(OsDetector::get_os_type(&client, None), OsType::Unknown);

        // Test case insensitivity
        assert_eq!(
            OsDetector::get_os_type(&client, Some("LINUX")),
            OsType::Linux
        );
        assert_eq!(
            OsDetector::get_os_type(&client, Some("Windows")),
            OsType::Windows
        );
    }

    #[test]
    fn test_get_os_type_with_fallback() {
        let client = Client {
            id: "1".to_string(),
            name: "test-client".to_string(),
            mac: "00:11:22:33:44:55".to_string(),
            ip: "192.168.1.100".to_string(),
            master: "master".to_string(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            snapshot: None,
            block_store: None,
            target_iqn: None,
            writeback: None,
            last_modified: None,
            block_device: None,
            status: Some("Online".to_string()),
            mode: None,
            pxe_mode: None,
            keep_writeback: None,
            use_game_disk: None,
        };

        // Test Linux OS type (no fallback needed)
        assert_eq!(
            OsDetector::get_os_type_with_fallback(&client, Some("linux")),
            OsType::Linux
        );

        // Test Windows OS type (no fallback needed)
        assert_eq!(
            OsDetector::get_os_type_with_fallback(&client, Some("windows")),
            OsType::Windows
        );

        // Test Unknown OS type (fallback to Windows)
        assert_eq!(
            OsDetector::get_os_type_with_fallback(&client, Some("unknown")),
            OsType::Windows
        );

        // Test None OS type (fallback to Windows)
        assert_eq!(
            OsDetector::get_os_type_with_fallback(&client, None),
            OsType::Windows
        );
    }

    #[test]
    fn test_os_type_serialization() {
        let os_type = OsType::Linux;
        let json = serde_json::to_string(&os_type).unwrap();
        assert_eq!(json, "\"linux\"");

        let deserialized: OsType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, OsType::Linux);
    }
}
