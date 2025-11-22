use tracing::info;
use tracing::warn;

use crate::utils::{run_command, run_command_output, run_command_output_no_sudo};
use crate::error::AppError;

pub fn setup_iscsi_target(
    target_iqn: &str,
    block_store: &str,
    volume_path: &str,
) -> Result<(), AppError> {
    // Check and create iSCSI target if it doesn't exist
    if !target_exists(target_iqn)? {
        run_command(&["targetcli", "/iscsi", "create", target_iqn])?;
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1", target_iqn),
            "set",
            "attribute",
            "generate_node_acls=1",
            "cache_dynamic_acls=1",
            "demo_mode_write_protect=0",
            "authentication=0",
        ]).map_err(|e| AppError::Command(format!("Failed to set target attributes: {}", e)))?;
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
                return Err(AppError::Validation(format!("Invalid volume path format: {}", volume_path)));
            };

            // Check if ZFS dataset exists and is accessible
            if let Err(ve) = run_command_output_no_sudo(&["zfs", "list", &dataset_path]) {
                return Err(AppError::NotFound(format!(
                    "ZFS volume '{}' not found or inaccessible: {}. \
                    Make sure the ZFS dataset exists and has correct permissions.", 
                    dataset_path, ve
                )));
            }

            // Check if the device file exists
            if !std::path::Path::new(volume_path).exists() {
                return Err(AppError::NotFound(format!(
                    "ZFS volume exists but device file '{}' is not available. \
                    Try running 'zfs set volmode=dev {}' to create the device file.", 
                    volume_path, dataset_path
                )));
            }

            // Check if there's an existing block device with the same name
            if let Ok(output) = run_command_output(&["targetcli", "backstores/block", "ls"]) {
                if output.contains(block_store) {
                    return Err(AppError::Validation(format!(
                        "Block device '{}' already exists in targetcli. \
                        Try using a different name or delete the existing one first.", 
                        block_store
                    )));
                }
            }

            // Check if any process is using the device
            if let Ok(output) = run_command_output(&["lsof", volume_path]) {
                return Err(AppError::Validation(format!(
                    "Volume '{}' is in use by other processes:\n{}", 
                    volume_path, output
                )));
            }

            // Get current targetcli configuration for debugging
            let targetcli_state = run_command_output(&["targetcli", "ls"])
                .unwrap_or_else(|_| "Failed to get targetcli state".to_string());

            // If all checks pass but targetcli still fails, return detailed error info
            return Err(AppError::Command(format!(
                "Failed to create iSCSI block backstore:\n\
                - Command failed: targetcli backstores/block create {} {}\n\
                - Error: {}\n\
                - Current targetcli state:\n{}\n\
                Try manually running 'targetcli backstores/block create {} {}' \
                for more detailed error output.",
                block_store, volume_path, e, targetcli_state, block_store, volume_path
            )));
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

pub fn cleanup_iscsi_target(target_iqn: &str, block_store: &str) -> Result<(), AppError> {
    info!(
        "Cleaning up iSCSI target {} and backstore {}",
        target_iqn, block_store
    );

    // Delete iSCSI target (handles LUNs/portals)
    if let Err(e) = run_command(&["targetcli", "/iscsi", "delete", target_iqn]) {
        warn!(
            "Warning: Could not delete target {}: {}",
            target_iqn, e
        );
    } else {
        info!("Deleted iSCSI target {}", target_iqn);
    }

    // Delete backstore if it exists
    if !block_store.is_empty() && backstore_exists(block_store)? {
        if let Err(e) = run_command(&["targetcli", "backstores/block/", "delete", block_store]) {
            warn!(
                "Warning: Could not delete block backstore {}: {}",
                block_store, e
            );
        } else {
            info!("Deleted block backstore {}", block_store);
        }
    }

    // Save configuration
    let _ = run_command(&["targetcli", "saveconfig"]);

    Ok(())
}

// Helper: Check if target exists
fn target_exists(target_iqn: &str) -> Result<bool, AppError> {
    match run_command_output(&["targetcli", "/iscsi", "ls"]) {
        Ok(output) => Ok(output.lines().any(|line| line.contains(target_iqn))),
        Err(_) => {
            // If command fails (e.g., no targets exist yet), treat as target doesn't exist
            Ok(false)
        }
    }
}

// Helper: Check if block backstore exists
fn backstore_exists(block_store: &str) -> Result<bool, AppError> {
    let output = run_command_output(&["targetcli", "backstores/block", "ls"])?;
    Ok(output.lines().any(|line| line.trim().contains(block_store)))
}

// Helper: Check if LUN exists for the backstore path
fn lun_exists(target_iqn: &str, lun_path: &str) -> Result<bool, AppError> {
    let output = run_command_output(&[
        "targetcli",
        &format!("iscsi/{}/tpg1/luns", target_iqn),
        "ls",
    ])?;
    Ok(output.lines().any(|line| line.contains(lun_path)))
}

