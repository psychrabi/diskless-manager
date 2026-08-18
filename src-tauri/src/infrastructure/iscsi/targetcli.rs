use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::cmd::{run_command, run_command_output_no_sudo};

use super::model::{IscsiLunSpec, IscsiLunState, IscsiTargetSpec, IscsiTargetState};

/// Infrastructure boundary for Linux LIO / targetcli.
pub trait IscsiProvisioner: Send + Sync {
    /// Create or reconcile the desired target configuration.
    fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()>;

    /// Remove an entire iSCSI target.
    ///
    /// Removing the target also removes its LUN and portal associations.
    ///
    /// Backstores are deliberately preserved because a backstore may be
    /// shared independently of the target.
    fn remove_target(&self, target_iqn: &str) -> Result<()>;

    /// Remove an iSCSI target and explicitly-owned backstores.
    ///
    /// The target is removed first so all LUN associations are detached.
    /// Only the supplied backstores are deleted.
    fn remove_target_with_backstores(&self, target_iqn: &str, backstores: &[String]) -> Result<()>;

    /// Check whether the target exists.
    fn target_exists(&self, target_iqn: &str) -> Result<bool>;

    /// Inspect the actual target configuration.
    fn inspect_target(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState>;

    /// Reconcile actual infrastructure with desired state.
    fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()>;
}

/// targetcli-fb implementation of the iSCSI provisioner.
#[derive(Debug, Clone, Copy, Default)]
pub struct TargetCliProvisioner;

impl TargetCliProvisioner {
    pub const fn new() -> Self {
        Self
    }

    // ========================================================================
    // COMMAND HELPERS
    // ========================================================================

    fn execute<I, S>(&self, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        run_command(args)
            .map_err(anyhow::Error::from)
            .context("targetcli command failed")
    }

    fn output<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        run_command_output_no_sudo(args)
            .map_err(anyhow::Error::from)
            .context("targetcli query failed")
    }

    // ========================================================================
    // PATH HELPERS
    // ========================================================================

    fn tpg_path(target_iqn: &str) -> String {
        format!("/iscsi/{target_iqn}/tpg1")
    }

    fn lun_path(target_iqn: &str) -> String {
        format!("/iscsi/{target_iqn}/tpg1/luns")
    }

    fn portal_path(target_iqn: &str) -> String {
        format!("/iscsi/{target_iqn}/tpg1/portals")
    }

    fn backstore_path(backstore: &str) -> String {
        format!("/backstores/block/{backstore}")
    }

    // ========================================================================
    // VALIDATION
    // ========================================================================

    fn validate_spec(spec: &IscsiTargetSpec) -> Result<()> {
        if spec.target_iqn.trim().is_empty() {
            bail!("iSCSI target IQN cannot be empty");
        }

        if spec.luns.is_empty() {
            bail!(
                "iSCSI target '{}' must contain at least one LUN",
                spec.target_iqn
            );
        }

        let mut seen_luns = std::collections::HashSet::new();
        let mut seen_backstores = std::collections::HashSet::new();

        for lun in &spec.luns {
            if lun.backstore.trim().is_empty() {
                bail!("iSCSI backstore name cannot be empty");
            }

            if !seen_luns.insert(lun.lun) {
                bail!(
                    "duplicate LUN number {} in target '{}'",
                    lun.lun,
                    spec.target_iqn
                );
            }

            if !seen_backstores.insert(lun.backstore.clone()) {
                bail!(
                    "duplicate backstore '{}' in target '{}'",
                    lun.backstore,
                    spec.target_iqn
                );
            }

            if !lun.block_device.is_absolute() {
                bail!(
                    "iSCSI block device must be an absolute path: {}",
                    lun.block_device.display()
                );
            }

            if !Path::new(&lun.block_device).exists() {
                bail!(
                    "iSCSI block device does not exist: {}",
                    lun.block_device.display()
                );
            }
        }

        if spec.portal_address.trim().is_empty() {
            bail!("iSCSI portal address cannot be empty");
        }

        if spec.portal_port == 0 {
            bail!("iSCSI portal port cannot be zero");
        }

        Ok(())
    }

    // ========================================================================
    // TARGET
    // ========================================================================

    fn create_target_object(&self, spec: &IscsiTargetSpec) -> Result<()> {
        if self.target_exists(&spec.target_iqn)? {
            return Ok(());
        }

        self.execute(["targetcli", "/iscsi", "create", &spec.target_iqn])
            .with_context(|| format!("failed to create iSCSI target '{}'", spec.target_iqn))
    }

