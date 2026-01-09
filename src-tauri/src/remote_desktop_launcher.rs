use crate::error::AppError;
use crate::os_detector::OsType;
use crate::ssh_executor::SshExecutor;
use crate::types::Client;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Remote desktop protocol types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RemoteDesktopProtocol {
    VNC,
    RDP,
    SSH,
}

impl std::fmt::Display for RemoteDesktopProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteDesktopProtocol::VNC => write!(f, "VNC"),
            RemoteDesktopProtocol::RDP => write!(f, "RDP"),
            RemoteDesktopProtocol::SSH => write!(f, "SSH"),
        }
    }
}

/// Response from a remote desktop launch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteDesktopResponse {
    pub success: bool,
    pub protocol_used: String,
    pub message: String,
    pub timestamp: String,
}

/// Remote desktop launcher for managing VNC, RDP, and SSH access
pub struct RemoteDesktopLauncher {
    ssh_executor: Arc<SshExecutor>,
}

impl RemoteDesktopLauncher {
    /// Create a new remote desktop launcher
    pub fn new(ssh_executor: Arc<SshExecutor>) -> Self {
        Self { ssh_executor }
    }

    /// Launch remote desktop for a client
    ///
    /// # Arguments
    /// * `client` - The client to launch remote desktop for
    /// * `os_type` - The OS type of the client
    ///
    /// # Returns
    /// A RemoteDesktopResponse with the result of the operation
    pub async fn launch_remote_desktop(
        &self,
        client: &Client,
        os_type: OsType,
    ) -> Result<RemoteDesktopResponse, AppError> {
        debug!(
            "Launching remote desktop for client {} ({}) with OS type {:?}",
            client.name, client.ip, os_type
        );

        // Detect available protocols
        let available_protocols = self.detect_protocols(&client.ip, os_type).await?;

        if available_protocols.is_empty() {
            warn!(
                "No remote desktop protocols available for client {} ({})",
                client.name, client.ip
            );
            return Err(AppError::Command(
                "No remote desktop protocols available".to_string(),
            ));
        }

        // Try protocols in order of preference
        for protocol in available_protocols {
            match protocol {
                RemoteDesktopProtocol::VNC => {
                    match self.launch_vnc(&client.ip) {
                        Ok(_) => {
                            info!(
                                "VNC client launched for client {} ({})",
                                client.name, client.ip
                            );
                            return Ok(RemoteDesktopResponse {
                                success: true,
                                protocol_used: "VNC".to_string(),
                                message: format!(
                                    "VNC client launched for {} ({})",
                                    client.name, client.ip
                                ),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            });
                        }
                        Err(e) => {
                            warn!("Failed to launch VNC for {}: {}", client.ip, e);
                            continue;
                        }
                    }
                }
                RemoteDesktopProtocol::RDP => {
                    match self.launch_rdp(&client.ip) {
                        Ok(_) => {
                            info!(
                                "RDP client launched for client {} ({})",
                                client.name, client.ip
                            );
                            return Ok(RemoteDesktopResponse {
                                success: true,
                                protocol_used: "RDP".to_string(),
                                message: format!(
                                    "RDP client launched for {} ({})",
                                    client.name, client.ip
                                ),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            });
                        }
                        Err(e) => {
                            warn!("Failed to launch RDP for {}: {}", client.ip, e);
                            continue;
                        }
                    }
                }
                RemoteDesktopProtocol::SSH => {
                    match self.launch_ssh_terminal(&client.ip) {
                        Ok(_) => {
                            info!(
                                "SSH terminal launched for client {} ({})",
                                client.name, client.ip
                            );
                            return Ok(RemoteDesktopResponse {
                                success: true,
                                protocol_used: "SSH".to_string(),
                                message: format!(
                                    "SSH terminal launched for {} ({})",
                                    client.name, client.ip
                                ),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            });
                        }
                        Err(e) => {
                            warn!("Failed to launch SSH terminal for {}: {}", client.ip, e);
                            continue;
                        }
                    }
                }
            }
        }

        error!(
            "Failed to launch any remote desktop protocol for client {} ({})",
            client.name, client.ip
        );
        Err(AppError::Command(
            "Failed to launch remote desktop with any available protocol".to_string(),
        ))
    }

