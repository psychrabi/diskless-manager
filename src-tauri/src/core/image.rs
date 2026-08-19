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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub virtual_size: u64,
    pub actual_size: u64,
    pub format: String,
    pub backing_file: Option<String>,
    pub snapshots: Vec<String>,
}
