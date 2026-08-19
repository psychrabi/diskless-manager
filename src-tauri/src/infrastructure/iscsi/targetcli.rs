use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::cmd::{run_command, run_command_output_no_sudo};

use super::model::{
    IscsiLunSpec, IscsiLunState, IscsiProvisionResult, IscsiTargetSpec, IscsiTargetState,
};

/// Abstraction over the iSCSI/LIO provisioning layer.
///
/// `create_target()` remains idempotent and is used by reconciliation.
///
/// `create_target_transaction()` is used by application provisioning
/// when ownership information is required for safe rollback.
pub trait IscsiProvisioner: Send + Sync {
    /// Create or reconcile the desired target configuration.
    fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()>;

    /// Create/reconcile an iSCSI target and report resources created
    /// by this particular transaction.
    ///
    /// The default implementation preserves compatibility with other
    /// implementations of the trait. Implementations that support
    /// transactional ownership should override this method.
    fn create_target_transaction(&self, spec: &IscsiTargetSpec) -> Result<IscsiProvisionResult> {
        self.create_target(spec)?;

        Ok(IscsiProvisionResult::default())
    }

    /// Remove an iSCSI target.
    ///
    /// Backstores are deliberately preserved because a backstore may be
    /// shared or owned independently of the target.
    fn remove_target(&self, target_iqn: &str) -> Result<()>;

    /// Remove an iSCSI target and explicitly-owned backstores.
    ///
    /// Only the supplied backstores are deleted.
    fn remove_target_with_backstores(&self, target_iqn: &str, backstores: &[String]) -> Result<()>;

    /// Check whether an iSCSI target exists.
    fn target_exists(&self, target_iqn: &str) -> Result<bool>;

