use crate::{
    domain::storage::{
        ClientStorage, ClientStorageSpec, StorageReconcileResult, StorageSource, StorageState,
        StorageVolume,
    },
    infrastructure::{
        image::ImageBackend,
        iscsi::{IscsiLunSpec, IscsiProvisionResult, IscsiProvisioner, IscsiTargetSpec},
    },
};
use anyhow::{bail, Context, Result};
use std::sync::Arc;

mod offline_replacement;
pub use offline_replacement::OfflineReplacement;

/// Application-level storage orchestration.
///
/// `StorageService` knows what needs to happen for a diskless client,
/// but does not know how ZFS or targetcli implement the operation.
///
/// Infrastructure dependencies are injected through the constructor:
///
/// ```text
/// StorageService
///     │
///     ├── ImageBackend
///     │      └── ZfsImageBackend
///     │
///     └── IscsiProvisioner
///            └── TargetCliProvisioner
/// ```
pub struct StorageService {
    image_backend: Arc<dyn ImageBackend>,
    iscsi: Arc<dyn IscsiProvisioner>,
}

impl StorageService {
    pub fn new(image_backend: Arc<dyn ImageBackend>, iscsi: Arc<dyn IscsiProvisioner>) -> Self {
        Self {
            image_backend,
            iscsi,
        }
    }

    /// Discover all ZFS game-disk volumes.
    ///
    /// Game disks are stored below:
    ///
    /// ```text
    /// <zpool>/games/
    /// ```
    fn get_game_disks() -> Result<Vec<String>> {
        use crate::config::get_zpool_name;
        use crate::infrastructure::command::run_command_output_no_sudo;

        let zpool = get_zpool_name();
        let games_parent = format!("{zpool}/games");

        if run_command_output_no_sudo(["zfs", "list", "-H", &games_parent]).is_err() {
            return Ok(Vec::new());
        }

        let output = run_command_output_no_sudo([
            "zfs",
            "list",
            "-H",
            "-t",
            "volume",
            "-o",
            "name",
            "-r",
            &games_parent,
        ])
        .map_err(|error| anyhow::anyhow!("failed to list ZFS game disks: {}", error))?;

        Ok(output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter(|line| *line != games_parent)
            .map(|dataset| format!("/dev/zvol/{dataset}"))
            .collect())
    }

    // ====================================================================
    // CREATE
    // ====================================================================

    /// Create client storage.
    ///
    /// This compatibility method preserves the original API.
    ///
    /// When callers need transaction ownership information they should
    /// use `create_client_storage_transaction()`.
    pub fn create_client_storage(&self, spec: &ClientStorageSpec) -> Result<ClientStorage> {
        let (storage, _) = self.create_client_storage_transaction(spec)?;
        Ok(storage)
    }

