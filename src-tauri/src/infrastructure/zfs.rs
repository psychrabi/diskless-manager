//! ZFS operations service interface
//!
//! This module provides abstractions for ZFS dataset, snapshot, and pool operations.

use crate::core::error::{DisklessError, Result};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::process::Output;

/// ZFS service trait for all ZFS operations
#[async_trait::async_trait]
pub trait ZfsService: Send + Sync {
    /// Check if a dataset exists
    async fn dataset_exists(&self, dataset: &str) -> Result<bool>;

    /// Create a new dataset
    async fn create_dataset(&self, dataset: &str, properties: Option<Vec<(String, String)>>) -> Result<()>;

    /// List all pools
    async fn list_pools(&self) -> Result<Vec<String>>;

    /// Create a snapshot
    async fn create_snapshot(&self, dataset: &str, snapshot_name: &str, recursive: bool) -> Result<()>;

    /// Destroy a dataset or snapshot
    async fn destroy(&self, target: &str, recursive: bool, force: bool) -> Result<()>;

    /// Clone a snapshot to a new dataset
    async fn clone_snapshot(&self, snapshot: &str, clone_dataset: &str) -> Result<()>;

    /// Get ZFS arc statistics
    async fn get_arc_stats(&self) -> Result<ArcStats>;

    /// Check if pool exists
    async fn pool_exists(&self, pool_name: &str) -> Result<bool>;
}

/// ZFS arc statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArcStats {
    pub size: u64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

/// ZFS operation configuration
#[derive(Debug, Clone)]
pub struct ZfsConfig {
    pub default_timeout: Duration,
}

impl Default for ZfsConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(60),
        }
    }
}

/// Real ZFS service implementation
#[derive(Debug, Clone)]
pub struct RealZfsService {
    process_service: Box<dyn crate::infrastructure::process::ProcessService>,
    config: ZfsConfig,
}

impl RealZfsService {
    /// Create a new real ZFS service
    pub fn new(process_service: Box<dyn crate::infrastructure::process::ProcessService>) -> Self {
        Self {
            process_service,
            config: ZfsConfig::default(),
        }
    }

    /// Execute ZFS command with proper error handling
    async fn execute_zfs_command<I>(&self, args: I) -> Result<String>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send,
    {
        let mut zfs_args = vec!["zfs"];
        let args_iter = args.into_iter();
        let output = self.process_service
            .get_command_output(zfs_args.chain(args_iter))
            .await?;

        Ok(output)
    }
}

#[async_trait::async_trait]
impl ZfsService for RealZfsService {
    async fn dataset_exists(&self, dataset: &str) -> Result<bool> {
        match self.execute_zfs_command(["list", "-H", dataset]).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn create_dataset(&self, dataset: &str, properties: Option<Vec<(String, String)>>) -> Result<()> {
        let mut args = vec!["create"];
        
        if let Some(props) = properties {
            for (key, value) in props {
                args.extend(["-o", &format!("{}={}", key, value)]);
            }
        }
        
        args.push(dataset);
        self.process_service.execute_command(args, self.config.default_timeout).await?;
        Ok(())
    }

    async fn list_pools(&self) -> Result<Vec<String>> {
        let output = self.execute_zfs_command(["zpool", "list", "-H", "-o", "name"]).await?;
        
        Ok(output.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
            .collect())
    }

    async fn create_snapshot(&self, dataset: &str, snapshot_name: &str, recursive: bool) -> Result<()> {
        let mut args = vec!["snapshot"];
        if recursive {
            args.push("-r");
        }
        args.extend([&format!("{}@{}", dataset, snapshot_name)]);
        
        self.process_service.execute_command(args, self.config.default_timeout).await?;
        Ok(())
    }

    async fn destroy(&self, target: &str, recursive: bool, force: bool) -> Result<()> {
        let mut args = vec!["destroy"];
        if recursive {
            args.push("-r");
        }
        if force {
            args.push("-f");
        }
        args.push(target);
        
        self.process_service.execute_command(args, self.config.default_timeout).await?;
        Ok(())
    }

    async fn clone_snapshot(&self, snapshot: &str, clone_dataset: &str) -> Result<()> {
        self.process_service.execute_command(["clone", snapshot, clone_dataset], self.config.default_timeout).await?;
        Ok(())
    }

    async fn get_arc_stats(&self) -> Result<ArcStats> {
        // Read from /proc/spl/kstat/zfs/arcstats
        let output = std::fs::read_to_string("/proc/spl/kstat/zfs/arcstats")
            .map_err(|e| DisklessError::internal(format!("Failed to read arcstats: {}", e)))?;

        let mut hits = 0u64;
        let mut misses = 0u64;
        let mut size = 0u64;

        for line in output.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                match parts[0] {
                    "hits" => hits = parts[1].parse().unwrap_or(0),
                    "misses" => misses = parts[1].parse().unwrap_or(0),
                    "size" => size = parts[1].parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        let hit_rate = if hits + misses > 0 {
            (hits as f64 / (hits + misses) as f64) * 100.0
        } else {
            0.0
        };

        Ok(ArcStats {
            size,
            hits,
            misses,
            hit_rate,
        })
    }

    async fn pool_exists(&self, pool_name: &str) -> Result<bool> {
        match self.execute_zfs_command(["zpool", "list", "-H", pool_name]).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}