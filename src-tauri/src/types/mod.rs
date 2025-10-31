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
pub use config::{Config, AppConfig};
pub use client::{Client, AddClientRequest, ControlRequest, DeprovisionRequest};
pub use auth::{User, Claims, LoginRequest, LoginResponse, UserResponse, AuthError};
pub use service::{ServiceControlRequest, PackageStatus, DHCPConfig, TFTPConfig, HTTPConfig};
pub use image::{Master, Snapshot, MasterData};
pub use disk::{DatasetInfo};