    /// Create client storage and return the exact iSCSI resources
    /// created by this operation.
    ///
    /// The returned `IscsiProvisionResult` is authoritative for
    /// transaction rollback.
    ///
    /// Existing resources are deliberately not reported as created.
    pub fn create_client_storage_transaction(
        &self,
        spec: &ClientStorageSpec,
    ) -> Result<(ClientStorage, IscsiProvisionResult)> {
        self.validate_spec(spec)?;

        let block_device = spec.block_device();

        tracing::info!(
            client_id = %spec.client_id,
            dataset = %spec.dataset,
            target_iqn = %spec.target_iqn,
            source = %spec.source.value(),
            use_game_disk = spec.use_game_disk,
            "creating client storage"
        );

        // ---------------------------------------------------------------
        // Step 1: Prepare ZFS resource.
        // ---------------------------------------------------------------

        match &spec.source {
            StorageSource::Snapshot(snapshot) => {
                if self.image_backend.exists(&spec.dataset)? {
                    bail!("client storage dataset already exists: {}", spec.dataset);
                }

                self.image_backend
                    .clone_image(snapshot, &spec.dataset)
                    .with_context(|| {
                        format!(
                            "failed to create ZFS clone '{}' from '{}'",
                            spec.dataset, snapshot
                        )
                    })?;

                let exists = self
                    .image_backend
                    .exists(&spec.dataset)
                    .context("failed to verify newly created client ZFS clone")?;

                if !exists {
                    let _ = self.image_backend.destroy(&spec.dataset);

                    bail!(
                        "ZFS clone was reported successful but does not exist: {}",
                        spec.dataset
                    );
                }
            }

            StorageSource::ExistingVolume(dataset) => {
                if dataset != &spec.dataset {
                    bail!(
                        "existing volume source '{}' does not match destination '{}'",
                        dataset,
                        spec.dataset
                    );
                }

                if !self.image_backend.exists(&spec.dataset)? {
                    bail!("existing ZFS volume does not exist: {}", spec.dataset);
                }

                tracing::debug!(
                    client_id = %spec.client_id,
                    dataset = %spec.dataset,
                    "using existing ZFS volume; resource is not client-owned"
                );
            }

            StorageSource::ExistingClientVolume(dataset) => {
                if dataset != &spec.dataset {
                    bail!(
                        "existing client volume source '{}' does not match destination '{}'",
                        dataset,
                        spec.dataset
                    );
                }

                if !self.image_backend.exists(&spec.dataset)? {
                    bail!(
                        "existing client ZFS volume does not exist: {}",
                        spec.dataset
                    );
                }

                tracing::debug!(
                    client_id = %spec.client_id,
                    dataset = %spec.dataset,
                    "using existing client-owned ZFS volume"
                );
            }
        }

        // ---------------------------------------------------------------
        // Step 2: Build iSCSI LUN specification.
        // ---------------------------------------------------------------

        let mut luns = vec![IscsiLunSpec::new(spec.lun, &spec.backstore, &block_device)];

        if spec.use_game_disk {
            let game_disks = Self::get_game_disks()?;

            for (index, game_disk_path) in game_disks.iter().enumerate() {
                let lun_number = (index + 1) as u32;

                let game_disk_name = game_disk_path
                    .strip_prefix("/dev/zvol/")
                    .unwrap_or(game_disk_path)
                    .replace('/', "_");

                let game_backstore = format!("game_{game_disk_name}");

                luns.push(IscsiLunSpec::readonly(
                    lun_number,
                    game_backstore,
                    game_disk_path,
                ));
            }

            tracing::info!(
                client_id = %spec.client_id,
                game_disk_count = luns.len().saturating_sub(1),
                "adding shared game disks to client iSCSI target"
            );
        }

        let iscsi_spec = IscsiTargetSpec::with_luns(&spec.target_iqn, luns)?;

        // ---------------------------------------------------------------
        // Step 3: Create iSCSI transactionally.
        // ---------------------------------------------------------------

        let iscsi_result = match self.iscsi.create_target_transaction(&iscsi_spec) {
            Ok(result) => result,

            Err(error) => {
                tracing::error!(
                    client_id = %spec.client_id,
                    dataset = %spec.dataset,
                    target_iqn = %spec.target_iqn,
                    error = %error,
                    "iSCSI provisioning failed"
                );

                // The iSCSI provisioner already rolled back its own
                // partially-created resources.
                //
                // StorageService only owns the ZFS resource when
                // the source declares ownership.
                if spec.owns_dataset() {
                    if let Err(cleanup_error) = self.image_backend.destroy(&spec.dataset) {
                        tracing::error!(
                            client_id = %spec.client_id,
                            dataset = %spec.dataset,
                            error = %cleanup_error,
                            "failed to rollback ZFS client resource after iSCSI failure"
                        );

                        return Err(error).context(format!(
                            "iSCSI provisioning failed and ZFS rollback also failed: {}",
                            cleanup_error
                        ));
                    }
                }

                return Err(error).with_context(|| {
                    format!(
                        "failed to provision iSCSI storage for client '{}'",
                        spec.client_id
                    )
                });
            }
        };

        // ---------------------------------------------------------------
        // Step 4: Build application result.
        // ---------------------------------------------------------------

        let volume = StorageVolume::new(
            spec.dataset.clone(),
            block_device,
            spec.backstore.clone(),
            spec.target_iqn.clone(),
            spec.lun,
        );

        let storage = ClientStorage {
            client_id: spec.client_id.clone(),
            source: spec.source.clone(),
            volume,
            use_game_disk: spec.use_game_disk,
        };

        tracing::info!(
            client_id = %spec.client_id,
            target_iqn = %spec.target_iqn,
            target_created = iscsi_result.target_created,
            portal_created = iscsi_result.portal_created,
            luns_created = ?iscsi_result.luns_created,
            backstores_created = ?iscsi_result.backstores_created,
            "client storage provisioned"
        );

        Ok((storage, iscsi_result))
    }

    // ====================================================================
    // DESTROY
    // ====================================================================

