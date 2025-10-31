// Core module - contains domain logic and business rules
pub mod error;
pub mod config;

pub mod auth;
pub mod client;
pub mod image;
pub mod disk;
pub mod service;
pub mod license;

// Re-export commonly used types
pub use error::{DisklessError, Result};
pub use config::ConfigManager;
pub use auth::domain::AuthDomain;
pub use client::domain::ClientDomain;
pub use image::domain::ImageDomain;
pub use disk::domain::DiskDomain;
pub use service::domain::ServiceDomain;
pub use license::domain::LicenseDomain;