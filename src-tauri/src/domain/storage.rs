use crate::infrastructure::iscsi::ChapCredentials;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Describes where a client's storage comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageSource {
    /// Client owns a ZFS clone created from this snapshot.
    Snapshot(String),

    /// Client uses an existing shared ZFS volume.
    ///
    /// The client does NOT own this volume and must never destroy it.
    ExistingVolume(String),

    /// Client uses an already-created ZFS volume that is owned by
    /// the client.
    ///
    /// This is used when the ZFS clone was created before the
    /// StorageService was asked to expose it through iSCSI.
    ExistingClientVolume(String),
}

impl StorageSource {
    pub fn value(&self) -> &str {
        match self {
            Self::Snapshot(value)
            | Self::ExistingVolume(value)
            | Self::ExistingClientVolume(value) => value,
        }
    }

    /// Whether this storage resource is owned by the client.
    ///
    /// Snapshot clones are client-owned.
    /// Existing volumes are shared infrastructure and are not owned.
    pub fn owns_zfs_resource(&self) -> bool {
        matches!(self, Self::Snapshot(_) | Self::ExistingClientVolume(_))
    }
}

/// Describes the Linux/ZFS block device that backs a diskless client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageVolume {
    /// ZFS dataset or ZVOL name.
    pub dataset: String,

    /// Linux device exposed by ZFS.
    pub block_device: PathBuf,

    /// LIO/targetcli backstore name.
    pub backstore: String,

    /// iSCSI Qualified Name.
    pub target_iqn: String,

    /// iSCSI LUN number.
    pub lun: u32,
}

impl StorageVolume {
    pub fn new(
        dataset: impl Into<String>,
        block_device: impl Into<PathBuf>,
        backstore: impl Into<String>,
        target_iqn: impl Into<String>,
        lun: u32,
    ) -> Self {
        Self {
            dataset: dataset.into(),
            block_device: block_device.into(),
            backstore: backstore.into(),
            target_iqn: target_iqn.into(),
            lun,
        }
    }
}

/// Desired storage resources for a diskless client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientStorageSpec {
    /// Client identifier.
    pub client_id: String,

    /// Registered name used by iPXE to set the initiator IQN.
    #[serde(default)]
    pub client_name: String,

    /// Where the client's storage comes from.
    pub source: StorageSource,

    /// Destination ZFS dataset/ZVOL.
    pub dataset: String,

    /// targetcli/LIO boot backstore name.
    pub backstore: String,

    /// iSCSI target IQN.
    pub target_iqn: String,

    /// Boot LUN number.
    pub lun: u32,

    /// Whether shared game disks should also be exposed.
    #[serde(default)]
    pub use_game_disk: bool,

    /// Stored per-client selection of game master datasets.
    ///
    /// Callers resolve this against discovered masters via
    /// `StorageService::resolve_game_selection`; an empty selection with
    /// `use_game_disk` set means "all discovered masters".
    #[serde(default)]
    pub game_disks: Vec<String>,

    /// One-way iSCSI CHAP credentials enforced on the target portal.
    /// `None` preserves the legacy open portal (no authentication).
    #[serde(default)]
    pub chap: Option<ChapCredentials>,

    /// Read-only installer ISO exported as LUN 2 while the client installs an OS.
    #[serde(default)]
    pub installer_iso: Option<PathBuf>,
}

impl ClientStorageSpec {
    pub fn initiator_iqn(&self) -> String {
        crate::infrastructure::pxe::client_initiator_iqn(&self.client_name)
    }
    pub fn block_device(&self) -> PathBuf {
        PathBuf::from(format!("/dev/zvol/{}", self.dataset))
    }

    /// Returns true when the client owns the ZFS resource.
    pub fn owns_zfs_resource(&self) -> bool {
        self.source.owns_zfs_resource()
    }

    /// Alias used by the application storage service.
    pub fn owns_dataset(&self) -> bool {
        self.source.owns_zfs_resource()
    }
}

#[cfg(test)]
mod initiator_tests {
    use super::{ClientStorageSpec, StorageSource};

    #[test]
    fn client_name_controls_initiator_even_with_uuid_id_and_custom_target() {
        let spec = ClientStorageSpec {
            installer_iso: None,
            client_id: "ed336631-0aa1-44b5-a4eb-c9d39d5742b3".into(),
            client_name: "PC 001/West".into(),
            source: StorageSource::ExistingVolume("pool/client".into()),
            dataset: "pool/client".into(),
            backstore: "unrelated".into(),
            target_iqn: "iqn.custom:target".into(),
            lun: 0,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap: None,
        };
        assert_eq!(
            spec.initiator_iqn(),
            "iqn.2026-01.client:client.pc_001_west"
        );
    }
}

/// Result of provisioning client storage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientStorage {
    pub client_id: String,

    /// Authoritative storage source.
    pub source: StorageSource,

    pub volume: StorageVolume,

    /// Whether shared game disks were attached to this target.
    #[serde(default)]
    pub use_game_disk: bool,
}

impl ClientStorage {
    pub fn dataset(&self) -> &str {
        &self.volume.dataset
    }

    pub fn block_device(&self) -> &PathBuf {
        &self.volume.block_device
    }

    pub fn target_iqn(&self) -> &str {
        &self.volume.target_iqn
    }

    pub fn backstore(&self) -> &str {
        &self.volume.backstore
    }

    pub fn lun(&self) -> u32 {
        self.volume.lun
    }

    /// Whether the client owns the underlying ZFS resource.
    pub fn owns_dataset(&self) -> bool {
        self.source.owns_zfs_resource()
    }

    /// Compatibility alias.
    pub fn owns_zfs_resource(&self) -> bool {
        self.source.owns_zfs_resource()
    }

    /// Returns the snapshot when this is snapshot-backed storage.
    pub fn source_snapshot(&self) -> Option<&str> {
        match &self.source {
            StorageSource::Snapshot(snapshot) => Some(snapshot.as_str()),
            StorageSource::ExistingVolume(_) | StorageSource::ExistingClientVolume(_) => None,
        }
    }
}

/// Current state of client storage.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StorageState {
    Missing,
    Partial,
    Ready,
    InUse,
    Error,
}

/// Result of a storage reconciliation operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageReconcileResult {
    pub state: StorageState,
    pub zfs_present: bool,
    pub iscsi_present: bool,
    pub target_iqn: String,
    pub dataset: String,
}
