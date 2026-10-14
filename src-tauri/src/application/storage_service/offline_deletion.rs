use super::*;
use std::collections::HashSet;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OwnedGameClone {
    pub master_dataset: String,
    pub clone_dataset: String,
}

pub trait DeletionSafety {
    fn target_connected(&self, target_iqn: &str) -> Result<bool>;
    fn device_exported(&self, device: &std::path::Path) -> Result<bool>;
    fn owned_game_clones(&self, client_id: &str) -> Result<Vec<OwnedGameClone>>;
}

struct SystemDeletionSafety<'a>(&'a StorageService);

impl DeletionSafety for SystemDeletionSafety<'_> {
    fn target_connected(&self, target_iqn: &str) -> Result<bool> {
        if !self.0.iscsi.target_exists(target_iqn)? {
            return Ok(false);
        }
        crate::infrastructure::iscsi::reconcile::confirmed_target_connected(target_iqn)
    }

    fn device_exported(&self, device: &std::path::Path) -> Result<bool> {
        crate::infrastructure::nvmeof::block_device_is_exported(device).map_err(Into::into)
    }

    fn owned_game_clones(&self, client_id: &str) -> Result<Vec<OwnedGameClone>> {
        Ok(self
            .0
            .list_game_clones()?
            .into_iter()
            .filter(|record| record.client_id == client_id)
            .map(|record| OwnedGameClone {
                master_dataset: record.master_dataset,
                clone_dataset: record.clone_dataset,
            })
            .collect())
    }
}

/// Durable progress for removing one client's writable storage.
///
/// The caller must persist this value before starting deletion and after each
/// successful stage. It contains only owned datasets, never their masters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OfflineDeletion {
    pub spec: ClientStorageSpec,
    pub game_clones: Vec<OwnedGameClone>,
    #[serde(default)]
    pub removed_datasets: Vec<String>,
}

impl OfflineDeletion {
    pub fn new(spec: ClientStorageSpec, game_clones: Vec<OwnedGameClone>) -> Self {
        let mut spec = spec;
        spec.chap = None;
        Self {
            spec,
            game_clones,
            removed_datasets: Vec::new(),
        }
    }
}

impl StorageService {
    /// Check ownership, origins, mounts, sessions and exports without deleting.
    pub fn ensure_offline_deletion(&self, operation: &OfflineDeletion) -> Result<()> {
        self.validate_deletion(operation, &SystemDeletionSafety(self))?;
        for dataset in std::iter::once(&operation.spec.dataset).chain(
            operation
                .game_clones
                .iter()
                .map(|clone| &clone.clone_dataset),
        ) {
            if self.image_backend.exists(dataset)? {
                self.image_backend.validate_unmounted(dataset)?;
            }
        }
        Ok(())
    }

    /// Treat a missing target as offline; an existing target with unknown
    /// session state remains an error.
    pub fn target_connected_if_present(&self, target_iqn: &str) -> Result<bool> {
        SystemDeletionSafety(self).target_connected(target_iqn)
    }

    /// Delete the iSCSI target and only the client-owned boot/game clones.
    /// Session and NVMe state is checked before each destructive step.
    pub fn delete_offline(&self, operation: &mut OfflineDeletion) -> Result<()> {
        self.recover_deletion_with_safety(operation, &SystemDeletionSafety(self))
    }

    /// Resume an interrupted deletion. Missing resources count as completed.
    pub fn recover_deletion_with_safety(
        &self,
        operation: &mut OfflineDeletion,
        safety: &dyn DeletionSafety,
    ) -> Result<()> {
        operation.spec.chap = None;
        self.validate_deletion(operation, safety)?;
        let spec = operation.spec.clone();
        self.remove_target_if_present(operation, safety)?;

        let datasets: Vec<String> = std::iter::once(spec.dataset.clone())
            .chain(
                operation
                    .game_clones
                    .iter()
                    .map(|clone| clone.clone_dataset.clone()),
            )
            .collect();
        for dataset in &datasets {
            if operation.removed_datasets.contains(dataset) {
                continue;
            }
            self.ensure_offline(&spec, &operation.game_clones, safety)?;
            self.remove_target_if_present(operation, safety)?;
            if self.iscsi.target_exists(&spec.target_iqn)? {
                bail!(
                    "target '{}' reappeared before dataset deletion",
                    spec.target_iqn
                );
            }
            if self.image_backend.exists(dataset)? {
                self.image_backend.validate_unmounted(dataset)?;
                self.image_backend
                    .destroy(dataset)
                    .with_context(|| format!("failed to destroy client dataset '{dataset}'"))?;
            }
            operation.removed_datasets.push(dataset.clone());
        }
        Ok(())
    }