// Helper: Check if portal (0.0.0.0:3260) exists
fn portal_exists(target_iqn: &str) -> Result<bool, AppError> {
    let output = run_command_output(&[
        "targetcli",
        &format!("iscsi/{}/tpg1/portals/", target_iqn),
        "ls",
    ])?;
    Ok(output.lines().any(|line| line.contains("0.0.0.0")))
}

/// Get all available game disk ZVOLs
fn get_all_game_disks() -> Result<Vec<String>, AppError> {
    use crate::config::get_zpool_name;
    
    let zpool = get_zpool_name();
    let games_parent = format!("{}/games", zpool);
    
    // Check if games dataset exists
    if run_command_output_no_sudo(&["zfs", "list", "-H", &games_parent]).is_err() {
        // No games dataset, return empty list
        return Ok(vec![]);
    }
    
    // List all ZVOLs under games dataset
    let output = run_command_output_no_sudo(&[
        "zfs",
        "list",
        "-H",
        "-t",
        "volume",
        "-o",
        "name",
        "-r",
        &games_parent,
    ])?;
    
    let game_disks: Vec<String> = output
        .lines()
        .filter(|line| !line.is_empty())
        .filter(|line| *line != games_parent) // Exclude parent dataset itself
        .map(|s| format!("/dev/zvol/{}", s))
        .collect();
    
    Ok(game_disks)
}

/// Setup iSCSI target with boot disk (LUN 0) and all game disks (LUN 1+)
pub fn setup_iscsi_target_with_game_disks(
    target_iqn: &str,
    boot_block_store: &str,
    boot_volume_path: &str,
) -> Result<(), AppError> {
    eprintln!("=== setup_iscsi_target_with_game_disks: target_iqn={}", target_iqn);
    
    // Check and create iSCSI target if it doesn't exist
    let exists = target_exists(target_iqn)?;
    eprintln!("=== target_exists returned: {}", exists);
    
    if !exists {
        eprintln!("=== Creating iSCSI target: {}", target_iqn);
        run_command(&["targetcli", "/iscsi", "create", target_iqn])?;
        eprintln!("=== Target created successfully");
        
        eprintln!("=== Setting target attributes");
        run_command(&[
            "targetcli",
            &format!("/iscsi/{}/tpg1", target_iqn),
            "set",
            "attribute",
            "generate_node_acls=1",
            "cache_dynamic_acls=1",
            "demo_mode_write_protect=0",
            "authentication=0",
        ]).map_err(|e| AppError::Command(format!("Failed to set target attributes: {}", e)))?;
        eprintln!("=== Attributes set successfully");
    } else {
        eprintln!("=== Target already exists, skipping creation");
    }

    // Setup LUN 0: Boot disk
    if backstore_exists(boot_block_store)? {
        run_command(&["targetcli", "backstores/block/", "delete", boot_block_store])?;
    }
    
    run_command(&[
        "targetcli",
        "backstores/block",
        "create",
        boot_block_store,
        boot_volume_path,
    ]).map_err(|e| AppError::Command(format!("Failed to create boot disk backstore: {}", e)))?;

    let boot_lun_path = format!("/backstores/block/{}", boot_block_store);
    if !lun_exists(target_iqn, &boot_lun_path)? {
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1/luns", target_iqn),
            "create",
            &boot_lun_path,
            "0",  // Explicit LUN 0 for boot disk
        ])?;
    }

    // Setup LUN 1+: Game disks
    let game_disks = get_all_game_disks()?;
    for (index, game_disk_path) in game_disks.iter().enumerate() {
        let lun_number = index + 1; // Start from LUN 1
        
        // Extract game disk name from path for backstore name
        let game_disk_name = game_disk_path
            .strip_prefix("/dev/zvol/")
            .unwrap_or(game_disk_path)
            .replace('/', "_");
        let game_block_store = format!("game_{}", game_disk_name);
        
        // Create backstore if doesn't exist
        if !backstore_exists(&game_block_store)? {
            if let Err(e) = run_command(&[
                "targetcli",
                "backstores/block",
                "create",
                &game_block_store,
                game_disk_path,
                "readonly=True",  // Make game disks read-only
            ]) {
                warn!("Failed to create game disk backstore {}: {}", game_block_store, e);
                continue; // Skip this game disk but continue with others
            }
        }
        
        // Add as LUN
        let game_lun_path = format!("/backstores/block/{}", game_block_store);
        if !lun_exists(target_iqn, &game_lun_path)? {
            if let Err(e) = run_command(&[
                "targetcli",
                &format!("iscsi/{}/tpg1/luns", target_iqn),
                "create",
                &game_lun_path,
                &lun_number.to_string(),
            ]) {
                warn!("Failed to add game disk LUN {}: {}", lun_number, e);
                // Try to cleanup the backstore we just created
                let _ = run_command(&["targetcli", "backstores/block/", "delete", &game_block_store]);
                continue;
            }
        }
        
        info!("Added game disk {} as LUN {}", game_disk_name, lun_number);
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
    
    info!("Setup iSCSI target {} with {} game disks", target_iqn, game_disks.len());

    Ok(())
}