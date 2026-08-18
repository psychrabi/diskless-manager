use log::{info, warn};

use crate::cmd::run_command;
use crate::error::AppError;
use crate::infrastructure::iscsi::{IscsiProvisioner, IscsiTargetSpec, TargetCliProvisioner};

/// Compatibility wrapper around the new infrastructure iSCSI provisioner.
///
/// This function is intentionally kept temporarily because older application
/// code still calls `crate::iscsi::setup_iscsi_target()`.
///
/// New code should use:
///
/// ```text
/// StorageService
///     -> IscsiProvisioner
///         -> TargetCliProvisioner
/// ```
pub fn setup_iscsi_target(
    target_iqn: &str,
    block_store: &str,
    volume_path: &str,
) -> Result<(), AppError> {
    let spec = IscsiTargetSpec::new(target_iqn, block_store, volume_path, 0);

    TargetCliProvisioner::new()
        .create_target(&spec)
        .map_err(|error| {
            AppError::Command(format!(
                "Failed to setup iSCSI target '{}': {}",
                target_iqn, error
            ))
        })
}

/// Compatibility wrapper for the old cleanup API.
///
/// The target itself is removed through the new infrastructure layer.
/// The explicitly-owned boot backstore is then removed separately.
///
/// Game backstores are deliberately NOT removed here because they can be
/// shared by multiple clients.
pub fn cleanup_iscsi_target(target_iqn: &str, block_store: &str) -> Result<(), AppError> {
    let provisioner = TargetCliProvisioner::new();

    info!(
        "Cleaning up iSCSI target '{}' and boot backstore '{}'",
        target_iqn, block_store
    );

    provisioner.remove_target(target_iqn).map_err(|error| {
        AppError::Command(format!(
            "Failed to remove iSCSI target '{}': {}",
            target_iqn, error
        ))
    })?;

    // The new provisioner intentionally does not delete backstores when
    // removing a target because backstores may be shared infrastructure.
    //
    // The client boot backstore, however, is owned by the client and is
    // safe to remove here.
    if !block_store.trim().is_empty() {
        let backstore_path = format!("/backstores/block/{}", block_store);

        match run_command(["targetcli", &backstore_path, "delete"]) {
            Ok(_) => {
                info!("Deleted iSCSI boot backstore '{}'", block_store);
            }

            Err(error) => {
                // Cleanup should remain best-effort, matching the behavior
                // of the previous implementation.
                warn!(
                    "Could not delete iSCSI backstore '{}': {}",
                    block_store, error
                );
            }
        }
    }

    let _ = run_command(["targetcli", "saveconfig"]);

    Ok(())
}