    /// Destroy the client's storage resources.
    pub fn destroy_client_storage(&self, storage: &ClientStorage) -> Result<()> {
        tracing::info!(
            client_id = %storage.client_id,
            dataset = %storage.dataset(),
            target_iqn = %storage.target_iqn(),
            owns_dataset = storage.owns_dataset(),
            use_game_disk = storage.use_game_disk,
            "destroying client storage"
        );

        let owned_backstores = vec![storage.backstore().to_string()];

        self.iscsi
            .remove_target_with_backstores(storage.target_iqn(), &owned_backstores)
            .with_context(|| format!("failed to remove iSCSI target '{}'", storage.target_iqn()))?;

        if storage.owns_dataset() {
            self.image_backend
                .destroy(storage.dataset())
                .with_context(|| {
                    format!(
                        "failed to destroy client ZFS storage '{}'",
                        storage.dataset()
                    )
                })?;
        } else {
            tracing::debug!(
                client_id = %storage.client_id,
                dataset = %storage.dataset(),
                "preserving non-owned ZFS volume"
            );
        }

        Ok(())
    }

    // ====================================================================
    // RESET
    // ====================================================================

    /// Reset a client's storage from its desired source.
    pub fn reset_client_storage(
        &self,
        current: &ClientStorage,
        spec: &ClientStorageSpec,
    ) -> Result<ClientStorage> {
        if current.client_id != spec.client_id {
            bail!(
                "storage client mismatch: current='{}', requested='{}'",
                current.client_id,
                spec.client_id
            );
        }

        if !spec.owns_dataset() {
            bail!("cannot reset client storage from an existing shared volume");
        }

        tracing::info!(
            client_id = %spec.client_id,
            old_dataset = %current.dataset(),
            new_dataset = %spec.dataset,
            "resetting client storage"
        );

        let owned_backstores = vec![current.backstore().to_string()];

        self.iscsi
            .remove_target_with_backstores(current.target_iqn(), &owned_backstores)
            .with_context(|| {
                format!(
                    "failed to remove existing iSCSI target '{}'",
                    current.target_iqn()
                )
            })?;

        if current.owns_dataset() && self.image_backend.exists(current.dataset())? {
            self.image_backend
                .destroy(current.dataset())
                .with_context(|| {
                    format!(
                        "failed to destroy existing client storage '{}'",
                        current.dataset()
                    )
                })?;
        }

        self.create_client_storage(spec)
            .context("client storage reset failed")
    }

    // ====================================================================
    // TARGET REMOVAL
    // ====================================================================

    /// Remove a client's iSCSI target without destroying ZFS storage.
    ///
    /// `backstores` must contain only backstores owned by the caller.
    pub fn remove_client_target(&self, target_iqn: &str, backstores: &[String]) -> Result<()> {
        if target_iqn.trim().is_empty() {
            bail!("target IQN cannot be empty");
        }

        let owned_backstores: Vec<String> = backstores
            .iter()
            .filter(|item| !item.trim().is_empty())
            .cloned()
            .collect();

        if owned_backstores.is_empty() {
            self.iscsi
                .remove_target(target_iqn)
                .with_context(|| format!("failed to remove iSCSI target '{}'", target_iqn))
        } else {
            self.iscsi
                .remove_target_with_backstores(target_iqn, &owned_backstores)
                .with_context(|| {
                    format!(
                        "failed to remove iSCSI target '{}' and owned backstores",
                        target_iqn
                    )
                })
        }
    }

    // ====================================================================
    // RECONCILIATION
    // ====================================================================

    /// Inspect current storage state without changing anything.
    pub fn reconcile_client_storage(
        &self,
        spec: &ClientStorageSpec,
    ) -> Result<StorageReconcileResult> {
        self.validate_spec(spec)?;

        let zfs_present = self
            .image_backend
            .exists(&spec.dataset)
            .context("failed to inspect ZFS client storage")?;

        let iscsi_spec = if spec.use_game_disk {
            let mut luns = vec![IscsiLunSpec::new(
                spec.lun,
                &spec.backstore,
                spec.block_device(),
            )];

            let game_disks = Self::get_game_disks()?;

            for (index, game_disk_path) in game_disks.iter().enumerate() {
                let lun_number = (index + 1) as u32;

                let game_disk_name = game_disk_path
                    .strip_prefix("/dev/zvol/")
                    .unwrap_or(game_disk_path)
                    .replace('/', "_");

                let game_backstore = format!("game_{game_disk_name}");

                luns.push(IscsiLunSpec::readonly(
                    lun_number,
                    game_backstore,
                    game_disk_path,
                ));
            }

            IscsiTargetSpec::with_luns(&spec.target_iqn, luns)?
        } else {
            IscsiTargetSpec::new(
                &spec.target_iqn,
                &spec.backstore,
                spec.block_device(),
                spec.lun,
            )
        };

        let iscsi_state = self
            .iscsi
            .inspect_target(&iscsi_spec)
            .context("failed to inspect iSCSI client storage")?;

        let iscsi_present = iscsi_state.is_ready();

        let state = match (zfs_present, iscsi_present) {
            (false, false) => StorageState::Missing,
            (true, true) => StorageState::Ready,
            _ => StorageState::Partial,
        };

        Ok(StorageReconcileResult {
            state,
            zfs_present,
            iscsi_present,
            target_iqn: spec.target_iqn.clone(),
            dataset: spec.dataset.clone(),
        })
    }

