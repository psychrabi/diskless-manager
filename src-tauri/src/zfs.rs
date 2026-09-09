//! Compatibility helpers still used by provisioning and rollback.

use crate::config::get_zpool_name;
use crate::error::AppError;
use crate::infrastructure::command::run_command_output_no_sudo;
use crate::infrastructure::zfs::ZfsCommand;

pub fn zfs_destroy(dataset: &str) -> Result<(), AppError> {
    ZfsCommand::new()
        .execute(["zfs", "destroy", dataset])
        .map_err(|error| AppError::Command(error.to_string()))
}

pub fn get_writeback_or_default_dataset(client_name: &str) -> String {
    let zpool = get_zpool_name();
    let mut writeback_path = format!("{}/{}-disk", zpool, client_name.to_uppercase());

    if let Ok(pool_list) =
        run_command_output_no_sudo(&["zfs", "list", "-H", "-o", "name", "-r", &zpool])
    {
        if let Some(parent) =
            pool_list
                .lines()
                .filter(|line| !line.is_empty())
                .find_map(|dataset| {
                    match run_command_output_no_sudo(&[
                        "zfs",
                        "get",
                        "-H",
                        "-o",
                        "value",
                        "org.diskless:type",
                        dataset,
                    ]) {
                        Ok(value) if value.trim() == "writeback" => Some(dataset.to_string()),
                        _ => None,
                    }
                })
        {
            writeback_path = format!("{}/{}-disk", parent, client_name.to_uppercase());
        }
    }

    writeback_path
}