    /// Detect available remote desktop protocols on a client
    ///
    /// # Arguments
    /// * `client_ip` - The IP address of the client
    /// * `os_type` - The OS type of the client
    ///
    /// # Returns
    /// A vector of available protocols
    async fn detect_protocols(
        &self,
        client_ip: &str,
        os_type: OsType,
    ) -> Result<Vec<RemoteDesktopProtocol>, AppError> {
        debug!(
            "Detecting remote desktop protocols for {} (OS: {:?})",
            client_ip, os_type
        );

        let mut protocols = Vec::new();

        match os_type {
            OsType::Linux => {
                // For Linux, check for VNC first, then RDP (xrdp), then SSH fallback
                if self.check_vnc_available(client_ip).await {
                    debug!("VNC detected on Linux client {}", client_ip);
                    protocols.push(RemoteDesktopProtocol::VNC);
                }

                if self.check_rdp_available(client_ip).await {
                    debug!("RDP (xrdp) detected on Linux client {}", client_ip);
                    protocols.push(RemoteDesktopProtocol::RDP);
                }

                // SSH is always available as fallback for Linux
                debug!("SSH available as fallback for Linux client {}", client_ip);
                protocols.push(RemoteDesktopProtocol::SSH);
            }
            OsType::Windows => {
                // For Windows, check for RDP first, then SSH (if available), then VNC
                if self.check_rdp_available(client_ip).await {
                    debug!("RDP detected on Windows client {}", client_ip);
                    protocols.push(RemoteDesktopProtocol::RDP);
                }

                if self.check_vnc_available(client_ip).await {
                    debug!("VNC detected on Windows client {}", client_ip);
                    protocols.push(RemoteDesktopProtocol::VNC);
                }

                // SSH may not be available on Windows, but try it as fallback
                if self.check_ssh_available(client_ip).await {
                    debug!("SSH available as fallback for Windows client {}", client_ip);
                    protocols.push(RemoteDesktopProtocol::SSH);
                }
            }
            OsType::Unknown => {
                // For unknown OS, try all protocols
                debug!("Unknown OS type for client {}, trying all protocols", client_ip);
                if self.check_vnc_available(client_ip).await {
                    protocols.push(RemoteDesktopProtocol::VNC);
                }
                if self.check_rdp_available(client_ip).await {
                    protocols.push(RemoteDesktopProtocol::RDP);
                }
                if self.check_ssh_available(client_ip).await {
                    protocols.push(RemoteDesktopProtocol::SSH);
                }
            }
        }

        info!(
            "Available protocols for {}: {:?}",
            client_ip, protocols
        );

        Ok(protocols)
    }

    /// Check if VNC is available on the client
    async fn check_vnc_available(&self, client_ip: &str) -> bool {
        debug!("Checking VNC availability on {}", client_ip);

        // Check if VNC port (5900) is open
        match tokio::net::TcpStream::connect(format!("{}:5900", client_ip)).await {
            Ok(_) => {
                debug!("VNC port 5900 is open on {}", client_ip);
                true
            }
            Err(_) => {
                debug!("VNC port 5900 is not open on {}", client_ip);
                false
            }
        }
    }

    /// Check if RDP is available on the client
    async fn check_rdp_available(&self, client_ip: &str) -> bool {
        debug!("Checking RDP availability on {}", client_ip);

        // Check if RDP port (3389) is open
        match tokio::net::TcpStream::connect(format!("{}:3389", client_ip)).await {
            Ok(_) => {
                debug!("RDP port 3389 is open on {}", client_ip);
                true
            }
            Err(_) => {
                debug!("RDP port 3389 is not open on {}", client_ip);
                false
            }
        }
    }

    /// Check if SSH is available on the client
    async fn check_ssh_available(&self, client_ip: &str) -> bool {
        debug!("Checking SSH availability on {}", client_ip);

        match self.ssh_executor.check_connectivity(client_ip).await {
            Ok(available) => {
                if available {
                    debug!("SSH is available on {}", client_ip);
                } else {
                    debug!("SSH is not available on {}", client_ip);
                }
                available
            }
            Err(_) => {
                debug!("SSH connectivity check failed for {}", client_ip);
                false
            }
        }
    }

