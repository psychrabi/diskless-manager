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
                std::str::from_utf8(chunk).expect("MAC normalization contains only ASCII hex")
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

#[derive(Clone, Serialize, Deserialize)]
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

    /// Stored per-client game master selection. Populated from the
    /// `client_game_disks` table by the service layer, not the row map.
    #[serde(default)]
    pub game_disks: Vec<String>,

    /// One-way iSCSI CHAP username (server-generated, stable).
    #[serde(default, skip_serializing)]
    pub chap_user: Option<String>,
    /// One-way iSCSI CHAP secret (server-generated; rotated, never input).
    #[serde(default, skip_serializing)]
    pub chap_secret: Option<String>,
    /// Whether the target enforces CHAP. Existing rows default off.
    #[serde(default)]
    pub chap_enabled: bool,

    /// Catalog installer selected while this client is in OS installation mode.
    #[serde(default)]
    pub installer_media_id: Option<String>,
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
            enabled: request.enabled.unwrap_or(true),

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
            game_disks: request.game_disks,
            // Credentials are generated after provisioning, never input.
            chap_user: None,
            chap_secret: None,
            chap_enabled: request.chap_enabled,
            installer_media_id: request.installer_media_id,
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

    /// The stored super-mode flag. Prefer [`Self::is_super_client`]: the
    /// assignment, not the flag, decides whether a client is the super client.
    pub fn is_super_mode(&self) -> bool {
        self.mode == BootMode::Super
    }

    pub fn has_snapshot(&self) -> bool {
        self.snapshot
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
    }

    /// Whether this client's storage points at an image directly, with no
    /// clone of a snapshot. Such a client writes into the image itself.
    pub fn is_direct_image_assignment(&self) -> bool {
        let master = self.master.trim();
        !master.is_empty() && !master.eq_ignore_ascii_case("pending") && !self.has_snapshot()
    }

    /// Whether this client is the super client: the machine that boots an
    /// image directly so driver installation and adaptation can be captured
    /// for every other client.
    ///
    /// The assignment decides, not the stored flag, so a legacy or imported
    /// row that never set `mode = super` is still recognized while its storage
    /// points at an image directly. A cloned snapshot is never super.
    pub fn is_super_client(&self) -> bool {
        self.is_super_mode() || self.is_direct_image_assignment()
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

    #[serde(default)]
    pub enabled: Option<bool>,

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

    /// Explicit per-client game master selection.
    #[serde(default)]
    pub game_disks: Vec<String>,

    /// Enforce one-way iSCSI CHAP for this client. Credentials are
    /// server-generated at provision time, never supplied here.
    #[serde(default = "default_true")]
    pub chap_enabled: bool,

    /// Keep this client in installer mode and attach the selected ISO at LUN 1.
    #[serde(default)]
    pub installer_media_id: Option<String>,
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

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("chap_enabled", &self.chap_enabled)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod super_client_tests {
    use super::*;

    fn request(master: &str, snapshot: Option<&str>) -> CreateClient {
        CreateClient {
            installer_media_id: None,
            enabled: None,
            name: "PC001".to_string(),
            mac: "02:00:00:00:00:01".to_string(),
            ip: "192.0.2.1".to_string(),
            master: master.to_string(),
            snapshot: snapshot.map(str::to_string),
            block_store: None,
            block_device: None,
            target_iqn: None,
            pxe_mode: PxeMode::Uefi,
            keep_writeback: true,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap_enabled: true,
        }
    }

    #[test]
    fn direct_image_assignment_is_the_super_client_without_the_stored_flag() {
        let client = Client::create(request("tank/image/windows", None)).unwrap();
        assert!(client.is_direct_image_assignment());
        assert!(client.is_super_client());
        assert!(!client.is_super_mode());
    }

    #[test]
    fn a_snapshot_clone_is_never_the_super_client() {
        let client = Client::create(request(
            "tank/image/windows",
            Some("tank/image/windows@clean"),
        ))
        .unwrap();
        assert!(!client.is_direct_image_assignment());
        assert!(!client.is_super_client());
        // An empty snapshot string is an absent assignment, not a clone.
        let blank = Client::create(request("tank/image/windows", Some("  "))).unwrap();
        assert!(blank.is_super_client());
    }

    #[test]
    fn an_unprovisioned_client_is_not_a_super_client() {
        for master in ["pending", "  PENDING  "] {
            let client = Client::create(request(master, None)).unwrap();
            assert!(!client.is_super_client(), "master {master:?}");
        }
        // A row read back without an image is unprovisioned, not super.
        let mut client = Client::create(request("tank/image/windows", None)).unwrap();
        client.master = String::new();
        assert!(!client.is_super_client());
        assert!(!client.is_direct_image_assignment());
    }

    #[test]
    fn the_stored_flag_alone_still_marks_the_super_client() {
        let mut client = Client::create(request(
            "tank/image/windows",
            Some("tank/image/windows@clean"),
        ))
        .unwrap();
        client.mode = BootMode::Super;
        assert!(client.is_super_client());
    }
}
