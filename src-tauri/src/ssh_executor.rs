use crate::error::AppError;
use ssh2::Session;
use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
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
    /// Optional password for password-based authentication.
    pub password: Option<String>,
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
            password: None,
            disable_host_key_verification: true,
            max_retries: 1,
        }
    }
}

/// SSH executor with isolated command sessions and retry logic.
pub struct SshExecutor {
    pub config: SshConfig,
}

impl SshExecutor {
    /// Create a new SSH executor with default configuration
    pub fn new() -> Self {
        Self::with_config(SshConfig::default())
    }

    /// Create a new SSH executor with custom configuration
    pub fn with_config(config: SshConfig) -> Self {
        Self { config }
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

        let host_owned = host.to_string();
        let worker_host = host_owned.clone();
        let command_owned = command.to_string();
        let username = self.config.username.clone();
        let password = self.config.password.clone();
        let connection_timeout = self.config.connection_timeout;
        let timeout_secs = self.config.command_timeout;

        // Each worker owns its SSH session. If the async timeout fires, the
        // blocking worker may take until its socket deadline to unwind, but it
        // cannot race with a retry or mutate a shared session.
        let spawned = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            tokio::task::spawn_blocking(move || {
                let session = Self::create_connection_blocking(
                    &worker_host,
                    &username,
                    password.as_deref(),
                    connection_timeout,
                    timeout_secs,
                )?;
                Self::execute_command_internal_blocking(&session, &command_owned)
            }),
        )
        .await
        .map_err(|_| {
            error!("SSH command timeout on {}", host_owned);
            AppError::SshTimeout
        })?;

        let result = spawned.map_err(|e| {
            error!("SSH worker task failed on {}: {}", host_owned, e);
            AppError::SshCommand(format!("SSH worker task failed: {}", e))
        })??;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        info!(
            "SSH command completed on {} with exit code {} ({}ms)",
            host_owned, result.exit_code, duration_ms
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

        match self.create_connection(host).await {
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

    /// Create a new SSH connection
    async fn create_connection(&self, host: &str) -> Result<Session, AppError> {
        debug!("Creating new SSH connection to {}", host);

        let host_owned = host.to_string();
        let user = self.config.username.clone();
        let password = self.config.password.clone();
        let connection_timeout = self.config.connection_timeout;

        // All blocking libssh2 / network calls must run off the async runtime.
        tokio::task::spawn_blocking(move || {
            Self::create_connection_blocking(
                &host_owned,
                &user,
                password.as_deref(),
                connection_timeout,
                connection_timeout,
            )
        })
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
        password: Option<&str>,
        connection_timeout: u64,
        io_timeout: u64,
    ) -> Result<Session, AppError> {
        let address = format!("{}:22", host)
            .to_socket_addrs()
            .map_err(|e| AppError::SshConnection(format!("Failed to resolve {}: {}", host, e)))?
            .next()
            .ok_or_else(|| AppError::SshConnection(format!("No address found for {}", host)))?;
        let tcp = TcpStream::connect_timeout(&address, Duration::from_secs(connection_timeout))
            .map_err(|e| {
                error!("Failed to connect to SSH server at {}: {}", host, e);
                AppError::SshConnection(format!("Failed to connect to {}: {}", host, e))
            })?;

        tcp.set_read_timeout(Some(Duration::from_secs(io_timeout)))
            .map_err(|e| {
                error!("Failed to set read timeout: {}", e);
                AppError::SshConnection(format!("Failed to set read timeout: {}", e))
            })?;

        tcp.set_write_timeout(Some(Duration::from_secs(io_timeout)))
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

        // Authenticate. Prefer the password when provided; fall back to the
        // SSH agent (public key) so existing key-based setups keep working.
        let auth_result = match password {
            Some(password) => session
                .userauth_password(username, password)
                .map_err(|e| AppError::SshAuth(format!("SSH password authentication failed: {e}"))),
            None => session
                .userauth_agent(username)
                .map_err(|e| AppError::SshAuth(format!("SSH agent authentication failed: {e}"))),
        };

        auth_result?;

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

    /// Retained for API compatibility; sessions are no longer shared.
    pub fn clear_pool(&self) {
        debug!("SSH sessions are isolated; there is no connection pool to clear");
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
            password: Some("testpass".to_string()),
            disable_host_key_verification: false,
            max_retries: 2,
        };
        let executor = SshExecutor::with_config(config);
        assert_eq!(executor.config.connection_timeout, 10);
        assert_eq!(executor.config.command_timeout, 60);
        assert_eq!(executor.config.username, "admin");
        assert_eq!(executor.config.password.as_deref(), Some("testpass"));
        assert!(!executor.config.disable_host_key_verification);
        assert_eq!(executor.config.max_retries, 2);
    }
}
