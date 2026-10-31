use super::*;
use crate::infrastructure::iscsi::ChapCredentials;

/// Unique staging paths retain the original until target verification and DB commit.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OfflineReplacement {
    pub spec: ClientStorageSpec,
    pub staged: String,
    pub backup: String,
    pub committed: bool,
}

impl OfflineReplacement {
    pub fn new(spec: ClientStorageSpec) -> Self {
        let token = uuid::Uuid::new_v4().simple().to_string();
        Self {
            staged: format!("{}-reset-{token}", spec.dataset),
            backup: format!("{}-previous-{token}", spec.dataset),
            spec,
            committed: false,
        }
    }

    /// Adopt an existing prewarmed spare as the staged replacement.
    ///
    /// The persisted operation never contains CHAP secrets; the boot path
    /// re-applies authentication from live state after the rename.
    pub fn from_spare(mut spec: ClientStorageSpec, spare: String) -> Self {
        spec.chap = None;
        let token = uuid::Uuid::new_v4().simple().to_string();
        Self {
            backup: format!("{}-previous-{token}", spec.dataset),
            staged: spare,
            spec,
            committed: false,
        }
    }
}

impl StorageService {
    /// A reconnect proves the replacement may now contain new client writes.
    /// Adopt it after a DB interruption instead of rolling those writes back.
    pub fn replacement_is_attached(&self, operation: &OfflineReplacement) -> Result<bool> {
        Ok(self.image_backend.exists(&operation.backup)?
            && self.image_backend.verify(&operation.spec.dataset)?
            && self
                .reconcile_client_storage(&operation.spec)?
                .iscsi_present)
    }
    fn expose_existing(&self, spec: &ClientStorageSpec) -> Result<()> {
        // A renamed ZVOL's old symlink can briefly remain present. Merely
        // checking path existence could attach the previous disk by mistake.
        self.image_backend.settle_device_changes()?;
        let mut existing = spec.clone();
        // Target provisioning must never delete a retained volume on failure.
        existing.source = StorageSource::ExistingVolume(spec.dataset.clone());
        self.create_client_storage(&existing)?;
        if !self.reconcile_client_storage(&existing)?.iscsi_present {
            bail!("replacement target verification failed");
        }
        Ok(())
    }

    pub fn replace_offline(&self, operation: &OfflineReplacement) -> Result<()> {
        let spec = &operation.spec;
        self.validate_spec(spec)?;
        self.image_backend.settle_device_changes()?;
        let StorageSource::Snapshot(snapshot) = &spec.source else {
            bail!("automatic reset requires a snapshot clone");
        };
        if snapshot.split('@').next() == Some(spec.dataset.as_str()) {
            bail!("refusing to replace a master volume");
        }
        if !self.image_backend.verify(&spec.dataset)? {
            bail!("existing client volume is unavailable");
        }
        if self.image_backend.clone_origin(&spec.dataset)?.as_deref() != Some(snapshot.as_str()) {
            bail!("client clone origin does not match the configured snapshot");
        }
        if self.image_backend.exists(&operation.staged)?
            || self.image_backend.exists(&operation.backup)?
        {
            bail!("replacement paths already exist; recover the journal first");
        }
        self.image_backend
            .clone_image(snapshot, &operation.staged)?;
        if !self.image_backend.verify(&operation.staged)? {
            bail!("staged clone verification failed");
        }
        self.iscsi.remove_target_with_backstores(
            &spec.target_iqn,
            std::slice::from_ref(&spec.backstore),
        )?;
        self.image_backend
            .rename(&spec.dataset, &operation.backup)?;
        self.image_backend
            .rename(&operation.staged, &spec.dataset)?;
        self.expose_existing(spec)
    }

