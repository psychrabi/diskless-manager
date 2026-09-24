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

    if let Ok(pool_list) = run_command_output_no_sudo(&[
        "zfs",
        "list",
        "-H",
        "-o",
        "name,org.diskless:type",
        "-r",
        &zpool,
    ]) {
        if let Some(parent) = writeback_parent(&pool_list) {
            writeback_path = format!("{}/{}-disk", parent, client_name.to_uppercase());
        }
    }

    writeback_path
}

fn writeback_parent(listing: &str) -> Option<&str> {
    listing.lines().find_map(|line| {
        let (name, kind) = line.split_once('\t')?;
        (kind.trim() == "writeback").then_some(name)
    })
}

#[cfg(test)]
mod tests {
    use super::writeback_parent;

    #[test]
    fn bulk_listing_selects_first_writeback_and_ignores_other_types() {
        assert_eq!(writeback_parent("diskless\t-\ndiskless/images\timage\ndiskless/clients\twriteback\ndiskless/clients/nested\twriteback\n"), Some("diskless/clients"));
        assert_eq!(
            writeback_parent("diskless\t-\ndiskless/images\timage\n"),
            None
        );
        assert_eq!(writeback_parent("\ninvalid\n"), None);
    }
}
