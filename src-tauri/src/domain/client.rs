use crate::domain::errors::DomainError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientId(String);

impl ClientId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_string(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();

        if value.trim().is_empty() {
            return Err(DomainError::InvalidClientId);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl TryFrom<String> for ClientId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_string(value)
    }
}

impl AsRef<str> for ClientId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Default for ClientId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MacAddress(String);

impl MacAddress {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let cleaned: String = value.chars().filter(|c| c.is_ascii_hexdigit()).collect();

        if cleaned.len() != 12 {
            return Err(DomainError::InvalidMacAddress(value.to_string()));
        }

        if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DomainError::InvalidMacAddress(value.to_string()));
        }

        let normalized = cleaned
            .to_ascii_lowercase()
            .as_bytes()
            .chunks(2)
            .map(|chunk| {
                // `cleaned` is ASCII hex, therefore this conversion is safe.
                std::str::from_utf8(chunk).unwrap()
            })
            .collect::<Vec<_>>()
            .join(":");

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ClientStatus {
    #[default]
    Provisioning,
    Ready,
    Online,
    Offline,
    Error,
    Disabled,
}

impl ClientStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Ready => "ready",
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Error => "error",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PxeMode {
    #[default]
    Uefi,
    Bios,
}

impl PxeMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Uefi => "uefi",
            Self::Bios => "bios",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BootMode {
    #[default]
    Normal,
    Super,
}

impl BootMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Super => "super",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Client {
    pub id: ClientId,
    pub name: String,
    pub mac: MacAddress,
    pub ip: IpAddr,
    pub master: String,
    pub enabled: bool,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    pub snapshot: Option<String>,
    pub block_store: Option<String>,
    pub target_iqn: Option<String>,
    pub writeback: Option<String>,
    pub last_modified: Option<DateTime<Utc>>,
    pub block_device: Option<String>,

    pub status: ClientStatus,
    pub mode: BootMode,
    pub pxe_mode: PxeMode,

    pub keep_writeback: bool,
    pub use_game_disk: bool,
}

impl Client {
    pub fn create(request: CreateClient) -> Result<Self, DomainError> {
        let name = request.name.trim().to_string();

        if name.is_empty() {
            return Err(DomainError::EmptyClientName);
        }

        if request.master.trim().is_empty() {
            return Err(DomainError::EmptyMasterImage);
        }

        let ip = IpAddr::from_str(request.ip.trim())
            .map_err(|_| DomainError::InvalidIpAddress(request.ip.clone()))?;

        let mac = MacAddress::parse(&request.mac)?;

        let now = Utc::now();

        Ok(Self {
            id: ClientId::new(),
            name,
            mac,
            ip,
            master: request.master,
            enabled: true,

            created_at: now,
            updated_at: now,

            snapshot: request.snapshot,
            block_store: request.block_store,
            target_iqn: request.target_iqn,
            writeback: None,
            last_modified: Some(now),
            block_device: request.block_device,

            status: ClientStatus::Provisioning,
            mode: BootMode::Normal,
            pxe_mode: request.pxe_mode,

            keep_writeback: request.keep_writeback,
            use_game_disk: request.use_game_disk,
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_online(&self) -> bool {
        self.status == ClientStatus::Online
    }

    pub fn is_ready(&self) -> bool {
        self.status == ClientStatus::Ready
    }

    pub fn is_super_mode(&self) -> bool {
        self.mode == BootMode::Super
    }

    pub fn has_snapshot(&self) -> bool {
        self.snapshot
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
    }

    pub fn mark_ready(&mut self) {
        self.status = ClientStatus::Ready;
        self.updated_at = Utc::now();
        self.last_modified = Some(self.updated_at);
    }

    pub fn mark_error(&mut self) {
        self.status = ClientStatus::Error;
        self.updated_at = Utc::now();
        self.last_modified = Some(self.updated_at);
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.status = ClientStatus::Disabled;
        self.updated_at = Utc::now();
        self.last_modified = Some(self.updated_at);
    }

    pub fn enable(&mut self) {
        self.enabled = true;

        if self.status == ClientStatus::Disabled {
            self.status = ClientStatus::Ready;
        }

        self.updated_at = Utc::now();
        self.last_modified = Some(self.updated_at);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateClient {
    pub name: String,
    pub mac: String,
    pub ip: String,
    pub master: String,

    pub snapshot: Option<String>,
    pub block_store: Option<String>,
    pub block_device: Option<String>,
    pub target_iqn: Option<String>,

    #[serde(default)]
    pub pxe_mode: PxeMode,

    #[serde(default = "default_true")]
    pub keep_writeback: bool,

    #[serde(default)]
    pub use_game_disk: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateClient {
    pub name: Option<String>,
    pub mac: Option<String>,
    pub ip: Option<String>,
    pub master: Option<String>,
    pub snapshot: Option<String>,

    pub enabled: Option<bool>,
    pub keep_writeback: Option<bool>,
    pub use_game_disk: Option<bool>,

    pub block_store: Option<String>,
    pub block_device: Option<String>,
    pub target_iqn: Option<String>,

    pub pxe_mode: Option<PxeMode>,
    pub mode: Option<BootMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootLog {
    pub id: String,
    pub client_id: ClientId,
    pub image_id: Option<String>,
    pub boot_time: DateTime<Utc>,
    pub success: bool,
    pub duration_ms: Option<i64>,
    pub message: Option<String>,
}
