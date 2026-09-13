use anyhow::{bail, Context, Result};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::infrastructure::command::{run_command, run_command_input_redacted, run_command_output};

use super::model::{
    ChapCredentials, IscsiLunSpec, IscsiLunState, IscsiProvisionResult, IscsiTargetSpec,
    IscsiTargetState,
};

fn tree_has_node(output: &str, name: &str) -> bool {
    output
        .lines()
        .any(|line| line.strip_prefix("  o- ").is_some_and(|line| line.split_whitespace().next() == Some(name)))
}

fn parse_luns(output: &str) -> Vec<(u32, &str)> {
    output
        .lines()
        .filter_map(|line| {
            let mut tokens = line.split_whitespace();
            while tokens.next()? != "o-" {}
            let lun = tokens.next()?.strip_prefix("lun")?.parse().ok()?;
            let backstore = tokens
                .next()?
                .strip_prefix('[')?
                .rsplit('/')
                .next()?;
            Some((lun, backstore))
        })
        .collect()
}

fn parse_backstore<'a>(output: &'a str, name: &str) -> Option<(&'a str, bool)> {
    output.lines().find_map(|line| {
        let mut tokens = line.split_whitespace();
        while tokens.next()? != "o-" {}
        if tokens.next()? != name {
            return None;
        }
        let device = tokens.next()?.strip_prefix('[')?;
        Some((device, line.contains(") ro ")))
    })
}

static TARGETCLI_MUTATION: Mutex<()> = Mutex::new(());

