use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Specification for one LUN exposed through an iSCSI target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IscsiLunSpec {
    /// LUN number presented to the iSCSI initiator.
    pub lun: u32,

    /// targetcli/LIO block backstore name.
    pub backstore: String,

    /// Linux block device exposed through the LIO backstore.
    pub block_device: PathBuf,

    /// Whether this LUN should be exposed read-only.
    #[serde(default)]
    pub readonly: bool,
}

impl IscsiLunSpec {
    pub fn new(lun: u32, backstore: impl Into<String>, block_device: impl Into<PathBuf>) -> Self {
        Self {
            lun,
            backstore: backstore.into(),
            block_device: block_device.into(),
            readonly: false,
        }
    }

    pub fn readonly(
        lun: u32,
        backstore: impl Into<String>,
        block_device: impl Into<PathBuf>,
    ) -> Self {
        Self {
            lun,
            backstore: backstore.into(),
            block_device: block_device.into(),
            readonly: true,
        }
    }
}

/// Desired state for one iSCSI target.
///
/// A target may expose multiple LUNs:
///
/// ```text
/// iSCSI target
/// ├── LUN 0 -> boot disk
/// ├── LUN 1 -> game disk
/// ├── LUN 2 -> game disk
/// └── ...
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IscsiTargetSpec {
    /// iSCSI Qualified Name.
    pub target_iqn: String,

    /// LUNs exposed by this target.
    pub luns: Vec<IscsiLunSpec>,

    /// iSCSI portal address.
    pub portal_address: String,

    /// iSCSI TCP port.
    pub portal_port: u16,
}

impl IscsiTargetSpec {
    /// Construct a target containing one LUN.
    pub fn new(
        target_iqn: impl Into<String>,
        backstore: impl Into<String>,
        block_device: impl Into<PathBuf>,
        lun: u32,
    ) -> Self {
        Self {
            target_iqn: target_iqn.into(),
            luns: vec![IscsiLunSpec::new(lun, backstore, block_device)],
            portal_address: "0.0.0.0".to_string(),
            portal_port: 3260,
        }
    }

    /// Construct a target containing multiple LUNs.
    pub fn with_luns(
        target_iqn: impl Into<String>,
        luns: Vec<IscsiLunSpec>,
    ) -> anyhow::Result<Self> {
        if luns.is_empty() {
            anyhow::bail!("iSCSI target must contain at least one LUN");
        }

        Ok(Self {
            target_iqn: target_iqn.into(),
            luns,
            portal_address: "0.0.0.0".to_string(),
            portal_port: 3260,
        })
    }

    /// Find a requested LUN.
    pub fn lun(&self, lun_number: u32) -> Option<&IscsiLunSpec> {
        self.luns.iter().find(|lun| lun.lun == lun_number)
    }
}

/// Current state of one LUN.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IscsiLunState {
    /// LUN number.
    pub lun: u32,

    /// Backstore name.
    pub backstore: String,

    /// Whether the LUN exists.
    pub exists: bool,

    /// Whether the backstore exists.
    pub backstore_exists: bool,

    /// Whether the backstore points at the desired block device.
    pub block_device_matches: bool,
}

impl IscsiLunState {
    pub fn is_ready(&self) -> bool {
        self.exists && self.backstore_exists && self.block_device_matches
    }
}

/// Current state of an iSCSI target.
///
/// The aggregate fields are retained for compatibility with the
/// application layer. For multi-LUN targets, `luns` contains the
/// authoritative per-LUN state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IscsiTargetState {
    /// iSCSI target IQN.
    pub target_iqn: String,

    /// Whether the target itself exists.
    pub exists: bool,

    /// Aggregate state: all requested backstores exist.
    pub backstore_exists: bool,

    /// Aggregate state: all requested LUNs exist.
    pub lun_exists: bool,

    /// Per-LUN state.
    pub luns: Vec<IscsiLunState>,

    /// Whether the required portal exists.
    pub portal_exists: bool,
}

impl IscsiTargetState {
    /// Returns true when the complete requested target is ready.
    pub fn is_ready(&self) -> bool {
        self.exists
            && self.backstore_exists
            && self.lun_exists
            && self.portal_exists
            && self.luns.iter().all(IscsiLunState::is_ready)
    }

    /// Calculate aggregate state from individual LUN states.
    pub fn from_luns(
        target_iqn: String,
        exists: bool,
        luns: Vec<IscsiLunState>,
        portal_exists: bool,
    ) -> Self {
        let backstore_exists = !luns.is_empty() && luns.iter().all(|lun| lun.backstore_exists);

        let lun_exists = !luns.is_empty() && luns.iter().all(|lun| lun.exists);

        Self {
            target_iqn,
            exists,
            backstore_exists,
            lun_exists,
            luns,
            portal_exists,
        }
    }
}

/// Resources created by one iSCSI provisioning transaction.
///
/// This is intentionally different from `IscsiTargetState`.
///
/// `IscsiTargetState` answers:
///
/// ```text
/// What exists right now?
/// ```
///
/// `IscsiProvisionResult` answers:
///
/// ```text
/// What did THIS transaction create?
/// ```
///
/// The distinction is required for safe rollback.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IscsiProvisionResult {
    /// Whether the target itself was created by this transaction.
    pub target_created: bool,

    /// Backstores created by this transaction.
    ///
    /// Existing/shared backstores must not appear here.
    pub backstores_created: Vec<String>,

    /// LUN numbers created by this transaction.
    pub luns_created: Vec<u32>,

    /// Whether the portal was created by this transaction.
    pub portal_created: bool,
}

impl IscsiProvisionResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn owns_backstore(&self, backstore: &str) -> bool {
        self.backstores_created.iter().any(|item| item == backstore)
    }

    pub fn owns_lun(&self, lun: u32) -> bool {
        self.luns_created.contains(&lun)
    }

    pub fn is_empty(&self) -> bool {
        !self.target_created
            && self.backstores_created.is_empty()
            && self.luns_created.is_empty()
            && !self.portal_created
    }
}
