use std::process::Command;

use crate::utils::run_command;

pub fn setup_iscsi_target(
    target_iqn: &str,
    block_store: &str,
    volume_path: &str,
) -> Result<(), String> {
    // Create target if it doesn't exist
    let output = Command::new("sudo")
        .args(["targetcli", "iscsi/", "ls"])
        .output()
        .map_err(|e| format!("Failed to run targetcli iscsi/ ls: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains(target_iqn) {
        run_command(&["targetcli", "iscsi/", "create", target_iqn])?;
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1", target_iqn),
            "set",
            "attribute",
            "generate_node_acls=1",
        ])?;
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1", target_iqn),
            "set",
            "attribute",
            "cache_dynamic_acls=1",
        ])?;
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1", target_iqn),
            "set",
            "attribute",
            "demo_mode_write_protect=0",
        ])?;
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1", target_iqn),
            "set",
            "attribute",
            "authentication=0",
        ])?;
    }

    // Create or update block store
    let output = Command::new("sudo")
        .args(["targetcli", "backstores/block/", "ls"])
        .output()
        .map_err(|e| format!("Failed to run targetcli backstores/block/ ls: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains(block_store) {
        run_command(&["targetcli", "backstores/block/", "delete", block_store])?;
    }
    run_command(&[
        "targetcli",
        "backstores/block",
        "create",
        block_store,
        volume_path,
    ])?;

    // Create LUN if it doesn't exist
    let output = Command::new("sudo")
        .args([
            "targetcli",
            &format!("iscsi/{}/tpg1/luns", target_iqn),
            "ls",
        ])
        .output()
        .map_err(|e| format!("Failed to run targetcli iscsi/.../luns ls: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains(block_store) {
        run_command(&[
            "targetcli",
            &format!("iscsi/{}/tpg1/luns", target_iqn),
            "create",
            &format!("/backstores/block/{}", block_store),
        ])?;
    }

    // Ensure portal exists
    let output = Command::new("sudo")
        .args([
            "targetcli",
            &format!("iscsi/{}/tpg1/portals/", target_iqn),
            "ls",
        ])
        .output()
        .map_err(|e| format!("Failed to run targetcli iscsi/.../portals/ ls: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("0.0.0.0") {
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
        "Cleaning up iSCSI target {} and block store {}",
        target_iqn, block_store
    );

    // Try to delete the iSCSI target
    match run_command(&["targetcli", "iscsi/", "delete", target_iqn]) {
        Ok(_) => println!("Deleted iSCSI target {}", target_iqn),
        Err(e) => println!(
            "Warning: Could not delete target {} directly: {}",
            target_iqn, e
        ),
    }

    // Delete block store if it exists
    if !block_store.is_empty() {
        match Command::new("sudo")
            .args(&["targetcli", "backstores/block", "ls"])
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains(block_store) {
                    println!("Deleting block store {}", block_store);
                    if let Err(e) =
                        run_command(&["targetcli", "backstores/block/", "delete", block_store])
                    {
                        println!(
                            "Warning: Could not delete block store {}: {}",
                            block_store, e
                        );
                    }
                }
            }
            Err(e) => println!("Warning: Could not list block stores: {}", e),
        }
    }

    // Save the configuration
    let _ = run_command(&["targetcli", "saveconfig"]);

    Ok(())
}