    /// Inspect actual target state.
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
            .context("targetcli command failed")
    }

    // ========================================================================
    // PATH HELPERS
    // ========================================================================

    fn tpg_path(target_iqn: &str) -> String {
        format!("/iscsi/{target_iqn}/tpg1")
    }

    fn lun_path(target_iqn: &str) -> String {
        format!("{}/luns", Self::tpg_path(target_iqn))
    }

    fn portal_path(target_iqn: &str) -> String {
        format!("{}/portals", Self::tpg_path(target_iqn))
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
            bail!("iSCSI target must contain at least one LUN");
        }

        if spec.portal_address.trim().is_empty() {
            bail!("iSCSI portal address cannot be empty");
        }

        if spec.portal_port == 0 {
            bail!("iSCSI portal port cannot be zero");
        }

        let mut seen_luns = std::collections::HashSet::new();
        let mut seen_backstores = std::collections::HashSet::new();

        for lun in &spec.luns {
            if !seen_luns.insert(lun.lun) {
                bail!("duplicate LUN {} in target '{}'", lun.lun, spec.target_iqn);
            }

            if lun.backstore.trim().is_empty() {
                bail!("iSCSI backstore cannot be empty for LUN {}", lun.lun);
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

    fn backstore_points_to_device(&self, backstore: &str, device: &Path) -> Result<bool> {
        let output = self.output(["targetcli", &Self::backstore_path(backstore), "ls"])?;

        let device_string = device.to_string_lossy();

        Ok(output.contains(device_string.as_ref()))
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

    fn remove_lun(&self, target_iqn: &str, lun_number: u32) -> Result<()> {
        if !self.lun_exists(target_iqn, lun_number)? {
            return Ok(());
        }

        let path = format!("{}/lun{}", Self::lun_path(target_iqn), lun_number);

        self.execute(["targetcli", &path, "delete"])
            .with_context(|| {
                format!(
                    "failed to remove LUN {} from target '{}'",
                    lun_number, target_iqn
                )
            })
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

    fn remove_portal(&self, spec: &IscsiTargetSpec) -> Result<()> {
        if !self.portal_exists(spec)? {
            return Ok(());
        }

        self.execute([
            "targetcli",
            &Self::portal_path(&spec.target_iqn),
            "delete",
            &spec.portal_address,
            &spec.portal_port.to_string(),
        ])
        .with_context(|| {
            format!(
                "failed to remove iSCSI portal {}:{}",
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

    // ========================================================================
    // SAVE
    // ========================================================================

    fn save(&self) -> Result<()> {
        self.execute(["targetcli", "saveconfig"])
            .context("failed to save targetcli configuration")
    }

    // ========================================================================
    // TRANSACTION ROLLBACK
    // ========================================================================

    fn rollback_transaction(
        &self,
        spec: &IscsiTargetSpec,
        created: &IscsiProvisionResult,
    ) -> Result<()> {
        let mut rollback_error: Option<anyhow::Error> = None;

        // If the target itself was created by this transaction,
        // deleting it removes its LUN associations and portal.
        //
        // Existing targets must never be deleted merely because a
        // provisioning transaction failed.
        if created.target_created {
            if let Err(error) = self.remove_target(&spec.target_iqn) {
                rollback_error = Some(error);
            }
        } else {
            // Existing target: remove only LUNs created by this
            // transaction.
            for lun_number in created.luns_created.iter().rev() {
                if let Err(error) = self.remove_lun(&spec.target_iqn, *lun_number) {
                    tracing::warn!(
                        target_iqn = %spec.target_iqn,
                        lun = *lun_number,
                        error = %error,
                        "failed to rollback iSCSI LUN"
                    );

                    if rollback_error.is_none() {
                        rollback_error = Some(error);
                    }
                }
            }

            // Only remove a portal if this transaction created it.
            if created.portal_created {
                if let Err(error) = self.remove_portal(spec) {
                    tracing::warn!(
                        target_iqn = %spec.target_iqn,
                        error = %error,
                        "failed to rollback iSCSI portal"
                    );

                    if rollback_error.is_none() {
                        rollback_error = Some(error);
                    }
                }
            }
        }

        // Only transaction-created backstores are owned by the
        // transaction.
        //
        // Shared game-disk backstores are therefore never removed.
        for backstore in created.backstores_created.iter().rev() {
            if let Err(error) = self.remove_backstore(backstore) {
                tracing::warn!(
                    target_iqn = %spec.target_iqn,
                    backstore = %backstore,
                    error = %error,
                    "failed to rollback iSCSI backstore"
                );

                if rollback_error.is_none() {
                    rollback_error = Some(error);
                }
            }
        }

        if let Err(error) = self.save() {
            if rollback_error.is_none() {
                rollback_error = Some(error);
            }
        }

        match rollback_error {
            Some(error) => Err(error).context("iSCSI transaction rollback failed"),

            None => Ok(()),
        }
    }
}

// ============================================================================
// PROVISIONER IMPLEMENTATION
// ============================================================================

impl IscsiProvisioner for TargetCliProvisioner {
    fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()> {
        self.create_target_transaction(spec).map(|_| ())
    }

    fn create_target_transaction(&self, spec: &IscsiTargetSpec) -> Result<IscsiProvisionResult> {
        Self::validate_spec(spec)?;

        let mut created = IscsiProvisionResult::new();

        let operation = (|| -> Result<()> {
            // ------------------------------------------------------------
            // 1. Target
            // ------------------------------------------------------------

            let target_existed = self.target_exists(&spec.target_iqn)?;

            self.create_target_object(spec)?;

            if !target_existed {
                created.target_created = true;
            }

            // ------------------------------------------------------------
            // 2. Configure TPG
            // ------------------------------------------------------------

            self.configure_tpg(spec)?;

            // ------------------------------------------------------------
            // 3. Backstores
            // ------------------------------------------------------------

            for lun in &spec.luns {
                let existed = self.backstore_exists(&lun.backstore)?;

                if existed {
                    // Never silently reuse a backstore that points
                    // to another block device.
                    if !self.backstore_points_to_device(&lun.backstore, &lun.block_device)? {
                        bail!(
                            "existing iSCSI backstore '{}' points to a different block device",
                            lun.backstore
                        );
                    }
                } else {
                    self.create_backstore(lun)?;

                    created.backstores_created.push(lun.backstore.clone());
                }
            }

            // ------------------------------------------------------------
            // 4. LUNs
            // ------------------------------------------------------------

            for lun in &spec.luns {
                let existed = self.lun_exists(&spec.target_iqn, lun.lun)?;

                if !existed {
                    self.create_lun(&spec.target_iqn, lun)?;

                    created.luns_created.push(lun.lun);
                }
            }

            // ------------------------------------------------------------
            // 5. Portal
            // ------------------------------------------------------------

            let portal_existed = self.portal_exists(spec)?;

            self.create_portal(spec)?;

            if !portal_existed {
                created.portal_created = true;
            }

            // ------------------------------------------------------------
            // 6. Persist
            // ------------------------------------------------------------

            self.save()?;

            Ok(())
        })();

        match operation {
            Ok(()) => Ok(created),

            Err(error) => {
                if let Err(rollback_error) = self.rollback_transaction(spec, &created) {
                    tracing::error!(
                        target_iqn = %spec.target_iqn,
                        error = %rollback_error,
                        "iSCSI transaction rollback failed"
                    );

                    return Err(error).context(format!(
                        "iSCSI provisioning failed and rollback also failed: {}",
                        rollback_error
                    ));
                }

                Err(error)
            }
        }
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
        // Remove the target first so all LUN references are
        // detached.
        self.remove_target(target_iqn)?;

        // Only delete explicitly-owned backstores.
        //
        // Shared game backstores must never be supplied here.
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
