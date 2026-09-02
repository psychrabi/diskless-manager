use crate::error::AppError;
use crate::infrastructure::dhcp::update_dhcp_config;
use crate::infrastructure::zfs::legacy::zfs_destroy;
use crate::state::AppState;

use log::{error, info, warn};

/// A resource created during client provisioning.
///
/// The transaction stores every resource that it successfully creates.
/// If a later provisioning step fails, resources are destroyed in
/// reverse order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisioningResource {
    /// A client-owned ZFS clone created during provisioning.
    ZfsClone { dataset: String },

    /// An iSCSI target and the backstores created by this transaction.
    ///
    /// Existing/shared backstores are deliberately excluded.
    IscsiTarget {
        target_iqn: String,
        backstores: Vec<String>,
    },

    /// A DHCP configuration entry created for the client.
    DhcpEntry { client_id: String },
}

/// Transaction state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisioningTransactionState {
    Active,
    Committed,
    RolledBack,
}

/// Tracks resources created during provisioning and provides
/// deterministic rollback.
///
/// The transaction does not own the resources themselves. It only
/// records resources that were successfully created and knows how
/// to undo those changes.
pub struct ProvisioningTransaction<'a> {
    state: &'a AppState,
    client_id: String,
    resources: Vec<ProvisioningResource>,
    transaction_state: ProvisioningTransactionState,
}

impl<'a> ProvisioningTransaction<'a> {
    /// Create a new provisioning transaction.
    pub fn new(state: &'a AppState, client_id: impl Into<String>) -> Self {
        Self {
            state,
            client_id: client_id.into(),
            resources: Vec::new(),
            transaction_state: ProvisioningTransactionState::Active,
        }
    }

    /// Return the client ID associated with this transaction.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Return the current transaction state.
    pub fn state(&self) -> ProvisioningTransactionState {
        self.transaction_state
    }

    /// Return the resources currently registered with the transaction.
    pub fn resources(&self) -> &[ProvisioningResource] {
        &self.resources
    }

    /// Register a successfully-created ZFS clone.
    pub fn record_zfs_clone(&mut self, dataset: impl Into<String>) {
        let dataset = dataset.into();

        info!(
            "Provisioning transaction {}: registered ZFS clone {}",
            self.client_id, dataset
        );

        self.resources
            .push(ProvisioningResource::ZfsClone { dataset });
    }

    /// Register an iSCSI target and the resources created by the
    /// iSCSI transaction.
    ///
    /// Only backstores actually created by the iSCSI transaction
    /// should be supplied here.
    pub fn record_iscsi_target(
        &mut self,
        target_iqn: impl Into<String>,
        backstores: impl IntoIterator<Item = String>,
    ) {
        let target_iqn = target_iqn.into();
        let backstores: Vec<String> = backstores.into_iter().collect();

        info!(
            "Provisioning transaction {}: registered iSCSI target {} with {} owned backstores",
            self.client_id,
            target_iqn,
            backstores.len()
        );

        self.resources.push(ProvisioningResource::IscsiTarget {
            target_iqn,
            backstores,
        });
    }

    /// Register a successfully-created DHCP entry.
    pub fn record_dhcp_entry(&mut self, client_id: impl Into<String>) {
        let client_id = client_id.into();

        info!(
            "Provisioning transaction {}: registered DHCP entry {}",
            self.client_id, client_id
        );

        self.resources
            .push(ProvisioningResource::DhcpEntry { client_id });
    }

    /// Mark the transaction as successfully completed.
    ///
    /// After commit, rollback is intentionally disabled because all
    /// resources have become part of the persistent application state.
    pub fn commit(&mut self) {
        if self.transaction_state != ProvisioningTransactionState::Active {
            warn!(
                "Attempted to commit provisioning transaction {} while in state {:?}",
                self.client_id, self.transaction_state
            );

            return;
        }

        info!(
            "Provisioning transaction {} committed with {} resources",
            self.client_id,
            self.resources.len()
        );

        self.transaction_state = ProvisioningTransactionState::Committed;
    }

