use anyhow::{Context, Result};
use std::ffi::OsStr;

use crate::cmd::{run_command, run_command_check, run_command_output_no_sudo};

/// Low-level ZFS command adapter.
///
/// This is the only V2 ZFS module that should know how the `zfs`
/// command-line interface is invoked.
#[derive(Debug, Clone, Copy, Default)]
pub struct ZfsCommand;

impl ZfsCommand {
    pub const fn new() -> Self {
        Self
    }

    /// Execute a privileged ZFS command.
    pub fn execute<I, S>(&self, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run_command(args)
            .map_err(anyhow::Error::from)
            .context("ZFS command failed")
    }

    /// Execute a privileged ZFS command and return stdout.
    pub fn execute_output<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run_command_output_no_sudo(args)
            .map_err(anyhow::Error::from)
            .context("ZFS query failed")
    }

    /// Check whether a ZFS command succeeds.
    pub fn check<I, S>(&self, args: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run_command_check(args) == 0
    }

    /// Query a ZFS property.
    pub fn get_property(&self, property: &str, dataset: &str) -> Result<Option<String>> {
        let output = self.execute_output(["zfs", "get", "-H", "-o", "value", property, dataset])?;

        let value = output.trim();

        if value.is_empty() || value == "-" {
            Ok(None)
        } else {
            Ok(Some(value.to_string()))
        }
    }

    /// Set a ZFS property.
    pub fn set_property(&self, property: &str, value: &str, dataset: &str) -> Result<()> {
        self.execute(["zfs", "set", &format!("{property}={value}"), dataset])
    }
}