    fn validate_deletion(
        &self,
        operation: &OfflineDeletion,
        safety: &dyn DeletionSafety,
    ) -> Result<()> {
        let spec = &operation.spec;
        self.validate_spec(spec)?;
        let StorageSource::Snapshot(snapshot) = &spec.source else {
            bail!("offline deletion requires snapshot-backed client storage");
        };
        if snapshot.split('@').next() == Some(spec.dataset.as_str()) {
            bail!("refusing to delete a master volume");
        }
        if self.image_backend.exists(&spec.dataset)?
            && self.image_backend.clone_origin(&spec.dataset)?.as_deref() != Some(snapshot)
        {
            bail!("client dataset origin does not match its configured snapshot");
        }
        let requested: HashSet<&str> = operation
            .game_clones
            .iter()
            .map(|clone| clone.clone_dataset.as_str())
            .collect();
        if requested.len() != operation.game_clones.len() {
            bail!("duplicate game clone dataset in deletion journal");
        }
        for clone in &operation.game_clones {
            if clone.clone_dataset == spec.dataset
                || clone.clone_dataset == clone.master_dataset
                || clone.clone_dataset != game_clone_dataset(&spec.client_id, &clone.master_dataset)
            {
                bail!("game clone journal contains an invalid owned dataset identity");
            }
        }
        let owned: std::collections::HashMap<String, String> = safety
            .owned_game_clones(&spec.client_id)?
            .into_iter()
            .map(|clone| (clone.clone_dataset, clone.master_dataset))
            .collect();
        for clone in &operation.game_clones {
            if self.image_backend.exists(&clone.clone_dataset)? {
                if owned.get(&clone.clone_dataset) != Some(&clone.master_dataset) {
                    bail!("deletion journal game clone ownership does not match ZFS metadata");
                }
                let expected_origin =
                    format!("{}@{}", clone.master_dataset, Self::GAME_BASE_SNAPSHOT);
                if self
                    .image_backend
                    .clone_origin(&clone.clone_dataset)?
                    .as_deref()
                    != Some(expected_origin.as_str())
                {
                    bail!("game clone origin does not match its recorded master");
                }
            }
        }
        self.ensure_offline(spec, &operation.game_clones, safety)?;
        Ok(())
    }

    fn remove_target_if_present(
        &self,
        operation: &mut OfflineDeletion,
        safety: &dyn DeletionSafety,
    ) -> Result<()> {
        let spec = &operation.spec;
        let target_exists = self.iscsi.target_exists(&spec.target_iqn)?;
        self.ensure_offline(spec, &operation.game_clones, safety)?;
        let mut backstores = vec![spec.backstore.clone()];
        backstores.extend(
            operation
                .game_clones
                .iter()
                .map(|clone| Self::game_backstore_name(&spec.client_id, &clone.master_dataset)),
        );
        backstores.sort();
        backstores.dedup();
        let expected: HashSet<&str> = backstores.iter().map(String::as_str).collect();
        if target_exists
            && self
                .iscsi
                .list_target_backstores(&spec.target_iqn)?
                .iter()
                .any(|backstore| !expected.contains(backstore.as_str()))
        {
            bail!("client target contains a backstore not owned by this deletion journal");
        }
        for dataset in std::iter::once(&spec.dataset).chain(
            operation
                .game_clones
                .iter()
                .map(|clone| &clone.clone_dataset),
        ) {
            if self.image_backend.exists(dataset)? {
                self.image_backend.validate_unmounted(dataset)?;
            }
        }
        self.iscsi
            .remove_target_with_backstores(&spec.target_iqn, &backstores)
            .with_context(|| format!("failed to remove target '{}'", spec.target_iqn))?;
        if target_exists {
            self.iscsi
                .remove_target(&spec.target_iqn)
                .with_context(|| format!("failed to remove target '{}'", spec.target_iqn))?;
        }
        if self.iscsi.target_exists(&spec.target_iqn)? {
            bail!("target '{}' remains after deletion", spec.target_iqn);
        }
        Ok(())
    }