fn lock_mutations() -> Result<MutexGuard<'static, ()>> {
    TARGETCLI_MUTATION
        .lock()
        .map_err(|_| anyhow::anyhow!("targetcli mutation lock poisoned"))
}

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

    /// Remove a target's LUN references and explicitly-owned backstores.
    ///
    /// The target itself is preserved. This is required when a target may
    /// contain resources owned by another client or a shared disk.
    fn remove_target_with_backstores(&self, target_iqn: &str, backstores: &[String]) -> Result<()>;

    /// Check whether an iSCSI target exists.
    fn target_exists(&self, target_iqn: &str) -> Result<bool>;

    /// Enforce (or clear with `None`) one-way CHAP on a live target.
    ///
    /// Applies immediately to new logins; existing sessions are
    /// unaffected. Used by secret rotation to avoid a full rebuild.
    fn set_chap_auth(
        &self,
        target_iqn: &str,
        chap: Option<&ChapCredentials>,
    ) -> Result<()>;

    /// List every LUN currently attached to a target.
    ///
    /// Used to find stale attachments (e.g. deselected game disks) that
    /// are not part of any desired spec.
    fn list_target_luns(&self, target_iqn: &str) -> Result<Vec<IscsiLunState>>;

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
        run_command_output(args)
            .map_err(anyhow::Error::from)
            .context("targetcli command failed")
    }

    fn set_chap(&self, tpg: &str, chap: &ChapCredentials) -> Result<()> {
        let input = format!(
            "{tpg} set auth userid={} password={}\nexit\n",
            chap.username, chap.password
        );
        run_command_input_redacted(["targetcli"], &input)
            .map_err(anyhow::Error::from)
            .context("targetcli CHAP command failed")
    }

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

    fn wait_for_block_device(path: &Path) -> bool {
        const DEVICE_TIMEOUT: Duration = Duration::from_secs(5);
        const POLL_INTERVAL: Duration = Duration::from_millis(50);

        let deadline = Instant::now() + DEVICE_TIMEOUT;
        loop {
            if path.exists() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

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

            if !Self::wait_for_block_device(&lun.block_device) {
                bail!(
                    "iSCSI block device does not exist: {}",
                    lun.block_device.display()
                );
            }
        }

        Ok(())
    }

    fn create_target_object(&self, spec: &IscsiTargetSpec, exists: bool) -> Result<()> {
        if exists {
            return Ok(());
        }

        self.execute(["targetcli", "/iscsi", "create", &spec.target_iqn])
            .with_context(|| format!("failed to create iSCSI target '{}'", spec.target_iqn))
    }

    fn delete_target_object(&self, target_iqn: &str) -> Result<()> {
        self.execute(["targetcli", "/iscsi", "delete", target_iqn])
            .with_context(|| format!("failed to remove iSCSI target '{target_iqn}'"))
    }

    fn configure_tpg(&self, spec: &IscsiTargetSpec) -> Result<()> {
        let tpg = Self::tpg_path(&spec.target_iqn);

        let mut attributes = vec![
            "targetcli",
            tpg.as_str(),
            "set",
            "attribute",
            "generate_node_acls=1",
            "cache_dynamic_acls=1",
            "demo_mode_write_protect=0",
        ];
        if spec.chap.is_none() {
            attributes.push("authentication=0");
        }
        self.execute(attributes)
        .context("failed to configure iSCSI target attributes")?;

        // One-way CHAP: the target authenticates the initiator. Secrets
        // are server-generated alphanumeric strings, safe to interpolate.
        if let Some(chap) = spec.chap.as_ref() {
            self.execute(["targetcli", &tpg, "set", "attribute", "authentication=1"])
                .context("failed to enable iSCSI authentication")?;
            self.save().context("failed to persist fail-closed authentication")?;
            self.set_chap(&tpg, chap)
            .with_context(|| {
                format!(
                    "failed to set iSCSI CHAP credentials for user '{}'",
                    chap.username
                )
            })?;

        }

        Ok(())
    }

    fn backstore_exists(&self, name: &str) -> Result<bool> {
        let output = self.output(["targetcli", "/backstores/block", "ls"])?;
        Ok(tree_has_node(&output, name))
    }

    fn create_backstore(&self, lun: &IscsiLunSpec, exists: bool) -> Result<()> {
        if !exists {
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
                &format!("readonly={}", lun.readonly),
            ])
            .with_context(|| {
                format!(
                    "failed to create iSCSI backstore '{}' for '{}'",
                    lun.backstore, device
                )
            })?;
        }

        Ok(())
    }

    fn remove_backstore(&self, backstore: &str) -> Result<()> {
        if !self.backstore_exists(backstore)? {
            return Ok(());
        }

        self.execute(["targetcli", "/backstores/block", "delete", backstore])
            .with_context(|| format!("failed to remove iSCSI backstore '{}'", backstore))
    }

    fn backstore_matches(&self, lun: &IscsiLunSpec) -> Result<bool> {
        let backstore = &lun.backstore;
        let output = self.output(["targetcli", &Self::backstore_path(backstore), "ls"])?;
        Ok(parse_backstore(&output, backstore) == lun.block_device.to_str().map(|p| (p, lun.readonly)))
    }

    fn lun_exists(&self, target_iqn: &str, lun_number: u32) -> Result<bool> {
        let output = self.output(["targetcli", &Self::lun_path(target_iqn), "ls"])?;
        Ok(parse_luns(&output).iter().any(|(lun, _)| *lun == lun_number))
    }

    fn create_lun(&self, target_iqn: &str, lun: &IscsiLunSpec, exists: bool) -> Result<()> {
        if exists {
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
        })
    }

    fn remove_lun(&self, target_iqn: &str, lun_number: u32) -> Result<()> {
        if !self.lun_exists(target_iqn, lun_number)? {
            return Ok(());
        }

        let lun_name = format!("lun{lun_number}");

        self.execute([
            "targetcli",
            &Self::lun_path(target_iqn),
            "delete",
            &lun_name,
        ])
        .with_context(|| {
            format!(
                "failed to remove LUN {} from target '{}'",
                lun_number, target_iqn
            )
        })
    }

    fn portal_exists(&self, spec: &IscsiTargetSpec) -> Result<bool> {
        let output = self.output(["targetcli", &Self::portal_path(&spec.target_iqn), "ls"])?;

        let configured_portal = format!("{}:{}", spec.portal_address, spec.portal_port);

        let default_ipv6_portal = format!("[::0]:{}", spec.portal_port);

        let wildcard_ipv6_portal = format!("[::]:{}", spec.portal_port);

        let wildcard_ipv4_portal = format!("0.0.0.0:{}", spec.portal_port);

        Ok(output.lines().any(|line| {
            line.contains(&configured_portal)
                || line.contains(&default_ipv6_portal)
                || line.contains(&wildcard_ipv6_portal)
                || line.contains(&wildcard_ipv4_portal)
        }))
    }

    fn create_portal(&self, spec: &IscsiTargetSpec, exists: bool) -> Result<()> {
        // targetcli-fb may automatically create [::0]:3260 when
        // auto_add_default_portal=true. That wildcard portal already
        // provides the listener required by iSCSI clients, so creating
        // 0.0.0.0:3260 again is unnecessary and fails on targetcli-fb.
        if exists {
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
        })
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
        })
    }

    fn inspect_lun(&self, target_iqn: &str, lun: &IscsiLunSpec) -> Result<IscsiLunState> {
        let output = self.output(["targetcli", &Self::lun_path(target_iqn), "ls"])?;
        let mapped_backstore = parse_luns(&output)
            .into_iter()
            .find_map(|(number, backstore)| (number == lun.lun).then_some(backstore));
        let exists = mapped_backstore.is_some();
        let backstore_exists = self.backstore_exists(&lun.backstore)?;
        let block_device_matches = if backstore_exists && mapped_backstore == Some(&lun.backstore) {
            self.backstore_matches(lun)?
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

    fn save(&self) -> Result<()> {
        self.execute(["targetcli", "saveconfig"])
            .context("failed to save targetcli configuration")
    }

    fn rollback_transaction(
        &self,
        spec: &IscsiTargetSpec,
        created: &IscsiProvisionResult,
    ) -> Result<()> {
        let mut rollback_error: Option<anyhow::Error> = None;

        if created.target_created {
            if let Err(error) = self.delete_target_object(&spec.target_iqn) {
                rollback_error = Some(error);
            }
        } else {
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

impl IscsiProvisioner for TargetCliProvisioner {
    fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()> {
        self.create_target_transaction(spec).map(|_| ())
    }

    fn create_target_transaction(&self, spec: &IscsiTargetSpec) -> Result<IscsiProvisionResult> {
        let _guard = lock_mutations()?;
        Self::validate_spec(spec)?;

        let mut created = IscsiProvisionResult::new();

        let operation = (|| -> Result<()> {
            let target_existed = self.target_exists(&spec.target_iqn)?;

            self.create_target_object(spec, target_existed)?;

            if !target_existed {
                created.target_created = true;
            }

            self.configure_tpg(spec)?;

            for lun in &spec.luns {
                let existed = self.backstore_exists(&lun.backstore)?;

                if existed
                    && !self.backstore_matches(lun)?
                {
                    bail!(
                        "existing iSCSI backstore '{}' has a different device or readonly state",
                        lun.backstore
                    );
                }

                self.create_backstore(lun, existed)?;
                if !existed {
                    created.backstores_created.push(lun.backstore.clone());
                }
            }

            for lun in &spec.luns {
                let output = self.output([
                    "targetcli",
                    &Self::lun_path(&spec.target_iqn),
                    "ls",
                ])?;
                let existing_backstore = parse_luns(&output)
                    .into_iter()
                    .find_map(|(number, backstore)| (number == lun.lun).then_some(backstore));

                if let Some(backstore) = existing_backstore {
                    if backstore != lun.backstore {
                        bail!(
                            "existing LUN {} points to backstore '{}', expected '{}'",
                            lun.lun,
                            backstore,
                            lun.backstore
                        );
                    }
                }

                if existing_backstore.is_none() {
                    self.create_lun(&spec.target_iqn, lun, false)?;
                    created.luns_created.push(lun.lun);
                }
            }

            let portal_existed = self.portal_exists(spec)?;
            self.create_portal(spec, portal_existed)?;

            if !portal_existed {
                created.portal_created = true;
            }

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
        let _guard = lock_mutations()?;
        if !self.target_exists(target_iqn)? {
            return Ok(());
        }

        self.delete_target_object(target_iqn)?;

        self.save()?;
        Ok(())
    }

    fn remove_target_with_backstores(&self, target_iqn: &str, backstores: &[String]) -> Result<()> {
        let _guard = lock_mutations()?;
        if !self.target_exists(target_iqn)? {
            for backstore in backstores {
                if !backstore.trim().is_empty() {
                    self.remove_backstore(backstore).with_context(|| {
                        format!(
                            "failed to remove owned backstore '{}' for target '{}'",
                            backstore, target_iqn
                        )
                    })?;
                }
            }

            return self.save();
        }

        // Detach only the LUNs that point to backstores owned by this
        // caller. Leave all unrelated LUNs on the target intact.
        let mut owned_lun_numbers = Vec::new();
        let lun_output = self.output(["targetcli", &Self::lun_path(target_iqn), "ls"])?;

        for (number, backstore) in parse_luns(&lun_output) {
            if backstores.iter().any(|owned| owned == backstore) {
                owned_lun_numbers.push(number);
            }
        }

        owned_lun_numbers.sort_unstable();
        owned_lun_numbers.dedup();

        for lun_number in owned_lun_numbers.into_iter().rev() {
            self.remove_lun(target_iqn, lun_number)?;
        }

        for backstore in backstores.iter().filter(|item| !item.trim().is_empty()) {
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

    fn set_chap_auth(
        &self,
        target_iqn: &str,
        chap: Option<&ChapCredentials>,
    ) -> Result<()> {
        let _guard = lock_mutations()?;
        let tpg = Self::tpg_path(target_iqn);
        match chap {
            Some(chap) => {
                self.execute(["targetcli", &tpg, "set", "attribute", "authentication=1"])
                    .context("failed to enable iSCSI authentication")?;
                self.save().context("failed to persist fail-closed authentication")?;
                self.set_chap(&tpg, chap)
                .with_context(|| {
                    format!(
                        "failed to set iSCSI CHAP credentials for user '{}'",
                        chap.username
                    )
                })?;
            }
            None => {
                self.execute(["targetcli", &tpg, "set", "attribute", "authentication=0"])
                    .context("failed to disable iSCSI authentication")?;
            }
        }
        self.save()
    }

    fn list_target_luns(&self, target_iqn: &str) -> Result<Vec<IscsiLunState>> {
        if !self.target_exists(target_iqn)? {
            return Ok(Vec::new());
        }
        let lun_output = self.output(["targetcli", &Self::lun_path(target_iqn), "ls"])?;
        // Lines look like `lun1 [block_pc001 (/dev/zvol/diskless/PC001-disk)]`.
        // Unparseable lines are skipped: listing must never fail a prune.
        let mut luns = Vec::new();
        for (number, backstore) in parse_luns(&lun_output) {
            luns.push(IscsiLunState {
                lun: number,
                backstore: backstore.to_string(),
                exists: true,
                backstore_exists: self.backstore_exists(backstore).unwrap_or(false),
                block_device_matches: false,
            });
        }
        Ok(luns)
    }

    fn target_exists(&self, target_iqn: &str) -> Result<bool> {
        let output = self.output(["targetcli", "/iscsi", "ls"])?;
        Ok(tree_has_node(&output, target_iqn))
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

#[cfg(test)]
mod tests {
    use super::{parse_backstore, parse_luns, tree_has_node, TargetCliProvisioner};
    use crate::infrastructure::iscsi::IscsiTargetSpec;
    use std::{fs, thread, time::Duration};

    #[test]
    fn validation_waits_for_a_delayed_zvol_device_node() {
        let path =
            std::env::temp_dir().join(format!("diskless-delayed-zvol-{}", uuid::Uuid::new_v4()));
        let delayed_path = path.clone();
        let creator = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            fs::write(delayed_path, []).unwrap();
        });

        let spec = IscsiTargetSpec::new(
            "iqn.2024-01.com.diskless:client.pc001",
            "block_pc001",
            &path,
            0,
        );
        let result = TargetCliProvisioner::validate_spec(&spec);

        creator.join().unwrap();
        let _ = fs::remove_file(path);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn tree_lookup_matches_complete_node_names() {
        let output = "  o- block_pc001 [...]\n  o- block_pc0010 [...]";

        assert!(tree_has_node(output, "block_pc001"));
        assert!(!tree_has_node(output, "block_pc00"));
    }

    #[test]
    fn lun_parser_does_not_confuse_prefixes() {
        let output = "  o- lun1 [block/game_1 (/dev/zvol/game-1)]\n  o- lun10 [block/game_10 (/dev/zvol/game-10)]";
        let luns = parse_luns(output);

        assert_eq!(luns, vec![(1, "game_1"), (10, "game_10")]);
    }

    #[test]
    fn backstore_device_parser_matches_complete_path() {
        let output = "o- block_pc001 [/dev/zvol/diskless/PC001-disk (50.0GiB) write-thru activated]";

        assert_eq!(
            parse_backstore(output, "block_pc001"),
            Some(("/dev/zvol/diskless/PC001-disk", false))
        );

        let readonly = "o- games [/dev/zvol/games (50.0GiB) ro write-thru activated]";
        assert_eq!(parse_backstore(readonly, "games"), Some(("/dev/zvol/games", true)));
    }
}
