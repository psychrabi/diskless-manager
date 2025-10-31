//! Infrastructure layer - external system integrations
//!
//! This module provides abstractions for all external system interactions
//! including filesystems, processes, ZFS operations, and network services.

pub mod filesystem;
pub mod process;
pub mod zfs;
pub mod dhcp;
pub mod iscsi;
pub mod http;
pub mod logging;

// Re-export commonly used traits and types
pub use filesystem::{FilesystemService, FileService};
pub use process::{ProcessService, CommandRunner};
pub use zfs::{ZfsService, ZfsOperations};
pub use dhcp::{DhcpService, DhcpConfiguration};
pub use iscsi::{IscsiService, IscsiTarget};
pub use http::{HttpService, LicenseClient};
pub use logging::{LoggingService, LogManager};