    fn ensure_offline(
        &self,
        spec: &ClientStorageSpec,
        clones: &[OwnedGameClone],
        safety: &dyn DeletionSafety,
    ) -> Result<()> {
        if safety.target_connected(&spec.target_iqn)? {
            return Err(
                crate::application::client_lifecycle::PendingRecoverySafetyError::ClientConnected
                    .into(),
            );
        }
        for dataset in
            std::iter::once(&spec.dataset).chain(clones.iter().map(|clone| &clone.clone_dataset))
        {
            let device = PathBuf::from(format!("/dev/zvol/{dataset}"));
            if safety.device_exported(&device)? {
                return Err(
                    crate::application::client_lifecycle::PendingRecoverySafetyError::NvmeExported
                        .into(),
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn spec() -> ClientStorageSpec {
        ClientStorageSpec {
            client_id: "client-1".into(),
            client_name: "client-1".into(),
            source: StorageSource::Snapshot("pool/master@ready".into()),
            dataset: "pool/clients/client-1".into(),
            backstore: "client-1".into(),
            target_iqn: "iqn.test:client-1".into(),
            lun: 0,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap: None,
        }
    }

    #[test]
    fn journal_round_trips_and_defaults_old_progress_fields() {
        let journal = OfflineDeletion::new(
            spec(),
            vec![OwnedGameClone {
                master_dataset: "pool/games/game".into(),
                clone_dataset: "pool/games/client-1-game-games".into(),
            }],
        );
        let value = serde_json::to_value(&journal).unwrap();
        assert_eq!(
            serde_json::from_value::<OfflineDeletion>(value)
                .unwrap()
                .game_clones,
            journal.game_clones
        );
        let old = serde_json::json!({"spec": spec(), "game_clones": [], "target_removed": true});
        let recovered: OfflineDeletion = serde_json::from_value(old).unwrap();
        assert!(recovered.removed_datasets.is_empty());
    }

    struct Guard {
        connected: Result<bool>,
        exported: Result<bool>,
        owned: Vec<OwnedGameClone>,
    }

    impl Guard {
        fn offline(owned: Vec<OwnedGameClone>) -> Self {
            Self {
                connected: Ok(false),
                exported: Ok(false),
                owned,
            }
        }
    }

    impl DeletionSafety for Guard {
        fn target_connected(&self, _: &str) -> Result<bool> {
            self.connected
                .as_ref()
                .copied()
                .map_err(|error| anyhow::anyhow!(error.to_string()))
        }

        fn device_exported(&self, _: &std::path::Path) -> Result<bool> {
            self.exported
                .as_ref()
                .copied()
                .map_err(|error| anyhow::anyhow!(error.to_string()))
        }

        fn owned_game_clones(&self, _: &str) -> Result<Vec<OwnedGameClone>> {
            Ok(self.owned.clone())
        }
    }

    #[test]
    fn deletion_guard_fails_closed_for_sessions_nvme_and_unknown_state() {
        let safety_cases = [
            (Ok(true), Ok(false)),
            (Ok(false), Ok(true)),
            (Err(anyhow::anyhow!("session state unavailable")), Ok(false)),
            (Ok(false), Err(anyhow::anyhow!("NVMe state unavailable"))),
        ];
        for (connected, exported) in safety_cases {
            let guard = Guard {
                connected,
                exported,
                owned: Vec::new(),
            };
            let service = StorageService::new(
                std::sync::Arc::new(NoopImage),
                std::sync::Arc::new(NoopIscsi),
            );
            assert!(service.ensure_offline(&spec(), &[], &guard).is_err());
        }
    }

    #[test]
    fn deletion_guard_returns_typed_session_and_export_safety_errors() {
        use crate::application::client_lifecycle::PendingRecoverySafetyError;

        let service = StorageService::new(
            std::sync::Arc::new(NoopImage),
            std::sync::Arc::new(NoopIscsi),
        );
        let connected = Guard {
            connected: Ok(true),
            exported: Ok(false),
            owned: Vec::new(),
        };
        assert!(service
            .ensure_offline(&spec(), &[], &connected)
            .unwrap_err()
            .downcast_ref::<PendingRecoverySafetyError>()
            .is_some_and(|error| matches!(error, PendingRecoverySafetyError::ClientConnected)));

        let exported = Guard {
            connected: Ok(false),
            exported: Ok(true),
            owned: Vec::new(),
        };
        assert!(service
            .ensure_offline(&spec(), &[], &exported)
            .unwrap_err()
            .downcast_ref::<PendingRecoverySafetyError>()
            .is_some_and(|error| matches!(error, PendingRecoverySafetyError::NvmeExported)));
    }

    #[test]
    fn repeated_deletion_succeeds_when_resources_are_already_absent() {
        let service = StorageService::new(
            std::sync::Arc::new(NoopImage),
            std::sync::Arc::new(NoopIscsi),
        );
        let safety = Guard {
            connected: Ok(false),
            exported: Ok(false),
            owned: Vec::new(),
        };
        let mut operation = OfflineDeletion::new(spec(), Vec::new());
        service
            .recover_deletion_with_safety(&mut operation, &safety)
            .unwrap();
        service
            .recover_deletion_with_safety(&mut operation, &safety)
            .unwrap();
        assert_eq!(operation.removed_datasets, ["pool/clients/client-1"]);
    }

    #[test]
    fn partial_delete_resumes_safely_if_target_is_recreated() {
        let master = "pool/games/steam-games";
        let game_clone = OwnedGameClone {
            master_dataset: master.into(),
            clone_dataset: game_clone_dataset("client-1", master),
        };
        let image = Arc::new(FakeImage::default());
        image.add("pool/clients/client-1", "pool/master@ready");
        image.add(
            &game_clone.clone_dataset,
            &format!("{master}@{}", StorageService::GAME_BASE_SNAPSHOT),
        );
        image.add(master, "");
        image.fail_once_on(&game_clone.clone_dataset);

        let iscsi = Arc::new(FakeIscsi::default());
        iscsi.attach(vec![
            "client-1".into(),
            StorageService::game_backstore_name("client-1", master),
        ]);
        let service = StorageService::new(image.clone(), iscsi.clone());
        let safety = Guard {
            connected: Ok(false),
            exported: Ok(false),
            owned: vec![game_clone.clone()],
        };
        let mut operation = OfflineDeletion::new(spec(), vec![game_clone.clone()]);

        assert!(service
            .recover_deletion_with_safety(&mut operation, &safety)
            .is_err());
        assert_eq!(operation.removed_datasets, ["pool/clients/client-1"]);

        iscsi.attach(vec![
            "client-1".into(),
            StorageService::game_backstore_name("client-1", master),
        ]);
        service
            .recover_deletion_with_safety(&mut operation, &safety)
            .unwrap();

        assert!(!iscsi.is_attached());
        assert!(!image.contains("pool/clients/client-1"));
        assert!(!image.contains(&game_clone.clone_dataset));
        assert!(
            image.contains(master),
            "master dataset must survive deletion"
        );
    }

    #[test]
    fn deletion_rejects_non_snapshot_ownership_and_wrong_boot_origin() {
        let image = Arc::new(FakeImage::default());
        image.add("pool/clients/client-1", "pool/other-client@ready");
        let iscsi = Arc::new(FakeIscsi::default());
        iscsi.attach(vec!["client-1".into()]);
        let service = StorageService::new(image.clone(), iscsi.clone());
        let safety = Guard::offline(Vec::new());

        let mut operation = OfflineDeletion::new(spec(), Vec::new());
        operation.spec.source = StorageSource::ExistingClientVolume("pool/clients/client-1".into());
        assert!(service
            .recover_deletion_with_safety(&mut operation, &safety)
            .is_err());
        assert!(iscsi.is_attached());
        assert!(image.contains("pool/clients/client-1"));

        let mut operation = OfflineDeletion::new(spec(), Vec::new());
        assert!(service
            .recover_deletion_with_safety(&mut operation, &safety)
            .is_err());
        assert!(iscsi.is_attached());
        assert!(image.contains("pool/clients/client-1"));
    }

    #[test]
    fn deletion_refuses_foreign_target_backstores() {
        let image = Arc::new(FakeImage::default());
        image.add("pool/clients/client-1", "pool/master@ready");
        let iscsi = Arc::new(FakeIscsi::default());
        iscsi.attach(vec!["client-1".into(), "another-clients-disk".into()]);
        let service = StorageService::new(image.clone(), iscsi.clone());
        let mut operation = OfflineDeletion::new(spec(), Vec::new());

        assert!(service
            .recover_deletion_with_safety(&mut operation, &Guard::offline(Vec::new()))
            .is_err());
        assert!(iscsi.is_attached());
        assert!(image.contains("pool/clients/client-1"));
    }

    #[test]
    fn absent_target_still_cleans_residual_backstores_before_datasets() {
        let master = "pool/games/steam-games";
        let game_clone = OwnedGameClone {
            master_dataset: master.into(),
            clone_dataset: game_clone_dataset("client-1", master),
        };
        let events = Arc::new(Mutex::new(Vec::new()));
        let image = Arc::new(FakeImage::with_events(Arc::clone(&events)));
        image.add("pool/clients/client-1", "pool/master@ready");
        image.add(
            &game_clone.clone_dataset,
            &format!("{master}@{}", StorageService::GAME_BASE_SNAPSHOT),
        );
        image.add(master, "");

        let iscsi = Arc::new(FakeIscsi::with_residual_backstores(
            Arc::clone(&events),
            vec![
                "client-1".into(),
                StorageService::game_backstore_name("client-1", master),
            ],
        ));
        let service = StorageService::new(image.clone(), iscsi.clone());
        let safety = Guard::offline(vec![game_clone.clone()]);
        let mut operation = OfflineDeletion::new(spec(), vec![game_clone]);

        service
            .recover_deletion_with_safety(&mut operation, &safety)
            .unwrap();

        let events = events.lock().unwrap();
        let first_dataset_destroy = events
            .iter()
            .position(|event| event.starts_with("destroy:"))
            .unwrap();
        assert!(events[..first_dataset_destroy]
            .iter()
            .any(|event| event == "backstores:client-1"));
        assert!(events[..first_dataset_destroy]
            .iter()
            .any(|event| event.starts_with("backstores:game_")));
        assert!(!iscsi.is_attached());
        assert!(!image.contains("pool/clients/client-1"));
        assert!(!image.contains(&operation.game_clones[0].clone_dataset));
        assert!(image.contains(master));
    }

    struct NoopImage;
    impl ImageBackend for NoopImage {
        fn validate_unmounted(&self, _: &str) -> Result<()> {
            Ok(())
        }

        fn exists(&self, _: &str) -> Result<bool> {
            Ok(false)
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn destroy(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn clone_image(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn import_raw(&self, _: &std::path::Path, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn verify(&self, _: &str) -> Result<bool> {
            Ok(false)
        }
        fn info(&self, _: &str) -> Result<crate::infrastructure::image::ImageBackendInfo> {
            bail!("unused")
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn image_parent(&self) -> Result<String> {
            Ok("pool".into())
        }
    }

    struct NoopIscsi;
    impl IscsiProvisioner for NoopIscsi {
        fn create_target(&self, _: &IscsiTargetSpec) -> Result<()> {
            Ok(())
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn remove_target_with_backstores(&self, _: &str, _: &[String]) -> Result<()> {
            Ok(())
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            Ok(false)
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> Result<()> {
            Ok(())
        }
        fn list_target_luns(
            &self,
            _: &str,
        ) -> Result<Vec<crate::infrastructure::iscsi::IscsiLunState>> {
            Ok(Vec::new())
        }
        fn inspect_target(
            &self,
            _: &IscsiTargetSpec,
        ) -> Result<crate::infrastructure::iscsi::IscsiTargetState> {
            bail!("unused")
        }
        fn reconcile(&self, _: &IscsiTargetSpec) -> Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeImage {
        volumes: Mutex<HashMap<String, String>>,
        fail_destroy_once: Mutex<Option<String>>,
        events: Option<Arc<Mutex<Vec<String>>>>,
    }

    impl FakeImage {
        fn with_events(events: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                events: Some(events),
                ..Self::default()
            }
        }

        fn add(&self, dataset: &str, origin: &str) {
            self.volumes
                .lock()
                .unwrap()
                .insert(dataset.into(), origin.into());
        }

        fn contains(&self, dataset: &str) -> bool {
            self.volumes.lock().unwrap().contains_key(dataset)
        }

        fn fail_once_on(&self, dataset: &str) {
            *self.fail_destroy_once.lock().unwrap() = Some(dataset.into());
        }
    }

    impl ImageBackend for FakeImage {
        fn validate_unmounted(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn clone_origin(&self, name: &str) -> Result<Option<String>> {
            Ok(self.volumes.lock().unwrap().get(name).cloned())
        }
        fn exists(&self, name: &str) -> Result<bool> {
            Ok(self.contains(name))
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn destroy(&self, name: &str) -> Result<()> {
            let mut fail = self.fail_destroy_once.lock().unwrap();
            if fail.as_deref() == Some(name) {
                *fail = None;
                bail!("injected destroy failure");
            }
            if let Some(events) = &self.events {
                events.lock().unwrap().push(format!("destroy:{name}"));
            }
            self.volumes.lock().unwrap().remove(name);
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn clone_image(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn import_raw(&self, _: &std::path::Path, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn verify(&self, name: &str) -> Result<bool> {
            self.exists(name)
        }
        fn info(&self, _: &str) -> Result<crate::infrastructure::image::ImageBackendInfo> {
            bail!("unused")
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn image_parent(&self) -> Result<String> {
            Ok("pool".into())
        }
    }

    #[derive(Default)]
    struct FakeIscsi {
        state: Mutex<(bool, Vec<String>)>,
        events: Option<Arc<Mutex<Vec<String>>>>,
    }

    impl FakeIscsi {
        fn with_residual_backstores(
            events: Arc<Mutex<Vec<String>>>,
            backstores: Vec<String>,
        ) -> Self {
            Self {
                state: Mutex::new((false, backstores)),
                events: Some(events),
            }
        }

        fn attach(&self, backstores: Vec<String>) {
            *self.state.lock().unwrap() = (true, backstores);
        }

        fn is_attached(&self) -> bool {
            self.state.lock().unwrap().0
        }
    }

    impl IscsiProvisioner for FakeIscsi {
        fn create_target(&self, _: &IscsiTargetSpec) -> Result<()> {
            Ok(())
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            self.state.lock().unwrap().0 = false;
            if let Some(events) = &self.events {
                events.lock().unwrap().push("target:removed".into());
            }
            Ok(())
        }
        fn remove_target_with_backstores(&self, _: &str, backstores: &[String]) -> Result<()> {
            let mut state = self.state.lock().unwrap();
            if let Some(events) = &self.events {
                for backstore in &state.1 {
                    if backstores.contains(backstore) {
                        events
                            .lock()
                            .unwrap()
                            .push(format!("backstores:{backstore}"));
                    }
                }
            }
            state.1.retain(|item| !backstores.contains(item));
            Ok(())
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            Ok(self.is_attached())
        }
        fn list_target_backstores(&self, _: &str) -> Result<Vec<String>> {
            Ok(self.state.lock().unwrap().1.clone())
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> Result<()> {
            Ok(())
        }
        fn list_target_luns(
            &self,
            _: &str,
        ) -> Result<Vec<crate::infrastructure::iscsi::IscsiLunState>> {
            Ok(Vec::new())
        }
        fn inspect_target(
            &self,
            _: &IscsiTargetSpec,
        ) -> Result<crate::infrastructure::iscsi::IscsiTargetState> {
            bail!("unused")
        }
        fn reconcile(&self, _: &IscsiTargetSpec) -> Result<()> {
            Ok(())
        }
    }
}