    fn configure_tpg(&self, spec: &IscsiTargetSpec) -> Result<()> {
        let tpg = Self::tpg_path(&spec.target_iqn);

        self.execute([
            "targetcli",
            &tpg,
            "set",
            "attribute",
            "generate_node_acls=1",
        ])
        .context("failed to enable generated node ACLs")?;

        self.execute([
            "targetcli",
            &tpg,
            "set",
            "attribute",
            "cache_dynamic_acls=1",
        ])
        .context("failed to enable dynamic ACL caching")?;

        self.execute([
            "targetcli",
            &tpg,
            "set",
            "attribute",
            "demo_mode_write_protect=0",
        ])
        .context("failed to disable demo-mode write protection")?;

        self.execute(["targetcli", &tpg, "set", "attribute", "authentication=0"])
            .context("failed to disable iSCSI authentication")?;

        Ok(())
    }

    // ========================================================================
    // BACKSTORE
    // ========================================================================

    fn backstore_exists(&self, name: &str) -> Result<bool> {
        let output = self.output(["targetcli", "/backstores/block", "ls"])?;

        Ok(output.lines().any(|line| line.contains(name)))
    }

    fn create_backstore(&self, lun: &IscsiLunSpec) -> Result<()> {
        if self.backstore_exists(&lun.backstore)? {
            return Ok(());
        }

        let device = lun.block_device.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "block device path is not valid UTF-8: {}",
                lun.block_device.display()
            )
        })?;

        self.execute([
            "targetcli",
            "/backstores/block",
            "create",
            &lun.backstore,
            device,
        ])
        .with_context(|| {
            format!(
                "failed to create iSCSI backstore '{}' for '{}'",
                lun.backstore, device
            )
        })?;

        Ok(())
    }

    fn remove_backstore(&self, backstore: &str) -> Result<()> {
        if !self.backstore_exists(backstore)? {
            return Ok(());
        }

        let path = Self::backstore_path(backstore);

        self.execute(["targetcli", &path, "delete"])
            .with_context(|| format!("failed to remove iSCSI backstore '{}'", backstore))?;

        Ok(())
    }

    // ========================================================================
    // LUN
    // ========================================================================

    fn lun_exists(&self, target_iqn: &str, lun_number: u32) -> Result<bool> {
        let output = self.output(["targetcli", &Self::lun_path(target_iqn), "ls"])?;

        let expected = format!("lun{lun_number}");

        Ok(output.lines().any(|line| line.contains(&expected)))
    }

    fn create_lun(&self, target_iqn: &str, lun: &IscsiLunSpec) -> Result<()> {
        if self.lun_exists(target_iqn, lun.lun)? {
            return Ok(());
        }

        let backstore = Self::backstore_path(&lun.backstore);
        let lun_number = lun.lun.to_string();

        self.execute([
            "targetcli",
            &Self::lun_path(target_iqn),
            "create",
            &backstore,
            &lun_number,
        ])
        .with_context(|| {
            format!(
                "failed to attach backstore '{}' as LUN {}",
                lun.backstore, lun.lun
            )
        })?;

        Ok(())
    }

    // ========================================================================
    // PORTAL
    // ========================================================================

    fn portal_exists(&self, spec: &IscsiTargetSpec) -> Result<bool> {
        let output = self.output(["targetcli", &Self::portal_path(&spec.target_iqn), "ls"])?;

        let portal = format!("{}:{}", spec.portal_address, spec.portal_port);

        Ok(output.lines().any(|line| line.contains(&portal)))
    }

    fn create_portal(&self, spec: &IscsiTargetSpec) -> Result<()> {
        if self.portal_exists(spec)? {
            return Ok(());
        }

        self.execute([
            "targetcli",
            &Self::portal_path(&spec.target_iqn),
            "create",
            &spec.portal_address,
            &spec.portal_port.to_string(),
        ])
        .with_context(|| {
            format!(
                "failed to create iSCSI portal {}:{}",
                spec.portal_address, spec.portal_port
            )
        })?;

        Ok(())
    }

    // ========================================================================
    // INSPECTION
    // ========================================================================

    fn inspect_lun(&self, target_iqn: &str, lun: &IscsiLunSpec) -> Result<IscsiLunState> {
        let exists = self.lun_exists(target_iqn, lun.lun)?;

        let backstore_exists = self.backstore_exists(&lun.backstore)?;

        let block_device_matches = if backstore_exists {
            self.backstore_points_to_device(&lun.backstore, &lun.block_device)?
        } else {
            false
        };

        Ok(IscsiLunState {
            lun: lun.lun,
            backstore: lun.backstore.clone(),
            exists,
            backstore_exists,
            block_device_matches,
        })
    }

    fn backstore_points_to_device(&self, backstore: &str, device: &Path) -> Result<bool> {
        let output = self.output(["targetcli", &Self::backstore_path(backstore), "ls"])?;

        let device_string = device.to_string_lossy();

        Ok(output.contains(device_string.as_ref()))
    }

    // ========================================================================
    // SAVE
    // ========================================================================

    fn save(&self) -> Result<()> {
        self.execute(["targetcli", "saveconfig"])
            .context("failed to save targetcli configuration")
    }
}