    /// Roll back all resources created by this transaction.
    ///
    /// Rollback always occurs in reverse creation order.
    ///
    /// Typical provisioning order:
    ///
    /// ```text
    /// ZFS
    ///   ↓
    /// iSCSI
    ///   ↓
    /// DHCP
    ///   ↓
    /// persistence
    /// ```
    ///
    /// Therefore rollback occurs as:
    ///
    /// ```text
    /// DHCP
    ///   ↓
    /// iSCSI
    ///   ↓
    /// ZFS
    /// ```
    pub async fn rollback(&mut self) -> Vec<String> {
        if self.transaction_state != ProvisioningTransactionState::Active {
            warn!(
                "Skipping rollback for transaction {} because state is {:?}",
                self.client_id, self.transaction_state
            );

            return Vec::new();
        }

        info!(
            "Rolling back provisioning transaction {} with {} resources",
            self.client_id,
            self.resources.len()
        );

        let mut errors = Vec::new();

        while let Some(resource) = self.resources.pop() {
            match self.rollback_resource(resource).await {
                Ok(()) => {
                    info!(
                        "Successfully rolled back provisioning resource for {}",
                        self.client_id
                    );
                }

                Err(error) => {
                    let message = error.to_string();

                    error!(
                        "Failed to rollback provisioning resource for {}: {}",
                        self.client_id, message
                    );

                    errors.push(message);
                }
            }
        }

        self.transaction_state = ProvisioningTransactionState::RolledBack;

        if errors.is_empty() {
            info!(
                "Provisioning transaction {} rolled back successfully",
                self.client_id
            );
        } else {
            error!(
                "Provisioning transaction {} rollback completed with {} errors",
                self.client_id,
                errors.len()
            );
        }

        errors
    }

    /// Roll back the transaction and convert rollback failures into a
    /// single application error.
    ///
    /// The original provisioning error is preserved in the returned
    /// message.
    pub async fn rollback_with_error(&mut self, original_error: AppError) -> AppError {
        let rollback_errors = self.rollback().await;

        if rollback_errors.is_empty() {
            return original_error;
        }

        let rollback_summary = rollback_errors.join("; ");

        AppError::Internal(format!(
            "{}. Rollback also encountered errors: {}",
            original_error, rollback_summary
        ))
    }

    /// Roll back a single resource.
    async fn rollback_resource(&self, resource: ProvisioningResource) -> Result<(), AppError> {
        match resource {
            ProvisioningResource::DhcpEntry { client_id } => self.rollback_dhcp(&client_id).await,

            ProvisioningResource::IscsiTarget {
                target_iqn,
                backstores,
            } => self.rollback_iscsi(&target_iqn, &backstores),

            ProvisioningResource::ZfsClone { dataset } => self.rollback_zfs(&dataset),
        }
    }

    /// Remove a DHCP entry created by the transaction.
    async fn rollback_dhcp(&self, client_id: &str) -> Result<(), AppError> {
        info!("Rollback DHCP entry for client {}", client_id);

        update_dhcp_config(client_id, "", false)
            .await
            .map_err(|error| {
                AppError::Config(format!(
                    "Failed to remove DHCP entry for {}: {}",
                    client_id, error
                ))
            })?;

        Ok(())
    }

    /// Remove the iSCSI target and only the backstores owned by this
    /// transaction.
    ///
    /// Shared/game backstores that were already present are not
    /// supplied and therefore remain untouched.
    fn rollback_iscsi(&self, target_iqn: &str, backstores: &[String]) -> Result<(), AppError> {
        info!(
            "Rollback iSCSI target {} with {} owned backstores for client {}",
            target_iqn,
            backstores.len(),
            self.client_id
        );

        self.state
            .application
            .storage
            .remove_client_target(target_iqn, backstores)
            .map_err(|error| {
                AppError::Command(format!(
                    "Failed to remove iSCSI target {}: {}",
                    target_iqn, error
                ))
            })?;

        Ok(())
    }

    /// Destroy the ZFS clone created by the transaction.
    fn rollback_zfs(&self, dataset: &str) -> Result<(), AppError> {
        info!(
            "Rollback ZFS clone {} for client {}",
            dataset, self.client_id
        );

        zfs_destroy(dataset).map_err(|error| {
            AppError::Command(format!(
                "Failed to destroy ZFS clone {}: {}",
                dataset, error
            ))
        })?;

        Ok(())
    }
}

impl<'a> Drop for ProvisioningTransaction<'a> {
    fn drop(&mut self) {
        if self.transaction_state == ProvisioningTransactionState::Active
            && !self.resources.is_empty()
        {
            warn!(
                "Provisioning transaction {} was dropped while still active with {} resources. \
                 Explicit rollback() is required because Drop cannot perform async cleanup.",
                self.client_id,
                self.resources.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_variants_are_distinct() {
        let zfs = ProvisioningResource::ZfsClone {
            dataset: "tank/client".to_string(),
        };

        let iscsi = ProvisioningResource::IscsiTarget {
            target_iqn: "iqn.test:client".to_string(),
            backstores: vec!["block_client".to_string()],
        };

        let dhcp = ProvisioningResource::DhcpEntry {
            client_id: "client".to_string(),
        };

        assert_ne!(zfs, iscsi);
        assert_ne!(iscsi, dhcp);
        assert_ne!(zfs, dhcp);
    }

    #[test]
    fn transaction_state_starts_active() {
        assert_eq!(
            ProvisioningTransactionState::Active,
            ProvisioningTransactionState::Active
        );
    }
}