    /// Launch VNC client for the given client IP
    fn launch_vnc(&self, client_ip: &str) -> Result<(), AppError> {
        debug!("Launching VNC client for {}", client_ip);

        // Try common VNC clients
        let vnc_clients = vec!["vncviewer", "vinagre", "krdc", "remmina"];

        for client in vnc_clients {
            match Command::new(client)
                .arg(format!("{}:5900", client_ip))
                .spawn()
            {
                Ok(_) => {
                    info!("VNC client {} launched successfully for {}", client, client_ip);
                    return Ok(());
                }
                Err(e) => {
                    debug!("Failed to launch VNC client {}: {}", client, e);
                    continue;
                }
            }
        }

        error!("Failed to launch any VNC client for {}", client_ip);
        Err(AppError::Command(
            "No VNC client found on system".to_string(),
        ))
    }

    /// Launch RDP client for the given client IP
    fn launch_rdp(&self, client_ip: &str) -> Result<(), AppError> {
        debug!("Launching RDP client for {}", client_ip);

        // Try common RDP clients
        let rdp_clients = vec!["rdesktop", "xfreerdp", "krdc", "remmina"];

        for client in rdp_clients {
            match Command::new(client)
                .arg(client_ip)
                .spawn()
            {
                Ok(_) => {
                    info!("RDP client {} launched successfully for {}", client, client_ip);
                    return Ok(());
                }
                Err(e) => {
                    debug!("Failed to launch RDP client {}: {}", client, e);
                    continue;
                }
            }
        }

        error!("Failed to launch any RDP client for {}", client_ip);
        Err(AppError::Command(
            "No RDP client found on system".to_string(),
        ))
    }

    /// Launch SSH terminal for the given client IP
    fn launch_ssh_terminal(&self, client_ip: &str) -> Result<(), AppError> {
        debug!("Launching SSH terminal for {}", client_ip);

        // Try common terminal emulators with SSH
        let terminal_commands = vec![
            ("gnome-terminal", vec!["--", "ssh", "root@{}"]),
            ("xterm", vec!["-e", "ssh root@{}"]),
            ("konsole", vec!["-e", "ssh root@{}"]),
            ("xfce4-terminal", vec!["-e", "ssh root@{}"]),
            ("mate-terminal", vec!["-e", "ssh root@{}"]),
        ];

        for (terminal, args) in terminal_commands {
            // Replace {} with client IP in arguments
            let replaced_args: Vec<String> = args
                .iter()
                .map(|arg| {
                    if arg.contains("{}") {
                        arg.replace("{}", client_ip)
                    } else {
                        arg.to_string()
                    }
                })
                .collect();

            match Command::new(terminal)
                .args(&replaced_args)
                .spawn()
            {
                Ok(_) => {
                    info!("SSH terminal {} launched successfully for {}", terminal, client_ip);
                    return Ok(());
                }
                Err(e) => {
                    debug!("Failed to launch SSH terminal {}: {}", terminal, e);
                    continue;
                }
            }
        }

        error!("Failed to launch any SSH terminal for {}", client_ip);
        Err(AppError::Command(
            "No terminal emulator found on system".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_desktop_protocol_display() {
        assert_eq!(RemoteDesktopProtocol::VNC.to_string(), "VNC");
        assert_eq!(RemoteDesktopProtocol::RDP.to_string(), "RDP");
        assert_eq!(RemoteDesktopProtocol::SSH.to_string(), "SSH");
    }

    #[test]
    fn test_remote_desktop_response_creation() {
        let response = RemoteDesktopResponse {
            success: true,
            protocol_used: "VNC".to_string(),
            message: "VNC client launched".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        assert!(response.success);
        assert_eq!(response.protocol_used, "VNC");
        assert_eq!(response.message, "VNC client launched");
    }

    #[test]
    fn test_remote_desktop_launcher_creation() {
        let ssh_executor = Arc::new(SshExecutor::new());
        let launcher = RemoteDesktopLauncher::new(ssh_executor);
        // Just verify it can be created
        assert!(true);
    }

    #[test]
    fn test_protocol_serialization() {
        let protocol = RemoteDesktopProtocol::VNC;
        let json = serde_json::to_string(&protocol).unwrap();
        assert_eq!(json, "\"VNC\"");

        let deserialized: RemoteDesktopProtocol = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, RemoteDesktopProtocol::VNC);
    }
}
