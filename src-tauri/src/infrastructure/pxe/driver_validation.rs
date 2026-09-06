//! Validation and inspection helpers for imported Windows network drivers.
//!
//! This is intentionally independent of WinPE and offline Windows servicing.
//! It validates the driver package before it is offered to either workflow.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const NETWORK_CLASS_GUID: &str = "4d36e972-e325-11ce-bfc1-08002be10318";

#[derive(Debug, Clone, Serialize)]
pub struct DriverInfInspection {
    pub path: String,
    pub is_network_class: bool,
    pub class: Option<String>,
    pub class_guid: Option<String>,
    pub provider: Option<String>,
    pub version: Option<String>,
    pub hardware_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriverPackageValidation {
    pub valid: bool,
    pub inf_files: Vec<DriverInfInspection>,
    pub warnings: Vec<String>,
}

pub fn validate_package(root: &Path) -> Result<DriverPackageValidation> {
    if !root.is_dir() {
        bail!("driver package directory does not exist: {}", root.display());
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
            warnings.push(format!("{} is not identified as a network driver", path.display()));
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

fn inspect_inf(path: &Path) -> Result<DriverInfInspection> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read INF {}", path.display()))?;

    let class = find_inf_value(&content, "Class");
    let class_guid = find_inf_value(&content, "ClassGuid");
    let provider = find_inf_value(&content, "Provider");
    let version = find_inf_value(&content, "DriverVer");
    let hardware_ids = collect_hardware_ids(&content);

    let is_network_class = class
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("Net"))
        || class_guid
            .as_deref()
            .is_some_and(|value| value.to_ascii_lowercase().contains(NETWORK_CLASS_GUID));

    Ok(DriverInfInspection {
        path: path.display().to_string(),
        is_network_class,
        class,
        class_guid,
        provider,
        version,
        hardware_ids,
    })
}

fn find_inf_value(content: &str, key: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        let (name, value) = trimmed.split_once('=')?;
        if name.trim().eq_ignore_ascii_case(key) {
            Some(value.trim().trim_matches('"').to_string())
        } else {
            None
        }
    })
}

fn collect_hardware_ids(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.to_ascii_lowercase().contains("pci\\ven_")
                || trimmed.to_ascii_lowercase().contains("usb\\")
                || trimmed.to_ascii_lowercase().contains("vmbus\\")
            {
                trimmed
                    .split_once('=')
                    .map(|(_, value)| value.trim().trim_matches('"').to_string())
            } else {
                None
            }
        })
        .filter(|value| !value.is_empty())
        .collect()
}

fn collect_inf_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_inf_files(&path, output)?;
        } else if path.extension().and_then(|v| v.to_str()).is_some_and(|v| v.eq_ignore_ascii_case("inf")) {
            output.push(path);
        }
    }
    Ok(())
}
