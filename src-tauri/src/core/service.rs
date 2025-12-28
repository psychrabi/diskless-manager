use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: String,
    pub running: bool,
    pub enabled: bool,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub active: bool,
    pub status: String,
    pub pid: Option<u32>,
    pub memory: Option<String>,
    pub uptime: Option<String>,
}

pub struct ServiceManager;

impl ServiceManager {
    pub fn new() -> Self {
        Self
    }

    pub fn list_services(&self) -> Vec<ServiceInfo> {
        let services = vec![
            ("isc-dhcp-server", "DHCP Server"),
            ("tftpd-hpa", "TFTP Server"),
            ("rtslib-fb-targetctl", "iSCSI Target (LIO)"),
            ("nfs-kernel-server", "NFS Server"),
            ("smbd", "Samba Server"),
            ("apache2", "Apache2 HTTP Server"),
        ];

        services
            .into_iter()
            .map(|(name, display_name)| {
                let running = self.is_running(name);
                let enabled = self.is_enabled(name);
                let pid = if running { self.get_pid(name) } else { None };

                ServiceInfo {
                    name: name.to_string(),
                    display_name: display_name.to_string(),
                    running,
                    enabled,
                    pid,
                }
            })
            .collect()
    }

    pub fn get_status(&self, name: &str) -> anyhow::Result<ServiceStatus> {
        let output = Command::new("systemctl").args(["status", name]).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let active = self.is_running(name);
        let pid = if active { self.get_pid(name) } else { None };

        let memory = stdout
            .lines()
            .find(|l| l.contains("Memory:"))
            .map(|l| l.trim().to_string());

        let uptime = stdout
            .lines()
            .find(|l| l.contains("Active:"))
            .and_then(|l| l.split(';').nth(1))
            .map(|s| s.trim().to_string());

        Ok(ServiceStatus {
            name: name.to_string(),
            active,
            status: if active { "running" } else { "stopped" }.to_string(),
            pid,
            memory,
            uptime,
        })
    }

    pub fn start(&self, name: &str) -> anyhow::Result<()> {
        let output = Command::new("sudo")
            .arg("systemctl")
            .arg("start")
            .arg(name)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to start {}: {}", name, stderr));
        }

        tracing::info!("Service '{}' started", name);
        Ok(())
    }

    pub fn stop(&self, name: &str) -> anyhow::Result<()> {
        let output = Command::new("sudo")
            .arg("systemctl")
            .arg("stop")
            .arg(name)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to stop {}: {}", name, stderr));
        }

        tracing::info!("Service '{}' stopped", name);
        Ok(())
    }

    pub fn restart(&self, name: &str) -> anyhow::Result<()> {
        let output = Command::new("sudo")
            .arg("systemctl")
            .arg("restart")
            .arg(name)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to restart {}: {}", name, stderr));
        }

        tracing::info!("Service '{}' restarted", name);
        Ok(())
    }

    pub fn start_all(&self) -> anyhow::Result<Vec<String>> {
        let services = [
            "isc-dhcp-server",
            "tftpd-hpa",
            "rtslib-fb-targetctl",
            "nfs-kernel-server",
            "smbd",
            "apache2",
        ];
        let mut started = Vec::new();

        for service in services {
            if !self.is_running(service) && self.start(service).is_ok() {
                started.push(service.to_string());
            }
        }

        Ok(started)
    }

    pub fn stop_all(&self) -> anyhow::Result<Vec<String>> {
        let services = [
            "apache2",
            "smbd",
            "nfs-kernel-server",
            "rtslib-fb-targetctl",
            "tftpd-hpa",
            "isc-dhcp-server",
        ];
        let mut stopped = Vec::new();

        for service in services {
            if self.is_running(service) && self.stop(service).is_ok() {
                stopped.push(service.to_string());
            }
        }

        Ok(stopped)
    }

    fn is_running(&self, name: &str) -> bool {
        Command::new("systemctl")
            .args(["is-active", "--quiet", name])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn is_enabled(&self, name: &str) -> bool {
        Command::new("systemctl")
            .args(["is-enabled", "--quiet", name])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn get_pid(&self, name: &str) -> Option<u32> {
        let output = Command::new("systemctl")
            .args(["show", "-p", "MainPID", name])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .strip_prefix("MainPID=")
            .and_then(|s| s.trim().parse().ok())
            .filter(|&pid| pid > 0)
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}
