//! Process and command execution service interface
//!
//! This module provides abstractions for system process execution,
//! replacing the scattered command handling throughout the codebase.

use crate::core::error::{DisklessError, Result};
use std::time::Duration;
use tokio::process::{Child, Command as AsyncCommand};
use std::process::{Command as SyncCommand, Stdio, Output};

/// Process execution service trait
#[async_trait::async_trait]
pub trait ProcessService: Send + Sync {
    /// Execute a command asynchronously with timeout
    async fn execute_command<I>(&self, args: I, timeout: Duration) -> Result<Output>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send;

    /// Execute a command synchronously with timeout
    fn execute_command_sync<I>(&self, args: I, timeout: Duration) -> Result<Output>
    where
        I: IntoIterator,
        I::Item: AsRef<std::ffi::OsStr>;

    /// Spawn a background process
    async fn spawn_command<I>(&self, args: I) -> Result<Child>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send;

    /// Check if a command exists
    fn command_exists(&self, command: &str) -> bool;

    /// Get command output as string
    async fn get_command_output<I>(&self, args: I) -> Result<String>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send;
}

/// Command execution configuration
#[derive(Debug, Clone)]
pub struct CommandConfig {
    pub use_sudo: bool,
    pub timeout: Duration,
    pub working_dir: Option<std::path::PathBuf>,
    pub env_vars: Option<std::collections::HashMap<String, String>>,
}

impl Default for CommandConfig {
    fn default() -> Self {
        Self {
            use_sudo: false,
            timeout: Duration::from_secs(30),
            working_dir: None,
            env_vars: None,
        }
    }
}

/// Command runner utility
#[derive(Debug, Clone)]
pub struct CommandRunner {
    config: CommandConfig,
}

impl CommandRunner {
    /// Create a new command runner with default config
    pub fn new() -> Self {
        Self {
            config: CommandConfig::default(),
        }
    }

    /// Create a new command runner with custom config
    pub fn with_config(config: CommandConfig) -> Self {
        Self { config }
    }

    /// Create a new async command
    pub async fn command<I>(&self, program: I, args: I) -> Result<Output>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send,
    {
        self.build_async_command(program, args).output().await
            .map_err(|e| DisklessError::Process(crate::core::error::ProcessError::ExecutionFailed(e.to_string())))
    }

    /// Build an async command with proper configuration
    async fn build_async_command<I>(&self, program: I, args: I) -> AsyncCommand
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send,
    {
        let mut command = AsyncCommand::new(program.as_ref());
        
        // Add arguments
        command.args(args);
        
        // Configure sudo if needed
        if self.config.use_sudo {
            let mut sudo_args = vec!["-n".to_string()];
            sudo_args.extend(command.as_mut().get_args().map(|s| s.to_string_lossy().to_string()));
            command = AsyncCommand::new("sudo");
            command.args(&sudo_args);
        }
        
        // Set working directory
        if let Some(working_dir) = &self.config.working_dir {
            command.current_dir(working_dir);
        }
        
        // Set environment variables
        if let Some(env_vars) = &self.config.env_vars {
            for (key, value) in env_vars {
                command.env(key, value);
            }
        }
        
        // Configure stdin/stdout/stderr
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        
        command
    }

    /// Execute command with timeout
    pub async fn execute_with_timeout<I>(&self, program: I, args: I) -> Result<Output>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send,
    {
        tokio::time::timeout(self.config.timeout, self.command(program, args))
            .await
            .map_err(|_| DisklessError::timeout("Command execution timed out"))?
    }

    /// Get command output as string
    pub async fn get_output_as_string<I>(&self, program: I, args: I) -> Result<String>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send,
    {
        let output = self.command(program, args).await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.to_string())
    }

    /// Check if command exists in PATH
    pub fn check_command_exists(&self, command: &str) -> bool {
        SyncCommand::new("which")
            .arg(command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

impl Default for CommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Implementation of ProcessService
#[derive(Debug, Clone)]
pub struct RealProcessService {
    runner: CommandRunner,
}

impl RealProcessService {
    /// Create a new real process service
    pub fn new() -> Self {
        Self {
            runner: CommandRunner::new(),
        }
    }

    /// Create a new real process service with custom config
    pub fn with_config(config: CommandConfig) -> Self {
        Self {
            runner: CommandRunner::with_config(config),
        }
    }
}

#[async_trait::async_trait]
impl ProcessService for RealProcessService {
    async fn execute_command<I>(&self, args: I, timeout: Duration) -> Result<Output>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send,
    {
        let mut args_iter = args.into_iter();
        let program = args_iter.next().ok_or_else(|| 
            DisklessError::internal("Command requires at least one argument (program)")
        )?;
        
        let mut runner = self.runner.clone();
        runner.config.timeout = timeout;
        
        let mut args_vec: Vec<_> = args_iter.collect();
        args_vec.insert(0, program.as_ref());
        
        runner.execute_with_timeout(&args_vec[0], &args_vec[1..]).await
    }

    fn execute_command_sync<I>(&self, args: I, timeout: Duration) -> Result<Output>
    where
        I: IntoIterator,
        I::Item: AsRef<std::ffi::OsStr>,
    {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| DisklessError::internal(format!("Failed to create runtime: {}", e)))?;
        
        rt.block_on(self.execute_command(args, timeout))
    }

    async fn spawn_command<I>(&self, args: I) -> Result<Child>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send,
    {
        let mut args_iter = args.into_iter();
        let program = args_iter.next().ok_or_else(|| 
            DisklessError::internal("Command requires at least one argument (program)")
        )?;
        
        let mut args_vec: Vec<_> = args_iter.collect();
        args_vec.insert(0, program.as_ref());
        
        let mut command = AsyncCommand::new(&args_vec[0]);
        command.args(&args_vec[1..]);
        
        if self.runner.config.use_sudo {
            let mut sudo_args = vec!["-n".to_string()];
            sudo_args.extend(command.as_mut().get_args().map(|s| s.to_string_lossy().to_string()));
            command = AsyncCommand::new("sudo");
            command.args(&sudo_args);
        }
        
        command.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| DisklessError::Process(crate::core::error::ProcessError::ExecutionFailed(e.to_string())))
    }

    fn command_exists(&self, command: &str) -> bool {
        self.runner.check_command_exists(command)
    }

    async fn get_command_output<I>(&self, args: I) -> Result<String>
    where
        I: IntoIterator + Send,
        I::Item: AsRef<std::ffi::OsStr> + Send,
    {
        let mut args_iter = args.into_iter();
        let program = args_iter.next().ok_or_else(|| 
            DisklessError::internal("Command requires at least one argument (program)")
        )?;
        
        let mut args_vec: Vec<_> = args_iter.collect();
        args_vec.insert(0, program.as_ref());
        
        self.runner.get_output_as_string(&args_vec[0], &args_vec[1..]).await
    }
}