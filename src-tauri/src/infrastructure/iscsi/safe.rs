use anyhow::{bail, Result};
use std::sync::Arc;

use super::{
    configfs::ConfigfsProvisioner,
    model::{
        ChapCredentials, IscsiLunState, IscsiProvisionResult, IscsiTargetSpec, IscsiTargetState,
    },
    reconcile::target_has_active_sessions,
    targetcli::TargetCliProvisioner,
    IscsiProvisioner,
};
use crate::core::config::IscsiBackend;

/// Application-facing iSCSI provisioner that refuses destructive changes
/// while an initiator is connected to the target.
///
/// The configured backend is selected at startup and remains fixed until
/// restart. Native availability is probed before use; an operation failure
/// never triggers a targetcli retry because native state may have changed.
#[derive(Clone)]
pub struct SafeIscsiProvisioner {
    inner: Arc<dyn IscsiProvisioner>,
    backend: IscsiBackend,
}

impl SafeIscsiProvisioner {
    pub fn new() -> Self {
        Self::with_backend(IscsiBackend::default())
    }

    pub fn with_backend(backend: IscsiBackend) -> Self {
        let backend = select_backend(backend, ConfigfsProvisioner::probe);
        let inner: Arc<dyn IscsiProvisioner> = match backend {
            IscsiBackend::Targetcli => Arc::new(TargetCliProvisioner::new()),
            IscsiBackend::InternalConfigfs => Arc::new(ConfigfsProvisioner::new()),
        };
        Self { inner, backend }
    }

    /// The backend selected after the one-time pre-mutation capability check.
    pub fn backend(&self) -> IscsiBackend {
        self.backend
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

fn select_backend(
    configured: IscsiBackend,
    native_preflight: impl FnOnce() -> anyhow::Result<()>,
) -> IscsiBackend {
    match configured {
        IscsiBackend::Targetcli => IscsiBackend::Targetcli,
        IscsiBackend::InternalConfigfs => match native_preflight() {
            Ok(()) => IscsiBackend::InternalConfigfs,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Native iSCSI preflight failed; selecting targetcli before any mutation"
                );
                IscsiBackend::Targetcli
            }
        },
    }
}

impl Default for SafeIscsiProvisioner {
    fn default() -> Self {
        Self::new()
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

    fn remove_target_with_backstores(&self, target_iqn: &str, backstores: &[String]) -> Result<()> {
        self.ensure_disconnected(target_iqn)?;
        self.inner
            .remove_target_with_backstores(target_iqn, backstores)
    }

    fn target_exists(&self, target_iqn: &str) -> Result<bool> {
        self.inner.target_exists(target_iqn)
    }

    fn set_chap_auth(&self, target_iqn: &str, chap: Option<&ChapCredentials>) -> Result<()> {
        self.inner.set_chap_auth(target_iqn, chap)
    }

    fn list_target_luns(&self, target_iqn: &str) -> Result<Vec<IscsiLunState>> {
        self.inner.list_target_luns(target_iqn)
    }

    fn list_target_backstores(&self, target_iqn: &str) -> Result<Vec<String>> {
        self.inner.list_target_backstores(target_iqn)
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
    use super::select_backend;
    use crate::core::config::IscsiBackend;

    #[test]
    fn native_backend_falls_back_to_targetcli_only_when_preflight_fails() {
        assert_eq!(
            select_backend(IscsiBackend::InternalConfigfs, || {
                Err(anyhow::anyhow!("native command unavailable"))
            }),
            IscsiBackend::Targetcli
        );
        assert_eq!(
            select_backend(IscsiBackend::InternalConfigfs, || Ok(())),
            IscsiBackend::InternalConfigfs
        );
    }

    #[test]
    fn targetcli_selection_does_not_probe_native_backend() {
        assert_eq!(
            select_backend(IscsiBackend::Targetcli, || {
                panic!("unexpected native probe")
            }),
            IscsiBackend::Targetcli
        );
    }

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
