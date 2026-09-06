//! Stable manifest generation for the PXE network-driver repository.
//!
//! The manifest is the machine-readable contract consumed by the PXE layer and
//! future UI/client matching code. It intentionally contains metadata and
//! relative paths only; no host-specific registry exports are applied here.

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
}

impl DriverManifest {
    pub fn new(entries: Vec<DriverManifestEntry>) -> Self {
        Self {
            schema_version: 1,
            generated_at: Utc::now(),
            drivers: entries,
        }
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).context("failed to serialize driver manifest")?;
        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("failed to write driver manifest {}", path.display()))?;
        Ok(())
    }
}
