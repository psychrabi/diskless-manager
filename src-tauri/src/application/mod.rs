//! Application layer - use cases and command handlers
//!
//! This module contains the application logic that coordinates between
//! domain services and the Tauri command interface.

pub mod auth_commands;
pub mod client_commands;
pub mod service_commands;
pub mod image_commands;
pub mod disk_commands;
pub mod license_commands;

// Re-export command handlers
pub use auth_commands::AuthCommands;
pub use client_commands::ClientCommands;
pub use service_commands::ServiceCommands;
pub use image_commands::ImageCommands;
pub use disk_commands::DiskCommands;
pub use license_commands::LicenseCommands;