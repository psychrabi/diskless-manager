//! Validation and inspection helpers for imported Windows network drivers.
//!
//! This is intentionally independent of WinPE and offline Windows servicing.
//! It validates the driver package before it is offered to either workflow.

use super::windows_inf::parse_inf_file;
use anyhow::{bail, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct DriverInfInspection {
    pub path: String,
    pub is_network_class: bool,
    pub class: Option<String>,
    pub class_guid: Option<String>,
    pub provider: Option<String>,
    pub driver_date: Option<String>,
    pub version: Option<String>,
    pub catalog_files: Vec<String>,
    pub architectures: Vec<String>,
    pub hardware_ids: Vec<String>,
    pub compatible_ids: Vec<String>,
    pub service_names: Vec<String>,
    pub service_binaries: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriverPackageValidation {
    pub valid: bool,
    pub inf_files: Vec<DriverInfInspection>,
    pub warnings: Vec<String>,
}

pub fn validate_package(root: &Path) -> Result<DriverPackageValidation> {
    if !root.is_dir() {
        bail!(
            "driver package directory does not exist: {}",
            root.display()
        );
    }

    let mut inf_files = Vec::new();
    collect_inf_files(root, &mut inf_files)?;
    inf_files.sort();

    if inf_files.is_empty() {
        bail!("driver package contains no INF files");
    }

    let mut inspections = Vec::with_capacity(inf_files.len());
    let mut warnings = Vec::new();

    for path in inf_files {
        let inspection = inspect_inf(&path)?;
        if !inspection.is_network_class {
            warnings.push(format!(
                "{} is not identified as a network driver",
                path.display()
            ));
        }
        if inspection.hardware_ids.is_empty() {
            warnings.push(format!(
                "{} contains no discoverable PnP hardware IDs",
                path.display()
            ));
        }
        inspections.push(inspection);
    }

    let valid = inspections.iter().any(|item| item.is_network_class);
    Ok(DriverPackageValidation {
        valid,
        inf_files: inspections,
        warnings,
    })
}

pub fn inspect_inf(path: &Path) -> Result<DriverInfInspection> {
    let metadata = parse_inf_file(path)?;

    Ok(DriverInfInspection {
        path: path.display().to_string(),
        is_network_class: metadata.is_network_class,
        class: metadata.class,
        class_guid: metadata.class_guid,
        provider: metadata.provider,
        driver_date: metadata.driver_date,
        version: metadata.driver_version,
        catalog_files: metadata.catalog_files,
        architectures: metadata.architectures,
        hardware_ids: metadata.hardware_ids,
        compatible_ids: metadata.compatible_ids,
        service_names: metadata.service_names,
        service_binaries: metadata.service_binaries,
    })
}

fn collect_inf_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_inf_files(&path, output)?;
        } else if path
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.eq_ignore_ascii_case("inf"))
        {
            output.push(path);
        }
    }
    Ok(())
}
