use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: String,
    pub running: bool,
    pub enabled: bool,
    pub pid: Option<u32>,
    /// Whether the service starts automatically on boot (systemd enable).
    /// Distinct from `enabled`, which tracks app-settings management.
    #[serde(default)]
    pub starts_on_boot: bool,
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
