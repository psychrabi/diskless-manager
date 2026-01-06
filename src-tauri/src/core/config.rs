use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub dhcp: DhcpConfig,
    #[serde(default)]
    pub tftp: TftpConfig,
    #[serde(default)]
    pub iscsi: IscsiConfig,
    #[serde(default)]
    pub nfs: NfsConfig,
    #[serde(default)]
    pub http: HttpConfig,
    #[serde(default)]
    pub samba: SambaConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(deserialize_with = "deserialize_interface")]
    pub interface: Vec<String>,
    pub ip_address: String,
    #[serde(default)]
    pub netmask: String,
    #[serde(default)]
    pub gateway: String,
    #[serde(default)]
    pub dns: Vec<String>,
    pub hostname: String,
    pub domain: String,
}

fn deserialize_interface<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct InterfaceVisitor;

    impl<'de> serde::de::Visitor<'de> for InterfaceVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or a sequence of strings")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![v.to_string()])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element()? {
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(InterfaceVisitor)
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            interface: vec!["eth0".to_string()],
            ip_address: "192.168.1.1".to_string(),
            netmask: "255.255.255.0".to_string(),
            gateway: "192.168.1.1".to_string(),
            dns: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
            hostname: "pxeserver".to_string(),
            domain: "local".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub subnet_ip: String,
    pub start_ip: String,
    pub end_ip: String,
    pub subnet_mask: String,
    pub gateway_ip: String,
    pub dns_server1: String,
    pub dns_server2: String,
    pub broadcast_ip: String,
    pub next_server_ip: String,
    pub boot_server_ip: String,
    pub boot_script: String,
    pub boot_file_legacy: String,
    pub boot_file_uefi32: String,
    pub boot_file_uefi64: String,
}

impl Default for DhcpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            subnet_ip: "192.168.1.0".to_string(),
            start_ip: "192.168.1.100".to_string(),
            end_ip: "192.168.1.200".to_string(),
            subnet_mask: "255.255.255.0".to_string(),
            gateway_ip: "192.168.1.1".to_string(),
            dns_server1: "1.1.1.1".to_string(),
            dns_server2: "1.0.0.1".to_string(),
            broadcast_ip: "192.168.1.255".to_string(),
            next_server_ip: "192.168.1.1".to_string(),
            boot_server_ip: "192.168.1.1".to_string(),
            boot_script: "autoexec.ipxe".to_string(),
            boot_file_legacy: "undionly.kpxe".to_string(),
            boot_file_uefi32: "ipxe.efi".to_string(),
            boot_file_uefi64: "ipxe.efi".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]

pub struct HttpConfig {
    pub enabled: bool,
    pub root_dir: String,
    pub server_ip: String,
    pub port: u16,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            root_dir: PathBuf::from("/srv/tftp").display().to_string(),
            server_ip: "192.168.1.1".to_string(),
            port: 80,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TftpConfig {
    pub enabled: bool,
    pub root_dir: String,
    pub server_ip: String,
    pub port: u16,
    pub options: String,
}

impl Default for TftpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            root_dir: PathBuf::from("/srv/tftp").display().to_string(),
            server_ip: "192.168.1.1".to_string(),
            port: 69,
            options: "--secure --verbose".to_string(),
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
            share_path: PathBuf::from("/srv/shared"),
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
