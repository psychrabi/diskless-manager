use anyhow::Result;

#[derive(Debug, Clone)]
pub struct ZfsDatasetInfo {
    pub name: String,
    pub dataset_type: String,
    pub used: Option<String>,
    pub available: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ZfsSnapshotInfo {
    pub name: String,
    pub dataset: String,
    pub snapshot: String,
    pub used: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ZfsVolumeInfo {
    pub name: String,
    pub volsize: Option<String>,
    pub volblocksize: Option<String>,
    pub used: Option<String>,
}

/// Application-facing ZFS capability boundary.
pub trait ZfsProvider: Send + Sync {
    fn exists(&self, name: &str) -> Result<bool>;

    fn dataset(&self, name: &str) -> Result<Option<ZfsDatasetInfo>>;

    fn list_datasets(&self, root: &str) -> Result<Vec<ZfsDatasetInfo>>;

    fn list_snapshots(&self, root: &str) -> Result<Vec<ZfsSnapshotInfo>>;

    fn create_dataset(&self, dataset: &str, properties: &[(&str, &str)]) -> Result<()>;

    fn create_volume(&self, volume: &str, size: &str, properties: &[(&str, &str)]) -> Result<()>;

    fn create_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()>;

    fn clone_snapshot(&self, snapshot: &str, destination: &str) -> Result<()>;

    fn destroy(&self, name: &str) -> Result<()>;

    fn get_property(&self, property: &str, dataset: &str) -> Result<Option<String>>;

    fn set_property(&self, property: &str, value: &str, dataset: &str) -> Result<()>;
}
