//! ZFS infrastructure adapter.
//!
//! This module is the single infrastructure boundary for ZFS operations.
//!
//! Application/domain code must not invoke `zfs`, `zpool`, or `sudo`
//! directly. Use the concrete ZFS operation adapters through the image backend.

mod clone;
mod command;
mod dataset;
mod snapshot;
mod volume;

#[path = "../../zfs.rs"]
pub mod legacy;

pub mod provider;

pub use clone::ZfsCloneOperations;
pub use command::ZfsCommand;
pub use dataset::ZfsDatasetOperations;
pub use snapshot::ZfsSnapshotOperations;
pub use volume::ZfsVolumeOperations;

pub use provider::{ZfsDatasetInfo, ZfsSnapshotInfo, ZfsVolumeInfo};
