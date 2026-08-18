use anyhow::Result;

use super::{
    model::{IscsiTargetSpec, IscsiTargetState},
    IscsiProvisioner,
};

/// Reconciles desired iSCSI state with the actual LIO configuration.
pub struct IscsiReconciler<P>
where
    P: IscsiProvisioner,
{
    provisioner: P,
}

impl<P> IscsiReconciler<P>
where
    P: IscsiProvisioner,
{
    pub fn new(provisioner: P) -> Self {
        Self { provisioner }
    }

    pub fn inspect(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
        self.provisioner.inspect_target(spec)
    }

    pub fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
        self.provisioner.reconcile(spec)
    }

    pub fn remove(&self, target_iqn: &str) -> Result<()> {
        self.provisioner.remove_target(target_iqn)
    }
}