    /// Create a job-owned replacement clone without attaching it anywhere.
    ///
    /// The caller owns `spare` and must consume or destroy it. A partially
    /// created or unverifiable clone is removed before the error propagates.
    pub fn prepare_spare_clone(&self, snapshot: &str, spare: &str) -> Result<()> {
        anyhow::ensure!(
            snapshot.contains('@') && snapshot.split('@').next() != Some(spare),
            "spare source must be an immutable snapshot of another dataset"
        );
        anyhow::ensure!(
            spare.contains("-spare-") && !spare.contains('@'),
            "spare dataset name is not a job-owned spare volume"
        );
        if self.image_backend.exists(spare)? {
            bail!("spare volume already exists: {spare}");
        }
        if !self.image_backend.exists(snapshot)? {
            bail!("spare source snapshot is missing: {snapshot}");
        }
        let check = (|| -> Result<()> {
            self.image_backend.clone_image(snapshot, spare)?;
            if !self.image_backend.verify(spare)? {
                bail!("spare clone verification failed: {spare}");
            }
            if self.image_backend.clone_origin(spare)?.as_deref() != Some(snapshot) {
                bail!("spare clone origin does not match its source snapshot: {spare}");
            }
            Ok(())
        })();
        if let Err(error) = check {
            if let Err(cleanup) = self.image_backend.destroy(spare) {
                tracing::error!(
                    spare = %spare,
                    error = %cleanup,
                    "failed to remove an unusable spare clone"
                );
            }
            return Err(error);
        }
        Ok(())
    }

    /// Destroy a job-owned spare clone. Only spare-named datasets are accepted
    /// so a client boot volume can never be removed through this path.
    pub fn discard_spare_clone(&self, spare: &str) -> Result<()> {
        anyhow::ensure!(
            spare.contains("-spare-") && !spare.contains('@'),
            "refusing to discard a dataset that is not a job-owned spare"
        );
        if !self.image_backend.exists(spare)? {
            return Ok(());
        }
        self.image_backend.validate_unmounted(spare)?;
        self.image_backend
            .destroy(spare)
            .with_context(|| format!("failed to destroy spare clone '{spare}'"))
    }

    /// Check that a spare can replace the client's boot volume without any
    /// mutation: same source snapshot, absent boot dataset, no local mounts.
    pub fn spare_is_consumable(&self, spec: &ClientStorageSpec, spare: &str) -> Result<()> {
        self.validate_spec(spec)?;
        let StorageSource::Snapshot(snapshot) = &spec.source else {
            bail!("spare adoption requires a snapshot-backed assignment");
        };
        if snapshot.split('@').next() == Some(spec.dataset.as_str()) {
            bail!("refusing to replace a master volume");
        }
        anyhow::ensure!(
            spare.starts_with(spec.dataset.as_str())
                && spare != spec.dataset
                && spare.contains("-spare-"),
            "spare dataset is outside the client's storage namespace"
        );
        self.image_backend.settle_device_changes()?;
        if self.image_backend.exists(&spec.dataset)? {
            bail!("client already has boot storage; a spare must not replace it");
        }
        if !self.image_backend.exists(spare)? {
            bail!("spare volume is missing");
        }
        self.image_backend.validate_unmounted(spare)?;
        if !self.image_backend.verify(spare)? {
            bail!("spare volume failed verification");
        }
        if self.image_backend.clone_origin(spare)?.as_deref() != Some(snapshot.as_str()) {
            bail!("spare clone origin does not match the configured snapshot");
        }
        Ok(())
    }

    /// Rename a validated spare into the client's boot dataset and expose it.
    ///
    /// The caller must have journaled `operation` first. Any failure leaves a
    /// journalled state [`recover_offline`](Self::recover_offline) can resolve:
    /// the spare is restored or rebuilt from its recorded snapshot.
    pub fn consume_spare_offline(
        &self,
        operation: &OfflineReplacement,
        chap: Option<&ChapCredentials>,
    ) -> Result<()> {
        let spec = &operation.spec;
        self.spare_is_consumable(spec, &operation.staged)?;
        self.iscsi.remove_target_with_backstores(
            &spec.target_iqn,
            std::slice::from_ref(&spec.backstore),
        )?;
        self.image_backend
            .rename(&operation.staged, &spec.dataset)?;
        self.expose_existing(spec)?;
        // The journaled spec carries no secrets; authentication is restored
        // from live state before the client can log in.
        self.set_target_chap(&spec.target_iqn, chap)?;
        Ok(())
    }

