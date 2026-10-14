use super::*;

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
            assert_eq!(backstores, &["block_pc001"]);
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
