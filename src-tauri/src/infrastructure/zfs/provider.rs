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
