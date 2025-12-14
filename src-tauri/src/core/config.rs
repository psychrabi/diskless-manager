use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub server: ServerConfig,
    pub dhcp: DhcpConfig,
    pub tftp: TftpConfig,
    pub iscsi: IscsiConfig,
    pub nfs: NfsConfig,
    #[serde(default)]
    pub samba: SambaConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub interface: String,
    pub ip_address: String,
    pub hostname: String,
    pub domain: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            interface: "eth0".to_string(),
            ip_address: "192.168.1.1".to_string(),
            hostname: "pxeserver".to_string(),
            domain: "local".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub range_start: String,
    pub range_end: String,
    pub subnet_mask: String,
    pub gateway: String,
    pub dns_servers: Vec<String>,
    pub lease_time: u32,
}

impl Default for DhcpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            range_start: "192.168.1.100".to_string(),
            range_end: "192.168.1.200".to_string(),
            subnet_mask: "255.255.255.0".to_string(),
            gateway: "192.168.1.1".to_string(),
            dns_servers: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
            lease_time: 86400,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TftpConfig {
    pub enabled: bool,
    pub root_dir: PathBuf,
    pub port: u16,
}

impl Default for TftpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            root_dir: PathBuf::from("/var/lib/tftpboot"),
            port: 69,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IscsiConfig {
    pub enabled: bool,
    pub target_prefix: String,
    pub portal_port: u16,
    pub targets_dir: PathBuf,
}

impl Default for IscsiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            target_prefix: "iqn.2024-01.com.diskless".to_string(),
            portal_port: 3260,
            targets_dir: PathBuf::from("/var/lib/iscsi-targets"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NfsConfig {
    pub enabled: bool,
    pub exports_dir: PathBuf,
}

impl Default for NfsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            exports_dir: PathBuf::from("/srv/nfs"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SambaConfig {
    pub enabled: bool,
    pub workgroup: String,
    pub share_name: String,
    pub share_path: PathBuf,
    pub read_only: bool,
    pub guest_ok: bool,
}

impl Default for SambaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            workgroup: "WORKGROUP".to_string(),
            share_name: "diskless".to_string(),
            share_path: PathBuf::from("/srv/samba/diskless"),
            read_only: false,
            guest_ok: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub images_dir: PathBuf,
    pub snapshots_dir: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            images_dir: PathBuf::from("/var/lib/diskless/images"),
            snapshots_dir: PathBuf::from("/var/lib/diskless/snapshots"),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let settings: Settings = toml::from_str(&content)?;
            Ok(settings)
        } else {
            let settings = Self::default();
            settings.save(path)?;
            Ok(settings)
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            dhcp: DhcpConfig::default(),
            tftp: TftpConfig::default(),
            iscsi: IscsiConfig::default(),
            nfs: NfsConfig::default(),
            samba: SambaConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}
