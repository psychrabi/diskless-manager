//! Stable manifest generation for the PXE network-driver repository.
//!
//! The manifest is the machine-readable contract consumed by the PXE layer and
//! UI/client matching code. It contains package metadata and relative paths only;
//! no host-specific registry exports are applied here.

use super::NetworkDriverPackage;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverManifest {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub drivers: Vec<DriverManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverManifestEntry {
    pub id: String,
    pub name: String,
    pub service_name: Option<String>,
    pub driver_name: Option<String>,
    pub pnp_device_id: Option<String>,
    pub guid: Option<String>,
    pub mac_address: Option<String>,
    pub inf_files: Vec<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub architectures: Vec<String>,
    #[serde(default)]
    pub hardware_ids: Vec<String>,
    #[serde(default)]
    pub compatible_ids: Vec<String>,
    #[serde(default)]
    pub service_names: Vec<String>,
    #[serde(default)]
    pub catalog_files: Vec<String>,
}

impl From<&NetworkDriverPackage> for DriverManifestEntry {
    fn from(package: &NetworkDriverPackage) -> Self {
        Self {
            id: package.id.clone(),
            name: package.name.clone(),
            service_name: package.service_name.clone(),
            driver_name: package.driver_name.clone(),
            pnp_device_id: package.pnp_device_id.clone(),
            guid: package.guid.clone(),
            mac_address: package.mac_address.clone(),
            inf_files: package.inf_files.clone(),
            provider: package.provider.clone(),
            version: package.version.clone(),
            architectures: package.architectures.clone(),
            hardware_ids: package.hardware_ids.clone(),
            compatible_ids: package.compatible_ids.clone(),
            service_names: package.service_names.clone(),
            catalog_files: package.catalog_files.clone(),
        }
    }
}

impl DriverManifest {
    pub fn from_packages(packages: &[NetworkDriverPackage]) -> Self {
        Self::new(packages.iter().map(DriverManifestEntry::from).collect())
    }

    pub fn new(entries: Vec<DriverManifestEntry>) -> Self {
        Self {
            schema_version: 2,
            generated_at: Utc::now(),
            drivers: entries,
        }
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json =
            serde_json::to_string_pretty(self).context("failed to serialize driver manifest")?;
        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("failed to write driver manifest {}", path.display()))?;
        Ok(())
    }
}