// ============================================================================
// PROVISIONER IMPLEMENTATION
// ============================================================================

impl IscsiProvisioner for TargetCliProvisioner {
    fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()> {
        Self::validate_spec(spec)?;

        // ------------------------------------------------------------
        // 1. Create target.
        // ------------------------------------------------------------

        self.create_target_object(spec)?;

        // ------------------------------------------------------------
        // 2. Configure TPG.
        // ------------------------------------------------------------

        self.configure_tpg(spec)?;

        // ------------------------------------------------------------
        // 3. Create every requested backstore.
        // ------------------------------------------------------------

        for lun in &spec.luns {
            self.create_backstore(lun)?;
        }

        // ------------------------------------------------------------
        // 4. Attach every backstore as its requested LUN.
        // ------------------------------------------------------------

        for lun in &spec.luns {
            self.create_lun(&spec.target_iqn, lun)?;
        }

        // ------------------------------------------------------------
        // 5. Create portal.
        // ------------------------------------------------------------

        self.create_portal(spec)?;

        // ------------------------------------------------------------
        // 6. Persist LIO configuration.
        // ------------------------------------------------------------

        self.save()?;

        Ok(())
    }

    fn remove_target(&self, target_iqn: &str) -> Result<()> {
        if !self.target_exists(target_iqn)? {
            return Ok(());
        }

        self.execute(["targetcli", "/iscsi", "delete", target_iqn])
            .with_context(|| format!("failed to remove iSCSI target '{}'", target_iqn))?;

        self.save()?;

        Ok(())
    }

    fn remove_target_with_backstores(&self, target_iqn: &str, backstores: &[String]) -> Result<()> {
        // Remove the target first so all LUN references are detached.
        self.remove_target(target_iqn)?;

        // Only delete explicitly-owned backstores.
        //
        // Shared game backstores must never be included here.
        for backstore in backstores {
            if backstore.trim().is_empty() {
                continue;
            }

            self.remove_backstore(backstore).with_context(|| {
                format!(
                    "failed to remove owned backstore '{}' for target '{}'",
                    backstore, target_iqn
                )
            })?;
        }

        self.save()?;

        Ok(())
    }

    fn target_exists(&self, target_iqn: &str) -> Result<bool> {
        let output = self.output(["targetcli", "/iscsi", "ls"])?;

        Ok(output.lines().any(|line| line.contains(target_iqn)))
    }

    fn inspect_target(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
        let exists = self.target_exists(&spec.target_iqn)?;

        if !exists {
            let luns = spec
                .luns
                .iter()
                .map(|lun| IscsiLunState {
                    lun: lun.lun,
                    backstore: lun.backstore.clone(),
                    exists: false,
                    backstore_exists: false,
                    block_device_matches: false,
                })
                .collect();

            return Ok(IscsiTargetState::from_luns(
                spec.target_iqn.clone(),
                false,
                luns,
                false,
            ));
        }

        let mut luns = Vec::with_capacity(spec.luns.len());

        for lun in &spec.luns {
            luns.push(self.inspect_lun(&spec.target_iqn, lun)?);
        }

        let portal_exists = self.portal_exists(spec)?;

        Ok(IscsiTargetState::from_luns(
            spec.target_iqn.clone(),
            true,
            luns,
            portal_exists,
        ))
    }

    fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
        Self::validate_spec(spec)?;

        let state = self.inspect_target(spec)?;

        if !state.is_ready() {
            self.create_target(spec)?;
        }

        Ok(())
    }
}