    /// Resume committed cleanup, or restore the original after an interrupted
    /// switch. If neither the original nor a backup exists, rebuild an
    /// uncommitted clone from its recorded snapshot. The caller must establish
    /// that the client is offline.
    pub fn recover_offline(&self, operation: &OfflineReplacement) -> Result<()> {
        let spec = &operation.spec;
        if operation.committed {
            if !self.image_backend.verify(&spec.dataset)? {
                bail!("cannot clean backup while replacement is unavailable");
            }
            self.expose_existing(spec)?;
            if self.image_backend.exists(&operation.backup)? {
                self.image_backend.destroy(&operation.backup)?;
            }
        } else if self.image_backend.exists(&operation.backup)? {
            self.iscsi.remove_target_with_backstores(
                &spec.target_iqn,
                std::slice::from_ref(&spec.backstore),
            )?;
            if self.image_backend.exists(&spec.dataset)? {
                self.image_backend.destroy(&spec.dataset)?;
            }
            self.image_backend
                .rename(&operation.backup, &spec.dataset)?;
            self.expose_existing(spec)?;
        } else {
            if !self.image_backend.exists(&spec.dataset)? {
                self.validate_spec(spec)?;
                let StorageSource::Snapshot(snapshot) = &spec.source else {
                    bail!("cannot recover missing client volume without a snapshot");
                };
                if snapshot.split('@').next() == Some(spec.dataset.as_str()) {
                    bail!("refusing to rebuild a master volume");
                }
                self.image_backend
                    .clone_image(snapshot, &spec.dataset)
                    .context("failed to rebuild missing client clone from recorded snapshot")?;
            }
            self.expose_existing(spec)?;
        }
        if self.image_backend.exists(&operation.staged)? {
            self.image_backend.destroy(&operation.staged)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::iscsi::{ChapCredentials, IscsiLunState};
    use crate::infrastructure::{image::ImageBackendInfo, iscsi::IscsiTargetState};
    use std::{collections::HashMap, path::Path, sync::Mutex};

    #[derive(Default)]
    struct FakeDisks {
        volumes: Mutex<HashMap<String, String>>,
    }
    impl ImageBackend for FakeDisks {
        fn validate_unmounted(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn settle_device_changes(&self) -> Result<()> {
            Ok(())
        }
        fn clone_origin(&self, _: &str) -> Result<Option<String>> {
            Ok(Some("pool/master@ready".into()))
        }
        fn exists(&self, name: &str) -> Result<bool> {
            Ok(self.volumes.lock().unwrap().contains_key(name))
        }
        fn verify(&self, name: &str) -> Result<bool> {
            self.exists(name)
        }
        fn clone_image(&self, source: &str, destination: &str) -> Result<()> {
            assert_eq!(source, "pool/master@ready");
            let mut volumes = self.volumes.lock().unwrap();
            if volumes.contains_key(destination) {
                bail!("already exists");
            }
            volumes.insert(destination.into(), "clean".into());
            Ok(())
        }
        fn destroy(&self, name: &str) -> Result<()> {
            self.volumes
                .lock()
                .unwrap()
                .remove(name)
                .context("missing volume")?;
            Ok(())
        }
        fn rename(&self, source: &str, destination: &str) -> Result<()> {
            let mut volumes = self.volumes.lock().unwrap();
            if volumes.contains_key(destination) {
                bail!("destination exists");
            }
            let data = volumes.remove(source).context("source missing")?;
            volumes.insert(destination.into(), data);
            Ok(())
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            bail!("unexpected create")
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected snapshot")
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected destroy snapshot")
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected rollback")
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            bail!("unexpected resize")
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> Result<()> {
            bail!("unexpected import")
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            bail!("unexpected info")
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected OS")
        }
        fn image_parent(&self) -> Result<String> {
            bail!("unexpected parent")
        }
    }

    #[derive(Default)]
    struct FakeTarget {
        fail_next: Mutex<bool>,
        connected: Mutex<bool>,
        attached: Mutex<bool>,
        report_absent: Mutex<bool>,
    }
    impl IscsiProvisioner for FakeTarget {
        fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()> {
            assert_eq!(spec.luns[0].block_device, Path::new("/dev/zvol/pool/pc001"));
            if std::mem::take(&mut *self.fail_next.lock().unwrap()) {
                bail!("target creation failed");
            }
            *self.attached.lock().unwrap() = true;
            Ok(())
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            bail!("must preserve target")
        }
        fn remove_target_with_backstores(&self, _: &str, backstores: &[String]) -> Result<()> {
            assert!(
                backstores == ["block_pc001"]
                    || backstores == ["block_pc001", "installer_client-1"],
                "unexpected owned backstores: {backstores:?}"
            );
            if *self.connected.lock().unwrap() {
                bail!("client reconnected");
            }
            *self.attached.lock().unwrap() = false;
            Ok(())
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            Ok(!*self.report_absent.lock().unwrap())
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> Result<()> {
            Ok(())
        }
        fn list_target_luns(&self, _: &str) -> Result<Vec<IscsiLunState>> {
            Ok(Vec::new())
        }
        fn inspect_target(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
            let attached = *self.attached.lock().unwrap();
            Ok(IscsiTargetState {
                target_iqn: spec.target_iqn.clone(),
                exists: !*self.report_absent.lock().unwrap(),
                backstore_exists: attached,
                lun_exists: attached,
                luns: vec![],
                portal_exists: true,
            })
        }
        fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
            self.create_target(spec)
        }
    }
    fn fixture() -> (
        StorageService,
        Arc<FakeDisks>,
        Arc<FakeTarget>,
        OfflineReplacement,
    ) {
        let disks = Arc::new(FakeDisks::default());
        disks
            .volumes
            .lock()
            .unwrap()
            .insert("pool/pc001".into(), "user changes".into());
        let target = Arc::new(FakeTarget::default());
        *target.attached.lock().unwrap() = true;
        let service = StorageService::new(disks.clone(), target.clone());
        let operation = OfflineReplacement::new(ClientStorageSpec {
            installer_iso: None,
            client_id: "client-1".into(),
            client_name: "PC001".into(),
            source: StorageSource::Snapshot("pool/master@ready".into()),
            dataset: "pool/pc001".into(),
            backstore: "block_pc001".into(),
            target_iqn: "iqn.test:pc001".into(),
            lun: 0,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap: None,
        });
        (service, disks, target, operation)
    }

    #[test]
    fn legacy_exit_failure_is_retryable_without_destroying_the_shared_master() {
        let (service, disks, target, operation) = fixture();
        disks.volumes.lock().unwrap().remove("pool/pc001");
        disks
            .volumes
            .lock()
            .unwrap()
            .insert("pool/master".into(), "shared master".into());
        *target.fail_next.lock().unwrap() = true;

        assert!(service
            .exit_legacy_super(
                "iqn.test:pc001",
                "block_pc001",
                &operation.spec,
                "pool/master@ready"
            )
            .is_err());
        assert!(!disks.volumes.lock().unwrap().contains_key("pool/pc001"));
        assert_eq!(
            disks
                .volumes
                .lock()
                .unwrap()
                .get("pool/master")
                .map(String::as_str),
            Some("shared master")
        );

        service
            .exit_legacy_super(
                "iqn.test:pc001",
                "block_pc001",
                &operation.spec,
                "pool/master@ready",
            )
            .unwrap();
        // A restarted retry can safely rebuild the target from the existing clone.
        service
            .exit_legacy_super(
                "iqn.test:pc001",
                "block_pc001",
                &operation.spec,
                "pool/master@ready",
            )
            .unwrap();
        assert!(disks.volumes.lock().unwrap().contains_key("pool/pc001"));
        assert!(*target.attached.lock().unwrap());
        assert_eq!(
            disks
                .volumes
                .lock()
                .unwrap()
                .get("pool/master")
                .map(String::as_str),
            Some("shared master")
        );
    }

    #[test]
    fn boot_target_failure_preserves_existing_client_clone() {
        let (service, disks, target, op) = fixture();
        *target.report_absent.lock().unwrap() = true;
        *target.fail_next.lock().unwrap() = true;

        assert!(service.ensure_client_storage_for_boot(&op.spec).is_err());

        assert_eq!(
            disks.volumes.lock().unwrap().get(&op.spec.dataset).unwrap(),
            "user changes"
        );
    }

    #[test]
    fn cleanup_can_be_retried_after_client_clone_is_already_absent() {
        let (service, disks, target, op) = fixture();
        disks.destroy(&op.spec.dataset).unwrap();
        let current = ClientStorage {
            client_id: op.spec.client_id.clone(),
            source: op.spec.source.clone(),
            volume: StorageVolume::new(
                &op.spec.dataset,
                op.spec.block_device(),
                &op.spec.backstore,
                &op.spec.target_iqn,
                op.spec.lun,
            ),
            use_game_disk: false,
        };

        service.destroy_client_storage(&current).unwrap();
        service.destroy_client_storage(&current).unwrap();

        assert!(!disks.exists(&op.spec.dataset).unwrap());
        assert!(!*target.attached.lock().unwrap());
    }

    #[test]
    fn success_retains_original_until_durable_commit() {
        let (service, disks, target, mut op) = fixture();
        service.replace_offline(&op).unwrap();
        assert_eq!(
            disks.volumes.lock().unwrap().get(&op.backup).unwrap(),
            "user changes"
        );
        assert_eq!(
            disks.volumes.lock().unwrap().get("pool/pc001").unwrap(),
            "clean"
        );
        assert!(*target.attached.lock().unwrap());
        op.committed = true;
        service.recover_offline(&op).unwrap();
        assert_eq!(disks.volumes.lock().unwrap().len(), 1);
    }
    #[test]
    fn failed_switch_restores_existing_clone_and_target() {
        let (service, disks, target, op) = fixture();
        *target.fail_next.lock().unwrap() = true;
        assert!(service.replace_offline(&op).is_err());
        service.recover_offline(&op).unwrap();
        assert_eq!(
            disks.volumes.lock().unwrap().get("pool/pc001").unwrap(),
            "user changes"
        );
        assert!(*target.attached.lock().unwrap());
        assert_eq!(disks.volumes.lock().unwrap().len(), 1);
    }
    #[test]
    fn reconnect_during_staging_preserves_original() {
        let (service, disks, target, op) = fixture();
        *target.connected.lock().unwrap() = true;
        assert!(service.replace_offline(&op).is_err());
        assert_eq!(
            disks.volumes.lock().unwrap().get("pool/pc001").unwrap(),
            "user changes"
        );
        assert!(*target.attached.lock().unwrap());
    }
    #[test]
    fn uncommitted_recovery_rebuilds_missing_clone_from_recorded_snapshot() {
        let (service, disks, target, op) = fixture();
        disks.destroy(&op.spec.dataset).unwrap();
        *target.attached.lock().unwrap() = false;

        service.recover_offline(&op).unwrap();

        assert_eq!(
            disks.volumes.lock().unwrap().get(&op.spec.dataset).unwrap(),
            "clean"
        );
        assert!(*target.attached.lock().unwrap());
        assert_eq!(disks.volumes.lock().unwrap().len(), 1);
    }

    #[test]
    fn committed_recovery_preserves_backup_when_replacement_is_missing() {
        let (service, disks, _, mut op) = fixture();
        disks.rename(&op.spec.dataset, &op.backup).unwrap();
        op.committed = true;

        assert!(service.recover_offline(&op).is_err());

        assert_eq!(
            disks.volumes.lock().unwrap().get(&op.backup).unwrap(),
            "user changes"
        );
        assert!(!disks.exists(&op.spec.dataset).unwrap());
    }

    #[test]
    fn restart_between_renames_restores_original() {
        let (service, disks, target, op) = fixture();
        disks.clone_image("pool/master@ready", &op.staged).unwrap();
        disks.rename("pool/pc001", &op.backup).unwrap();
        let serialized = serde_json::to_string(&op).unwrap();
        let restarted: OfflineReplacement = serde_json::from_str(&serialized).unwrap();
        service.recover_offline(&restarted).unwrap();
        assert_eq!(
            disks.volumes.lock().unwrap().get("pool/pc001").unwrap(),
            "user changes"
        );
        assert!(*target.attached.lock().unwrap());
    }
}

/// Journaled adoption of a prewarmed spare, exercised against fake storage so
/// no live pool, LIO target or client disk is involved.
#[cfg(test)]
mod spare_tests {
    use super::*;
    use crate::infrastructure::image::ImageBackendInfo;
    use crate::infrastructure::iscsi::{ChapCredentials, IscsiLunState, IscsiTargetState};
    use std::collections::{HashMap, HashSet};
    use std::path::Path;
    use std::sync::Mutex;

    const SPARE: &str = "pool/clients/pc001-spare-abc123";

    #[derive(Default)]
    struct SpareDisks {
        // dataset -> origin (empty for snapshots)
        volumes: Mutex<HashMap<String, String>>,
        unreadable: Mutex<HashSet<String>>,
        unverifiable: Mutex<HashSet<String>>,
    }

    impl SpareDisks {
        fn add(&self, dataset: &str, origin: &str) {
            self.volumes
                .lock()
                .unwrap()
                .insert(dataset.into(), origin.into());
        }

        fn content(&self, dataset: &str) -> Option<String> {
            self.volumes.lock().unwrap().get(dataset).cloned()
        }

        fn is_unreadable(&self, dataset: &str) -> bool {
            self.unreadable.lock().unwrap().contains(dataset)
        }
    }

    impl ImageBackend for SpareDisks {
        fn validate_unmounted(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn settle_device_changes(&self) -> Result<()> {
            Ok(())
        }
        fn clone_origin(&self, name: &str) -> Result<Option<String>> {
            if self.is_unreadable(name) {
                bail!("injected origin query failure");
            }
            Ok(self
                .volumes
                .lock()
                .unwrap()
                .get(name)
                .filter(|origin| !origin.is_empty())
                .cloned())
        }
        fn exists(&self, name: &str) -> Result<bool> {
            if self.is_unreadable(name) {
                bail!("injected presence query failure");
            }
            Ok(self.volumes.lock().unwrap().contains_key(name))
        }
        fn verify(&self, name: &str) -> Result<bool> {
            Ok(self.exists(name)? && !self.unverifiable.lock().unwrap().contains(name))
        }
        fn clone_image(&self, source: &str, destination: &str) -> Result<()> {
            let mut volumes = self.volumes.lock().unwrap();
            if !volumes.contains_key(source) {
                bail!("missing source {source}");
            }
            if volumes.contains_key(destination) {
                bail!("destination exists");
            }
            volumes.insert(destination.into(), source.into());
            Ok(())
        }
        fn destroy(&self, name: &str) -> Result<()> {
            self.volumes.lock().unwrap().remove(name);
            Ok(())
        }
        fn rename(&self, source: &str, destination: &str) -> Result<()> {
            let mut volumes = self.volumes.lock().unwrap();
            if volumes.contains_key(destination) {
                bail!("destination exists");
            }
            let origin = volumes.remove(source).context("missing source")?;
            volumes.insert(destination.into(), origin);
            Ok(())
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            bail!("unexpected create")
        }
        fn create_snapshot(&self, dataset: &str, _: &str) -> Result<()> {
            let mut volumes = self.volumes.lock().unwrap();
            volumes.entry(dataset.into()).or_default();
            Ok(())
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected destroy snapshot")
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected rollback")
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            bail!("unexpected resize")
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> Result<()> {
            bail!("unexpected import")
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            bail!("unexpected info")
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            bail!("unexpected OS")
        }
        fn image_parent(&self) -> Result<String> {
            Ok("pool/clients".into())
        }
    }

    #[derive(Default)]
    struct SpareTarget {
        attached: Mutex<bool>,
        connected: Mutex<bool>,
        fail_next: Mutex<bool>,
        chap: Mutex<Option<ChapCredentials>>,
    }

    impl SpareTarget {
        fn attached(&self) -> bool {
            *self.attached.lock().unwrap()
        }

        fn chap(&self) -> Option<ChapCredentials> {
            self.chap.lock().unwrap().clone()
        }
    }

    impl IscsiProvisioner for SpareTarget {
        fn create_target(&self, _: &IscsiTargetSpec) -> Result<()> {
            if std::mem::take(&mut *self.fail_next.lock().unwrap()) {
                bail!("injected target creation failure");
            }
            *self.attached.lock().unwrap() = true;
            Ok(())
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            *self.attached.lock().unwrap() = false;
            Ok(())
        }
        fn remove_target_with_backstores(&self, _: &str, backstores: &[String]) -> Result<()> {
            anyhow::ensure!(
                backstores == ["block_pc001"],
                "unexpected owned backstores: {backstores:?}"
            );
            if *self.connected.lock().unwrap() {
                bail!("client is connected");
            }
            *self.attached.lock().unwrap() = false;
            Ok(())
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            Ok(self.attached())
        }
        fn set_chap_auth(&self, _: &str, chap: Option<&ChapCredentials>) -> Result<()> {
            *self.chap.lock().unwrap() = chap.cloned();
            Ok(())
        }
        fn list_target_luns(&self, _: &str) -> Result<Vec<IscsiLunState>> {
            Ok(Vec::new())
        }
        fn inspect_target(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
            let attached = self.attached();
            let luns = spec
                .luns
                .iter()
                .map(|lun| IscsiLunState {
                    lun: lun.lun,
                    backstore: lun.backstore.clone(),
                    exists: attached,
                    backstore_exists: attached,
                    block_device_matches: attached,
                })
                .collect();
            Ok(IscsiTargetState::from_luns(
                spec.target_iqn.clone(),
                attached,
                luns,
                true,
            ))
        }
        fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
            self.create_target(spec)
        }
    }

    fn spec() -> ClientStorageSpec {
        ClientStorageSpec {
            installer_iso: None,
            client_id: "client-1".into(),
            client_name: "pc001".into(),
            source: StorageSource::Snapshot("pool/master@ready".into()),
            dataset: "pool/clients/pc001".into(),
            backstore: "block_pc001".into(),
            target_iqn: "iqn.test:pc001".into(),
            lun: 0,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap: None,
        }
    }

    fn fixture() -> (StorageService, Arc<SpareDisks>, Arc<SpareTarget>) {
        let disks = Arc::new(SpareDisks::default());
        disks.add("pool/master@ready", "");
        let target = Arc::new(SpareTarget::default());
        (
            StorageService::new(disks.clone(), target.clone()),
            disks,
            target,
        )
    }

    fn prepared(service: &StorageService, disks: &SpareDisks) {
        service
            .prepare_spare_clone("pool/master@ready", SPARE)
            .unwrap();
        assert_eq!(disks.content(SPARE).as_deref(), Some("pool/master@ready"));
    }

    #[test]
    fn prepared_spare_is_adopted_and_authentication_is_restored() {
        let (service, disks, target) = fixture();
        prepared(&service, &disks);
        let operation = OfflineReplacement::from_spare(spec(), SPARE.into());
        assert!(
            operation.spec.chap.is_none(),
            "journaled operations must not carry CHAP secrets"
        );

        service.spare_is_consumable(&operation.spec, SPARE).unwrap();
        let chap = ChapCredentials {
            username: "synthetic-user".into(),
            password: "synthetic-secret".into(),
        };
        service
            .consume_spare_offline(&operation, Some(&chap))
            .unwrap();

        assert_eq!(
            disks.content("pool/clients/pc001").as_deref(),
            Some("pool/master@ready")
        );
        assert_eq!(
            disks.content(SPARE),
            None,
            "the spare is consumed, not kept"
        );
        assert!(target.attached());
        assert_eq!(target.chap(), Some(chap));
    }

    #[test]
    fn adoption_refuses_existing_boot_storage_and_connected_clients() {
        let (service, disks, target) = fixture();
        prepared(&service, &disks);
        disks.add("pool/clients/pc001", "pool/master@ready");

        assert!(service
            .spare_is_consumable(&spec(), SPARE)
            .unwrap_err()
            .to_string()
            .contains("already has boot storage"));
        assert!(service
            .consume_spare_offline(&OfflineReplacement::from_spare(spec(), SPARE.into()), None)
            .is_err());
        assert!(disks.content(SPARE).is_some(), "the spare must survive");

        disks.destroy("pool/clients/pc001").unwrap();
        *target.connected.lock().unwrap() = true;
        assert!(service
            .consume_spare_offline(&OfflineReplacement::from_spare(spec(), SPARE.into()), None)
            .is_err());
        assert!(disks.content(SPARE).is_some());
        assert_eq!(disks.content("pool/clients/pc001"), None);
    }

    #[test]
    fn adoption_rejects_wrong_origin_unreadable_state_and_foreign_names() {
        let (service, disks, _) = fixture();
        prepared(&service, &disks);
        disks.add(SPARE, "pool/other@ready");
        let wrong_origin = service.spare_is_consumable(&spec(), SPARE).unwrap_err();
        assert!(wrong_origin.to_string().contains("origin"));

        disks.add(SPARE, "pool/master@ready");
        disks.unreadable.lock().unwrap().insert(SPARE.into());
        assert!(service.spare_is_consumable(&spec(), SPARE).is_err());
        assert!(
            service
                .spare_is_consumable(&spec(), "pool/clients/pc001")
                .is_err(),
            "the client's own boot dataset is never a spare"
        );
        disks.unreadable.lock().unwrap().clear();
        disks.add("pool/clients/pc001-extra-spare-x", "pool/master@ready");
        assert!(
            service
                .spare_is_consumable(&spec(), "pool/clients/pc001-extra-spare-x")
                .is_ok(),
            "another spare in the same client namespace remains valid"
        );
        assert!(
            service.discard_spare_clone("pool/clients/pc001").is_err(),
            "only job-owned spares may be discarded"
        );
        assert!(service.discard_spare_clone(SPARE).is_ok());
        assert_eq!(disks.content(SPARE), None);
    }

    #[test]
    fn interrupted_adoption_keeps_or_rebuilds_a_consistent_volume() {
        // Crash after the rename: the adopted spare is already boot storage.
        let (service, disks, target) = fixture();
        prepared(&service, &disks);
        let operation = OfflineReplacement::from_spare(spec(), SPARE.into());
        disks.rename(SPARE, "pool/clients/pc001").unwrap();
        service.recover_offline(&operation).unwrap();
        assert_eq!(
            disks.content("pool/clients/pc001").as_deref(),
            Some("pool/master@ready")
        );
        assert!(target.attached());

        // Crash before the rename: the spare is discarded and the client's
        // storage is rebuilt from its recorded snapshot.
        let (service, disks, target) = fixture();
        prepared(&service, &disks);
        let operation = OfflineReplacement::from_spare(spec(), SPARE.into());
        service.recover_offline(&operation).unwrap();
        assert_eq!(
            disks.content("pool/clients/pc001").as_deref(),
            Some("pool/master@ready")
        );
        assert_eq!(disks.content(SPARE), None);
        assert!(target.attached());
    }

    #[test]
    fn failed_target_creation_after_the_swap_keeps_verified_data() {
        let (service, disks, target) = fixture();
        prepared(&service, &disks);
        let operation = OfflineReplacement::from_spare(spec(), SPARE.into());
        *target.fail_next.lock().unwrap() = true;

        // The rename succeeds but target creation fails, so the boot volume
        // must be restored from verified state without losing the clone.
        let result = service.consume_spare_offline(&operation, None);
        assert!(result.is_err());
        assert!(!target.attached());

        service.recover_offline(&operation).unwrap();
        assert_eq!(
            disks.content("pool/clients/pc001").as_deref(),
            Some("pool/master@ready")
        );
        assert_eq!(disks.content(SPARE), None);
        assert!(target.attached());
        assert_eq!(
            disks.volumes.lock().unwrap().len(),
            2,
            "only the source snapshot and the client volume survive"
        );
    }

    #[test]
    fn failure_before_the_swap_leaves_the_spare_for_the_next_attempt() {
        let (service, disks, target) = fixture();
        prepared(&service, &disks);
        let operation = OfflineReplacement::from_spare(spec(), SPARE.into());
        *target.connected.lock().unwrap() = true;

        assert!(service.consume_spare_offline(&operation, None).is_err());
        assert_eq!(disks.content("pool/clients/pc001"), None);
        assert!(disks.content(SPARE).is_some());
    }

    #[test]
    fn preparation_rejects_unsafe_sources_and_cleans_up_failures() {
        let (service, disks, _) = fixture();
        assert!(service
            .prepare_spare_clone("pool/clients/pc001-spare-other", SPARE)
            .is_err());
        assert!(service
            .prepare_spare_clone("pool/master@ready", "pool/clients/pc001")
            .is_err());
        assert!(service
            .prepare_spare_clone("pool/missing@ready", SPARE)
            .is_err());
        assert!(service
            .prepare_spare_clone("pool/master@ready", "pool/clients/pc001-spare-twice")
            .is_ok());
        assert!(service
            .prepare_spare_clone("pool/master@ready", "pool/clients/pc001-spare-twice")
            .is_err());

        // A clone that cannot be verified is removed rather than left behind.
        let unverifiable = "pool/clients/pc001-spare-broken";
        disks
            .unverifiable
            .lock()
            .unwrap()
            .insert(unverifiable.into());
        assert!(service
            .prepare_spare_clone("pool/master@ready", unverifiable)
            .is_err());
        assert_eq!(disks.content(unverifiable), None);
    }
}
