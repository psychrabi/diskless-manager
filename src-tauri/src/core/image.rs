use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImageKind {
    #[default]
    Master,
    Snapshot,
    Clone,
}

impl std::fmt::Display for ImageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Master => write!(f, "master"),
            Self::Snapshot => write!(f, "snapshot"),
            Self::Clone => write!(f, "clone"),
        }
    }
}

impl std::str::FromStr for ImageKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "master" => Ok(Self::Master),
            "snapshot" => Ok(Self::Snapshot),
            "clone" => Ok(Self::Clone),
            _ => Err(anyhow::anyhow!("invalid image kind: {}", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OsType {
    Linux,
    Windows,
}

impl std::fmt::Display for OsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Linux => write!(f, "linux"),
            Self::Windows => write!(f, "windows"),
        }
    }
}

impl std::str::FromStr for OsType {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "linux" => Ok(Self::Linux),
            "windows" => Ok(Self::Windows),
            _ => Err(anyhow::anyhow!("invalid OS type: {}", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Raw,
    Qcow2,
    Vmdk,
    Vdi,
    None,
}

impl std::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raw => write!(f, "raw"),
            Self::Qcow2 => write!(f, "qcow2"),
            Self::Vmdk => write!(f, "vmdk"),
            Self::Vdi => write!(f, "vdi"),
            Self::None => write!(f, "none"),
        }
    }
}

impl std::str::FromStr for ImageFormat {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "raw" | "img" => Ok(Self::Raw),
            "qcow2" => Ok(Self::Qcow2),
            "vmdk" => Ok(Self::Vmdk),
            "vdi" => Ok(Self::Vdi),
            "none" => Ok(Self::None),
            _ => Err(anyhow::anyhow!("invalid image format: {}", value)),
        }
    }
}

impl ImageFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Raw => "img",
            Self::Qcow2 => "qcow2",
            Self::Vmdk => "vmdk",
            Self::Vdi => "vdi",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub id: String,
    pub name: String,
    pub kind: ImageKind,
    pub os_type: OsType,
    pub size_gb: u64,
    pub path: PathBuf,
    pub format: ImageFormat,
    pub status: String,
    pub description: Option<String>,

    /// For snapshots:
    ///     parent_id = image/clone the snapshot belongs to.
    ///
    /// For clones:
    ///     parent_id = source master/clone image.
    ///
    /// For masters:
    ///     parent_id = None.
    pub parent_id: Option<String>,

    /// For clones, the snapshot from which the clone was created.
    ///
    /// Example:
    ///     "v1"
    ///
    /// For snapshots and masters this is None.
    pub source_snapshot: Option<String>,

    pub checksum: Option<String>,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateImageRequest {
    pub name: String,
    pub os_type: String,
    pub size_gb: u64,
    pub format: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportImageRequest {
    pub name: String,
    pub source_path: String,
    pub os_type: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateImageRequest {
    pub name: Option<String>,
    pub os_type: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DriverRevisionStatus {
    Preparing,
    AwaitingTest,
    Ready,
    Failed,
}

impl std::fmt::Display for DriverRevisionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preparing => write!(f, "preparing"),
            Self::AwaitingTest => write!(f, "awaiting-test"),
            Self::Ready => write!(f, "ready"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for DriverRevisionStatus {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "preparing" => Ok(Self::Preparing),
            "awaiting-test" => Ok(Self::AwaitingTest),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            _ => Err(anyhow::anyhow!("invalid driver revision status: {}", value)),
        }
    }
}

/// Immutable Windows image revision carrying the compatible signed driver set.
///
/// A source image is never rewritten: each revision records the source
/// image/snapshot it was built from, the catalog package IDs injected, a
/// deterministic hash of that driver set, and a lifecycle status. Clients keep
/// their current assignment until a revision reaches `ready`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageDriverRevision {
    pub id: String,
    pub source_image_id: String,
    pub source_snapshot: String,
    pub revision_image_id: Option<String>,
    pub driver_package_ids: Vec<String>,
    pub driver_set_hash: String,
    pub status: DriverRevisionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Boot-test record for one candidate image revision on its designated
/// test client.
///
/// A test passes only when the client component reports the exact candidate
/// revision and Boot NIC after Windows starts and the server confirms the
/// client's iSCSI session is active. A passing test makes the revision
/// `ready` for new client assignments; failed tests keep the prior revision
/// current and never change the revision itself.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BootTestStatus {
    Candidate,
    Passed,
    Failed,
}

impl std::fmt::Display for BootTestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Candidate => write!(f, "candidate"),
            Self::Passed => write!(f, "passed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for BootTestStatus {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "candidate" => Ok(Self::Candidate),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            _ => Err(anyhow::anyhow!("invalid boot test status: {}", value)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionBootTest {
    pub id: String,
    pub revision_id: String,
    pub test_client_id: String,
    pub expected_nic_id: String,
    pub candidate_image_id: Option<String>,
    pub candidate_snapshot: String,
    pub status: BootTestStatus,
    pub reported_revision: Option<String>,
    pub reported_nic_id: Option<String>,
    pub result_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ImageDriverRevision {
    /// Deterministic hash of a driver set used for idempotent retries.
    ///
    /// Stable across processes (FNV-1a 64-bit over the sorted, deduplicated,
    /// comma-joined package IDs); the same set always yields the same hash.
    pub fn hash_driver_set(package_ids: &[String]) -> String {
        use std::collections::BTreeSet;
        let ordered: BTreeSet<&str> = package_ids.iter().map(String::as_str).collect();
        let joined = ordered.into_iter().collect::<Vec<_>>().join(",");
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in joined.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }

    /// Allowed lifecycle transitions: preparing -> awaiting-test/failed,
    /// awaiting-test -> ready/failed. Terminal states never reopen.
    pub fn can_transition(&self, next: DriverRevisionStatus) -> bool {
        matches!(
            (&self.status, next),
            (
                DriverRevisionStatus::Preparing,
                DriverRevisionStatus::AwaitingTest | DriverRevisionStatus::Failed
            ) | (
                DriverRevisionStatus::AwaitingTest,
                DriverRevisionStatus::Ready | DriverRevisionStatus::Failed
            )
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub virtual_size: u64,
    pub actual_size: u64,
    pub format: String,
    pub backing_file: Option<String>,
    pub snapshots: Vec<String>,
}
