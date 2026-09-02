//! ZFS infrastructure adapter.
//!
//! This module is the single infrastructure boundary for ZFS operations.
//!
//! Application/domain code must not invoke `zfs`, `zpool`, or `sudo`
//! directly. It should depend on `ZfsProvider`.

mod clone;
mod command;
mod dataset;
mod reconcile;
mod snapshot;
mod volume;

#[path = "../../zfs.rs"]
pub mod legacy;

pub mod provider;

pub use clone::ZfsCloneOperations;
pub use command::ZfsCommand;
pub use dataset::ZfsDatasetOperations;
pub use reconcile::ZfsReconciler;
pub use snapshot::ZfsSnapshotOperations;
pub use volume::ZfsVolumeOperations;

pub use provider::{ZfsDatasetInfo, ZfsProvider, ZfsSnapshotInfo, ZfsVolumeInfo};
