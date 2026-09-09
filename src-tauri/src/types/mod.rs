//! Shared types across the application
//!
//! This module contains all the domain models and DTOs used throughout
//! the application to ensure type consistency.

pub mod auth;
pub mod client;
pub mod config;
pub mod disk;
pub mod image;

// Re-export commonly used types
pub use auth::{AuthError, Claims, LoginRequest, LoginResponse, User, UserResponse};
pub use client::AddClientRequest;
pub use config::AppConfig;
pub use disk::{DatasetOperationResponse, Disk, MemoryStats, RamUsage};
pub use image::Snapshot;
