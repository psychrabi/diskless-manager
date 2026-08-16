use anyhow::Result;
use std::path::Path;

use crate::core::image::{ImageFormat, ImageInfo};

#[derive(Debug, Clone)]
pub struct ImageBackendInfo {
    pub virtual_size: u64,
    pub actual_size: u64,
    pub format: ImageFormat,
    pub backing_file: Option<String>,
    pub snapshots: Vec<String>,
}

impl From<ImageBackendInfo> for ImageInfo {
    fn from(info: ImageBackendInfo) -> Self {
        Self {
            virtual_size: info.virtual_size,

            actual_size: info.actual_size,

            format: info.format.to_string(),

            backing_file: info.backing_file,

            snapshots: info.snapshots,
        }
    }
}

pub trait ImageBackend: Send + Sync {
    fn exists(&self, name: &str) -> Result<bool>;

    fn create_volume(&self, name: &str, size_gb: u64) -> Result<()>;

    fn destroy(&self, name: &str) -> Result<()>;

    fn rename(&self, old_name: &str, new_name: &str) -> Result<()>;

    fn clone_image(&self, source: &str, destination: &str) -> Result<()>;

    fn create_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()>;

    fn destroy_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()>;

    fn rollback_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()>;

    fn resize(&self, name: &str, size_gb: u64) -> Result<()>;

    fn import_raw(&self, source: &Path, destination: &str, size_bytes: u64) -> Result<()>;

    fn verify(&self, name: &str) -> Result<bool>;

    fn info(&self, name: &str) -> Result<ImageBackendInfo>;

    fn set_os_type(&self, name: &str, os_type: &str) -> Result<()>;

    fn image_parent(&self) -> Result<String>;
}
