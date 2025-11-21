//! Shared types across the application
//!
//! This module contains all the domain models and DTOs used throughout
//! the application to ensure type consistency.

pub mod config;
pub mod client;
pub mod auth;
pub mod service;
pub mod image;
pub mod disk;

// Re-export commonly used types
pub use config::AppConfig;
pub use client::{Client, AddClientRequest, ControlRequest, DeprovisionRequest, ClientOverview};
pub use auth::{User, Claims, LoginRequest, LoginResponse, UserResponse, AuthError};
pub use service::{ServiceControlRequest, PackageStatus, DHCPConfig, TFTPConfig, HTTPConfig, SambaShare};
pub use image::{Master, Snapshot, MasterData, ZpoolInfo, ArcstatInfo, CreateImageRequest, CreateSnapshotRequest, ImageOperationResponse};
pub use disk::{DatasetInfo, Disk, MemoryStats, RamUsage, CreateZpoolRequest, CreateDatasetRequest, DatasetOperationResponse};