    /// Reconcile actual infrastructure to desired state.
    pub fn reconcile_client_storage_in_place(
        &self,
        spec: &ClientStorageSpec,
    ) -> Result<ClientStorage> {
        let state = self.reconcile_client_storage(spec)?;

        match state.state {
            StorageState::Ready => Ok(self.storage_from_spec(spec)),

            StorageState::Missing => self.create_client_storage(spec),

            StorageState::Partial => self.repair_partial_storage(spec),

            StorageState::InUse => {
                bail!("client storage '{}' is currently in use", spec.client_id);
            }

            StorageState::Error => {
                bail!("client storage '{}' is in an error state", spec.client_id);
            }
        }
    }

    // ====================================================================
    // PARTIAL REPAIR
    // ====================================================================

    fn repair_partial_storage(&self, spec: &ClientStorageSpec) -> Result<ClientStorage> {
        tracing::warn!(
            client_id = %spec.client_id,
            dataset = %spec.dataset,
            target_iqn = %spec.target_iqn,
            "repairing partially provisioned client storage"
        );

        if self.iscsi.target_exists(&spec.target_iqn)? {
            let owned_backstores = vec![spec.backstore.clone()];

            self.iscsi
                .remove_target_with_backstores(&spec.target_iqn, &owned_backstores)
                .with_context(|| {
                    format!(
                        "failed to remove partial iSCSI target '{}'",
                        spec.target_iqn
                    )
                })?;
        } else {
            self.iscsi
                .remove_target_with_backstores(
                    &spec.target_iqn,
                    std::slice::from_ref(&spec.backstore),
                )
                .with_context(|| {
                    format!(
                        "failed to remove partial iSCSI backstore '{}'",
                        spec.backstore
                    )
                })?;
        }

        if spec.owns_dataset() && self.image_backend.exists(&spec.dataset)? {
            self.image_backend.destroy(&spec.dataset).with_context(|| {
                format!(
                    "failed to remove partial client ZFS storage '{}'",
                    spec.dataset
                )
            })?;
        }

        self.create_client_storage(spec)
            .context("failed to repair partial client storage")
    }

    // ====================================================================
    // HELPERS
    // ====================================================================

    fn storage_from_spec(&self, spec: &ClientStorageSpec) -> ClientStorage {
        ClientStorage {
            client_id: spec.client_id.clone(),
            source: spec.source.clone(),
            volume: StorageVolume::new(
                spec.dataset.clone(),
                spec.block_device(),
                spec.backstore.clone(),
                spec.target_iqn.clone(),
                spec.lun,
            ),
            use_game_disk: spec.use_game_disk,
        }
    }

    fn validate_spec(&self, spec: &ClientStorageSpec) -> Result<()> {
        if spec.client_id.trim().is_empty() {
            bail!("client ID cannot be empty");
        }

        if spec.dataset.trim().is_empty() {
            bail!("ZFS client dataset cannot be empty");
        }

        if spec.backstore.trim().is_empty() {
            bail!("iSCSI backstore cannot be empty");
        }

        if spec.target_iqn.trim().is_empty() {
            bail!("iSCSI target IQN cannot be empty");
        }

        match &spec.source {
            StorageSource::Snapshot(snapshot) => {
                if snapshot.trim().is_empty() {
                    bail!("source snapshot cannot be empty");
                }

                if !snapshot.contains('@') {
                    bail!("source snapshot must be a ZFS snapshot: '{}'", snapshot);
                }
            }

            StorageSource::ExistingVolume(volume) => {
                if volume.trim().is_empty() {
                    bail!("existing volume cannot be empty");
                }
            }

            StorageSource::ExistingClientVolume(volume) => {
                if volume.trim().is_empty() {
                    bail!("existing client volume cannot be empty");
                }
            }
        }

        Ok(())
    }
}
