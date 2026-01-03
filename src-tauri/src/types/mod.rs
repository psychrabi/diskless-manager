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
pub use client::{AddClientRequest, Client, ClientOverview, ControlRequest, EditClientRequest};
pub use config::AppConfig;
pub use disk::{
    CreateDatasetRequest, CreateZpoolRequest, DatasetInfo, DatasetOperationResponse, Disk,
    MemoryStats, RamUsage,
};
pub use image::{
    ArcstatInfo, CreateImageRequest, CreateSnapshotRequest, ImageOperationResponse, Master,
    MasterData, Snapshot, ZpoolInfo,
};
