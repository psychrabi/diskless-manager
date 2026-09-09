// New ZFS management commands: list_zpools, list_datasets, create_zfs_dataset

use crate::{
    error::AppError,
    infrastructure::command::{run_command, run_command_output_no_sudo},
};

pub fn list_block_devices() -> Result<Vec<String>, AppError> {
    let out = run_command_output_no_sudo(&["lsblk", "-d", "-n", "-o", "NAME"])
        .map_err(|e| AppError::Command(e.to_string()))?;

    let devices: Vec<String> = out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|s| s.trim().to_string())
        .collect();

    Ok(devices)
}

pub fn rename_zfs_dataset(old: &str, new: &str) -> Result<String, AppError> {
    if old.trim().is_empty() || new.trim().is_empty() {
        return Err(AppError::Validation(
            "old and new dataset names are required".into(),
        ));
    }

    run_command(&["zfs", "rename", old, new]).map_err(|e| AppError::Command(e.to_string()))?;

    Ok(format!("Renamed {} -> {}", old, new))
}
