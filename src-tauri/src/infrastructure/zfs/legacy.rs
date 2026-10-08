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

/// Resolve the client's clone destination under the first writeback dataset,
/// falling back to the pool root when no writeback dataset can be listed.
///
/// # Arguments
///
/// * `client_name` - Client name used to build the destination dataset name.
///
/// # Returns
///
/// The full destination dataset name for the client's writeback clone.
pub fn get_writeback_or_default_dataset(client_name: &str) -> String {
    let zpool = get_zpool_name();
    let listing = run_command_output_no_sudo(&[
        "zfs",
        "list",
        "-H",
        "-o",
        "name,org.diskless:type",
        "-r",
        &zpool,
    ])
    .unwrap_or_default();

    client_writeback_dataset(client_name, &zpool, &listing)
}

fn writeback_parent(listing: &str) -> Option<&str> {
    listing.lines().find_map(|line| {
        let (name, kind) = line.split_once('\t')?;
        (kind.trim() == "writeback").then_some(name)
    })
}

fn client_writeback_dataset(client_name: &str, pool: &str, listing: &str) -> String {
    let parent = writeback_parent(listing).unwrap_or(pool);
    format!("{}/{}-disk", parent, client_name.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::{client_writeback_dataset, writeback_parent};

    #[test]
    fn bulk_listing_selects_first_writeback_and_ignores_other_types() {
        assert_eq!(writeback_parent("diskless\t-\ndiskless/images\timage\ndiskless/clients\twriteback\ndiskless/clients/nested\twriteback\n"), Some("diskless/clients"));
        assert_eq!(
            writeback_parent("diskless\t-\ndiskless/images\timage\n"),
            None
        );
        assert_eq!(writeback_parent("\ninvalid\n"), None);
    }

    #[test]
    fn client_clone_is_created_under_writeback_dataset_when_available() {
        assert_eq!(
            client_writeback_dataset(
                "pc001",
                "diskless",
                "diskless\t-\ndiskless/images\timage\ndiskless/writeback\twriteback\n"
            ),
            "diskless/writeback/PC001-disk"
        );
    }

    #[test]
    fn client_clone_falls_back_to_pool_root_without_writeback_dataset() {
        assert_eq!(
            client_writeback_dataset("pc001", "diskless", "diskless\t-\n"),
            "diskless/PC001-disk"
        );
    }
}
