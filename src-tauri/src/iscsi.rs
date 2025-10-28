use crate::utils::{run_command, run_command_output, run_command_output_no_sudo};

pub fn setup_iscsi_target(
    target_iqn: &str,
    block_store: &str,
    volume_path: &str,
) -> Result<(), String> {
    // Check and create iSCSI target if it doesn't exist
    if !target_exists(target_iqn)? {
        run_command(&["targetcli", "iscsi/", "create", target_iqn])?;
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1", target_iqn),
            "set",
            "attribute",
            "generate_node_acls=1",
            "cache_dynamic_acls=1",
            "demo_mode_write_protect=0",
            "authentication=0",
        ])?;
    }

    // Ensure block backstore (delete if exists, then create)
    if backstore_exists(block_store)? {
        run_command(&["targetcli", "backstores/block/", "delete", block_store])?;
    }
    
    // Create block backstore and capture detailed error if it fails
    match run_command_output(&[
        "targetcli",
        "backstores/block",
        "create",
        block_store,
        volume_path,
    ]) {
        Ok(_) => (),
        Err(e) => {
            // Convert /dev/zvol path to ZFS dataset path
            let dataset_path = if let Some(dataset) = volume_path.strip_prefix("/dev/zvol/") {
                dataset.to_string()
            } else {
                return Err(format!("Invalid volume path format: {}", volume_path));
            };

            // Check if ZFS dataset exists and is accessible
            if let Err(ve) = run_command_output_no_sudo(&["zfs", "list", &dataset_path]) {
                return Err(format!(
                    "ZFS volume '{}' not found or inaccessible: {}. \
                    Make sure the ZFS dataset exists and has correct permissions.", 
                    dataset_path, ve
                ));
            }

            // Check if the device file exists
            if !std::path::Path::new(volume_path).exists() {
                return Err(format!(
                    "ZFS volume exists but device file '{}' is not available. \
                    Try running 'zfs set volmode=dev {}' to create the device file.", 
                    volume_path, dataset_path
                ));
            }

            // Check if there's an existing block device with the same name
            if let Ok(output) = run_command_output(&["targetcli", "backstores/block", "ls"]) {
                if output.contains(block_store) {
                    return Err(format!(
                        "Block device '{}' already exists in targetcli. \
                        Try using a different name or delete the existing one first.", 
                        block_store
                    ));
                }
            }

            // Check if any process is using the device
            if let Ok(output) = run_command_output(&["lsof", volume_path]) {
                return Err(format!(
                    "Volume '{}' is in use by other processes:\n{}", 
                    volume_path, output
                ));
            }

            // Get current targetcli configuration for debugging
            let targetcli_state = run_command_output(&["targetcli", "ls"])
                .unwrap_or_else(|_| "Failed to get targetcli state".to_string());

            // If all checks pass but targetcli still fails, return detailed error info
            return Err(format!(
                "Failed to create iSCSI block backstore:\n\
                - Command failed: targetcli backstores/block create {} {}\n\
                - Error: {}\n\
                - Current targetcli state:\n{}\n\
                Try manually running 'targetcli backstores/block create {} {}' \
                for more detailed error output.",
                block_store, volume_path, e, targetcli_state, block_store, volume_path
            ));
        }
    };

    // Create LUN if it doesn't exist
    let lun_path = format!("/backstores/block/{}", block_store);
    if !lun_exists(target_iqn, &lun_path)? {
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1/luns", target_iqn),
            "create",
            &lun_path,
        ])?;
    }

    // Ensure portal exists (bind to 0.0.0.0:3260)
    if !portal_exists(target_iqn)? {
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1/portals/", target_iqn),
            "create",
            "0.0.0.0",
            "3260",
        ])?;
    }

    run_command(&["targetcli", "saveconfig"])?;

    Ok(())
}

pub fn cleanup_iscsi_target(target_iqn: &str, block_store: &str) -> Result<(), String> {
    println!(
        "Cleaning up iSCSI target {} and backstore {}",
        target_iqn, block_store
    );

    // Delete iSCSI target (handles LUNs/portals)
    if let Err(e) = run_command(&["targetcli", "iscsi/", "delete", target_iqn]) {
        println!(
            "Warning: Could not delete target {}: {}",
            target_iqn, e
        );
    } else {
        println!("Deleted iSCSI target {}", target_iqn);
    }

    // Delete backstore if it exists
    if !block_store.is_empty() && backstore_exists(block_store)? {
        if let Err(e) = run_command(&["targetcli", "backstores/block/", "delete", block_store]) {
            println!(
                "Warning: Could not delete block backstore {}: {}",
                block_store, e
            );
        } else {
            println!("Deleted block backstore {}", block_store);
        }
    }

    // Save configuration
    let _ = run_command(&["targetcli", "saveconfig"]);

    Ok(())
}

// Helper: Check if target exists
fn target_exists(target_iqn: &str) -> Result<bool, String> {
    let output = run_command_output(&["targetcli", "iscsi/", "ls"])?;
    Ok(output.lines().any(|line| line.contains(target_iqn)))
}

// Helper: Check if block backstore exists
fn backstore_exists(block_store: &str) -> Result<bool, String> {
    let output = run_command_output(&["targetcli", "backstores/block", "ls"])?;
    Ok(output.lines().any(|line| line.trim().contains(block_store)))
}

// Helper: Check if LUN exists for the backstore path
fn lun_exists(target_iqn: &str, lun_path: &str) -> Result<bool, String> {
    let output = run_command_output(&[
        "targetcli",
        &format!("iscsi/{}/tpg1/luns", target_iqn),
        "ls",
    ])?;
    Ok(output.lines().any(|line| line.contains(lun_path)))
}

// Helper: Check if portal (0.0.0.0:3260) exists
fn portal_exists(target_iqn: &str) -> Result<bool, String> {
    let output = run_command_output(&[
        "targetcli",
        &format!("iscsi/{}/tpg1/portals/", target_iqn),
        "ls",
    ])?;
    Ok(output.lines().any(|line| line.contains("0.0.0.0")))
}