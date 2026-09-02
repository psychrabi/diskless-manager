use crate::error::AppError;
use parking_lot::Mutex;
use ssh2::Session;
use std::collections::HashMap;
use std::io::Read;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Result of SSH command execution
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// SSH connection configuration
#[derive(Debug, Clone)]
pub struct SshConfig {
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Command execution timeout in seconds
    pub command_timeout: u64,
    /// SSH username
    pub username: String,
    /// Whether to disable host key verification
    pub disable_host_key_verification: bool,
    /// Maximum number of retries on connection failure
    pub max_retries: u32,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            connection_timeout: 5,
            command_timeout: 30,
            username: "root".to_string(),
            disable_host_key_verification: true,
            max_retries: 1,
        }
    }
}

/// SSH connection pool entry
struct PooledConnection {
    session: Session,
    last_used: std::time::Instant,
}

/// SSH executor with connection pooling and retry logic
pub struct SshExecutor {
    pub config: SshConfig,
    connection_pool: Arc<Mutex<HashMap<String, PooledConnection>>>,
}

impl SshExecutor {
    /// Create a new SSH executor with default configuration
    pub fn new() -> Self {
        Self::with_config(SshConfig::default())
    }

    /// Create a new SSH executor with custom configuration
    pub fn with_config(config: SshConfig) -> Self {
        Self {
            config,
            connection_pool: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Execute a command on a remote host with retry logic
    pub async fn execute_with_retry(
        &self,
        host: &str,
        command: &str,
    ) -> Result<CommandResult, AppError> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match self.execute_command(host, command).await {
                Ok(result) => {
                    info!(
                        "SSH command executed successfully on {} (attempt {})",
                        host,
                        attempt + 1
                    );
                    return Ok(result);
                }
                Err(e) => {
                    warn!(
                        "SSH command failed on {} (attempt {}): {}",
                        host,
                        attempt + 1,
                        e
                    );
                    last_error = Some(e);

                    // Don't retry on the last attempt
                    if attempt < self.config.max_retries {
                        // Brief delay before retry
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }

        error!(
            "SSH command failed after {} retries on {}",
            self.config.max_retries + 1,
            host
        );
        Err(last_error.unwrap_or_else(|| AppError::SshConnection("Unknown SSH error".to_string())))
    }

    /// Execute a command on a remote host
    pub async fn execute_command(
        &self,
        host: &str,
        command: &str,
    ) -> Result<CommandResult, AppError> {
        let start_time = std::time::Instant::now();

        debug!("Executing SSH command on {}: {}", host, command);

        // Establish connection
        let session = self.get_or_create_connection(host).await?;

        // Execute command with timeout.
        let host_owned = host.to_string();
        let command_owned = command.to_string();
        let timeout_secs = self.config.command_timeout;

        // All blocking channel/read operations run inside spawn_blocking so a
        // slow remote host cannot stall the async runtime.
        let spawned = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            tokio::task::spawn_blocking(move || {
                Self::execute_command_internal_blocking(&session, &command_owned)
            }),
        )
        .await
        .map_err(|_| {
            error!("SSH command timeout on {}", host_owned);
            AppError::SshTimeout
        })?;

        let result = spawned
            .map_err(|e| {
                error!("SSH worker task failed on {}: {}", host_owned, e);
                AppError::SshCommand(format!("SSH worker task failed: {}", e))
            })??;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        info!(
            "SSH command completed on {} with exit code {} ({}ms)",
            host_owned,
            result.exit_code,
            duration_ms
        );

        Ok(CommandResult {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
            duration_ms,
        })
    }

    /// Check SSH connectivity to a host
    pub async fn check_connectivity(&self, host: &str) -> Result<bool, AppError> {
        debug!("Checking SSH connectivity to {}", host);

        match self.get_or_create_connection(host).await {
            Ok(_) => {
                info!("SSH connectivity check passed for {}", host);
                Ok(true)
            }
            Err(e) => {
                warn!("SSH connectivity check failed for {}: {}", host, e);
                Ok(false)
            }
        }
    }

    /// Get or create a connection from the pool
    async fn get_or_create_connection(&self, host: &str) -> Result<Session, AppError> {
        // Check if we have a pooled connection
        {
            let mut pool = self.connection_pool.lock();
            if let Some(pooled) = pool.get_mut(host) {
                // Check if connection is still valid (simple check)
                if pooled.last_used.elapsed() < Duration::from_secs(300) {
                    debug!("Reusing pooled SSH connection to {}", host);
                    pooled.last_used = std::time::Instant::now();
                    // Session::clone shares the underlying connection via Arc
                    return Ok(pooled.session.clone());
                } else {
                    // Connection expired, remove and recreate
                    pool.remove(host);
                }
            }
        }

        // Create new connection
        let session = self.create_connection(host).await?;

        // Store in pool for future reuse
        {
            let mut pool = self.connection_pool.lock();
            pool.insert(
                host.to_string(),
                PooledConnection {
                    session: session.clone(),
                    last_used: std::time::Instant::now(),
                },
            );
        }

        Ok(session)
    }

    /// Create a new SSH connection
    async fn create_connection(&self, host: &str) -> Result<Session, AppError> {
        debug!("Creating new SSH connection to {}", host);

        let host_owned = host.to_string();
        let user = self.config.username.clone();
        let timeout = self.config.connection_timeout;

        // All blocking libssh2 / network calls must run off the async runtime.
        tokio::task::spawn_blocking(move || Self::create_connection_blocking(&host_owned, &user, timeout))
            .await
            .map_err(|e| {
                error!("SSH connection task panicked: {}", e);
                AppError::SshConnection(format!("SSH worker task error: {}", e))
            })?
    }

    /// Blocking-only SSH connection logic (runs in spawn_blocking).
    fn create_connection_blocking(
        host: &str,
        username: &str,
        connection_timeout: u64,
    ) -> Result<Session, AppError> {
        let tcp = TcpStream::connect(format!("{}:22", host)).map_err(|e| {
            error!("Failed to connect to SSH server at {}: {}", host, e);
            AppError::SshConnection(format!("Failed to connect to {}: {}", host, e))
        })?;

        tcp.set_read_timeout(Some(Duration::from_secs(connection_timeout)))
            .map_err(|e| {
                error!("Failed to set read timeout: {}", e);
                AppError::SshConnection(format!("Failed to set read timeout: {}", e))
            })?;

        tcp.set_write_timeout(Some(Duration::from_secs(connection_timeout)))
            .map_err(|e| {
                error!("Failed to set write timeout: {}", e);
                AppError::SshConnection(format!("Failed to set write timeout: {}", e))
            })?;

        let mut session = Session::new().map_err(|e| {
            error!("Failed to create SSH session: {}", e);
            AppError::SshConnection(format!("Failed to create SSH session: {}", e))
        })?;

        session.set_tcp_stream(tcp);

        // Handshake
        session.handshake().map_err(|e| {
            error!("SSH handshake failed with {}: {}", host, e);
            AppError::SshConnection(format!("SSH handshake failed: {}", e))
        })?;

        // Authenticate with agent (public key)
        session.userauth_agent(username).map_err(|e| {
            error!(
                "SSH authentication failed for user {} on {}: {}",
                username, host, e
            );
            AppError::SshAuth(format!("SSH authentication failed: {}", e))
        })?;

        if !session.authenticated() {
            error!(
                "SSH authentication not successful for {} on {}",
                username, host
            );
            return Err(AppError::SshAuth(
                "SSH authentication not successful".to_string(),
            ));
        }

        info!("SSH connection established to {}", host);

        Ok(session)
    }

    /// Internal command execution (blocking-only, runs inside spawn_blocking).
    fn execute_command_internal_blocking(
        session: &Session,
        command: &str,
    ) -> Result<CommandResult, AppError> {
        let mut channel = session.channel_session().map_err(|e| {
            error!("Failed to open SSH channel: {}", e);
            AppError::SshCommand(format!("Failed to open SSH channel: {}", e))
        })?;

        channel.exec(command).map_err(|e| {
            error!("Failed to execute SSH command: {}", e);
            AppError::SshCommand(format!("Failed to execute SSH command: {}", e))
        })?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        channel.read_to_string(&mut stdout).map_err(|e| {
            error!("Failed to read SSH stdout: {}", e);
            AppError::SshCommand(format!("Failed to read SSH stdout: {}", e))
        })?;

        channel.stderr().read_to_string(&mut stderr).map_err(|e| {
            error!("Failed to read SSH stderr: {}", e);
            AppError::SshCommand(format!("Failed to read SSH stderr: {}", e))
        })?;

        channel.wait_close().map_err(|e| {
            error!("Failed to close SSH channel: {}", e);
            AppError::SshCommand(format!("Failed to close SSH channel: {}", e))
        })?;

        let exit_code = channel.exit_status().unwrap_or(-1);

        if exit_code != 0 {
            warn!("SSH command exited with code {}: {}", exit_code, stderr);
        }

        Ok(CommandResult {
            exit_code,
            stdout,
            stderr,
            duration_ms: 0, // Will be set by caller
        })
    }

    /// Clear the connection pool
    pub fn clear_pool(&self) {
        let mut pool = self.connection_pool.lock();
        pool.clear();
        debug!("SSH connection pool cleared");
    }
}

impl Default for SshExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_config_default() {
        let config = SshConfig::default();
        assert_eq!(config.connection_timeout, 5);
        assert_eq!(config.command_timeout, 30);
        assert_eq!(config.username, "root");
        assert!(config.disable_host_key_verification);
        assert_eq!(config.max_retries, 1);
    }

    #[test]
    fn test_command_result_creation() {
        let result = CommandResult {
            exit_code: 0,
            stdout: "test output".to_string(),
            stderr: String::new(),
            duration_ms: 100,
        };
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "test output");
        assert_eq!(result.duration_ms, 100);
    }

    #[test]
    fn test_ssh_executor_creation() {
        let executor = SshExecutor::new();
        assert_eq!(executor.config.connection_timeout, 5);
        assert_eq!(executor.config.command_timeout, 30);
    }

    #[test]
    fn test_ssh_executor_with_custom_config() {
        let config = SshConfig {
            connection_timeout: 10,
            command_timeout: 60,
            username: "admin".to_string(),
            disable_host_key_verification: false,
            max_retries: 2,
        };
        let executor = SshExecutor::with_config(config);
        assert_eq!(executor.config.connection_timeout, 10);
        assert_eq!(executor.config.command_timeout, 60);
        assert_eq!(executor.config.username, "admin");
        assert!(!executor.config.disable_host_key_verification);
        assert_eq!(executor.config.max_retries, 2);
    }
}
