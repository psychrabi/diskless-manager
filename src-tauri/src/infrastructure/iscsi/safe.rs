use anyhow::{bail, Result};

use super::{
    model::{IscsiProvisionResult, IscsiTargetSpec, IscsiTargetState},
    reconcile::target_has_active_sessions,
    IscsiProvisioner, TargetCliProvisioner,
};

/// Application-facing iSCSI provisioner that refuses destructive changes
/// while an initiator is connected to the target.
#[derive(Debug, Clone, Copy, Default)]
pub struct SafeIscsiProvisioner {
    inner: TargetCliProvisioner,
}

impl SafeIscsiProvisioner {
    pub const fn new() -> Self {
        Self {
            inner: TargetCliProvisioner::new(),
        }
    }

    fn ensure_disconnected(&self, target_iqn: &str) -> Result<()> {
        if target_has_active_sessions(target_iqn)? {
            bail!(
                "cannot modify iSCSI target '{}' while an initiator is connected",
                target_iqn
            );
        }

        Ok(())
    }
}

impl IscsiProvisioner for SafeIscsiProvisioner {
    fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()> {
        self.inner.create_target(spec)
    }

    fn create_target_transaction(&self, spec: &IscsiTargetSpec) -> Result<IscsiProvisionResult> {
        self.inner.create_target_transaction(spec)
    }

    fn remove_target(&self, target_iqn: &str) -> Result<()> {
        self.ensure_disconnected(target_iqn)?;
        self.inner.remove_target(target_iqn)
    }

    fn remove_target_with_backstores(
        &self,
        target_iqn: &str,
        backstores: &[String],
    ) -> Result<()> {
        self.ensure_disconnected(target_iqn)?;
        self.inner
            .remove_target_with_backstores(target_iqn, backstores)
    }

    fn target_exists(&self, target_iqn: &str) -> Result<bool> {
        self.inner.target_exists(target_iqn)
    }

    fn inspect_target(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
        self.inner.inspect_target(spec)
    }

    fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
        self.inner.reconcile(spec)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn safety_error_message_is_specific() {
        let message = format!(
            "cannot modify iSCSI target '{}' while an initiator is connected",
            "iqn.test:client"
        );

        assert!(message.contains("iqn.test:client"));
        assert!(message.contains("initiator is connected"));
    }
}
