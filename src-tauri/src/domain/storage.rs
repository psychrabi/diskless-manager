use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Describes where a client's storage comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageSource {
    /// Client owns a ZFS clone created from this snapshot.
    Snapshot(String),

    /// Client uses an existing ZFS volume, normally a master image.
    ///
    /// The client does NOT own this volume and must never destroy it.
    ExistingVolume(String),
}

impl StorageSource {
    pub fn value(&self) -> &str {
        match self {
            Self::Snapshot(value) => value,
            Self::ExistingVolume(value) => value,
        }
    }

    /// Whether this storage resource is owned by the client.
    ///
    /// Snapshot clones are client-owned.
    /// Existing volumes are shared infrastructure and are not owned.
    pub fn owns_zfs_resource(&self) -> bool {
        matches!(self, Self::Snapshot(_))
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
}

impl ClientStorageSpec {
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
            StorageSource::ExistingVolume(_) => None,
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
