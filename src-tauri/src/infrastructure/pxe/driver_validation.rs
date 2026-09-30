//! Validation and inspection helpers for imported Windows network drivers.
//!
//! Validation is independent of WinPE and offline Windows servicing. INF parsing is
//! centralized in windows_inf so the importer, selector and validation API all agree
//! on package identity and supported hardware.

use super::windows_inf::{inspect_inf_file, WindowsInfMetadata};
use anyhow::{bail, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub type DriverInfInspection = WindowsInfMetadata;

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
        let inspection = inspect_inf_file(&path)?;
        if !inspection.is_network_class {
            warnings.push(format!(
                "{} is not identified as a network driver",
                path.display()
            ));
        }
        if inspection.is_network_class && inspection.hardware_ids.is_empty() {
            warnings.push(format!(
                "{} is a network INF but no hardware IDs could be resolved",
                path.display()
            ));
        }
        if inspection.is_network_class && inspection.service_names.is_empty() {
            warnings.push(format!(
                "{} is a network INF but no AddService declaration could be resolved",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn accepts_utf16_network_inf() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("net.inf");
        let text = r#"[Version]
Class=Net
ClassGuid={4d36e972-e325-11ce-bfc1-08002be10318}

[Manufacturer]
M=Models,NTamd64

[Models.NTamd64]
D=Install,PCI\VEN_1234&DEV_5678

[Install.NT.Services]
AddService=sample,2,ServiceInstall
"#;
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&[0xFF, 0xFE]).unwrap();
        for word in text.encode_utf16() {
            file.write_all(&word.to_le_bytes()).unwrap();
        }

        let result = validate_package(dir.path()).unwrap();
        assert!(result.valid);
        assert_eq!(
            result.inf_files[0].hardware_ids,
            vec!["PCI\\VEN_1234&DEV_5678"]
        );
    }
}
