use crate::{
    domain::{storage::ClientStorageSpec, Client, CreateClient},
    infrastructure::dhcp::{BootReservation, BootReservationPublisher},
    persistence::ClientRepository,
};
use anyhow::{bail, Context, Result};
use std::sync::Arc;

use super::StorageService;

/// Authoritative client creation transaction.
///
/// Infrastructure is created before persistence and is removed if the
/// database write fails. Existing/shared ZFS volumes remain protected by the
/// ownership semantics carried in `ClientStorage`.
#[derive(Clone)]
pub struct ProvisioningService {
    storage: Arc<StorageService>,
    clients: ClientRepository,
    boot: Arc<dyn BootReservationPublisher>,
}

impl ProvisioningService {
    pub fn new(
        storage: Arc<StorageService>,
        clients: ClientRepository,
        boot: Arc<dyn BootReservationPublisher>,
    ) -> Self {
        Self {
            storage,
            clients,
            boot,
        }
    }

    pub async fn create_client(
        &self,
        request: CreateClient,
        storage_spec: ClientStorageSpec,
        server_ip: &str,
    ) -> Result<Client> {
        let service = self.clone();
        let server_ip = server_ip.to_owned();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            runtime.block_on(service.create_client_blocking(request, storage_spec, &server_ip))
        })
        .await
        .context("client provisioning worker failed")?
    }

    async fn create_client_blocking(
        &self,
        request: CreateClient,
        mut storage_spec: ClientStorageSpec,
        server_ip: &str,
    ) -> Result<Client> {
        let mut client = Client::create(request)?;

        if self.clients.exists_by_name(&client.name).await? {
            bail!("client name already exists: {}", client.name);
        }
        if self.clients.exists_by_mac(&client.mac).await? {
            bail!("client MAC address already exists: {}", client.mac);
        }
        if self.clients.exists_by_ip(&client.ip).await? {
            bail!("client IP address already exists: {}", client.ip);
        }

        storage_spec.client_id = client.id.to_string();
        let storage = self
            .storage
            .create_client_storage(&storage_spec)
            .with_context(|| format!("failed to provision storage for client '{}'", client.name))?;

        client.block_store = Some(storage.backstore().to_string());
        client.block_device = Some(storage.block_device().display().to_string());
        client.target_iqn = Some(storage.target_iqn().to_string());
        // Persist the writeback dataset so inspection and repair can
        // rebuild the spec. Only snapshot clones own one. Note
        // `block_store` is the LIO backstore NAME, not a device path:
        // reconciliation rebuilds LUN specs from it.
        if matches!(
            storage.source,
            crate::domain::storage::StorageSource::Snapshot(_)
        ) {
            client.writeback = Some(storage.dataset().to_string());
        }
        if let Some(chap) = &storage_spec.chap {
            client.chap_user = Some(chap.username.clone());
            client.chap_secret = Some(chap.password.clone());
            client.chap_enabled = true;
        }

        client.mark_ready();

        let reservation = BootReservation {
            client_name: client.name.clone(),
            mac: client.mac.to_string(),
            ip: client.ip.to_string(),
            target_iqn: storage.target_iqn().to_string(),
            server_ip: server_ip.to_string(),
            // The provisioned menu must carry the same credentials the
            // target enforces, or the first boot cannot log in.
            chap: storage_spec.chap.clone(),
        };
        if let Err(error) = self.boot.publish(&reservation).await {
            let rollback = self.storage.destroy_client_storage(&storage);
            return match rollback {
                Ok(()) => Err(error).context("failed to publish client boot reservation"),
                Err(rollback_error) => Err(anyhow::anyhow!(
                    "failed to publish client boot reservation: {error}; storage rollback also failed: {rollback_error}"
                )),
            };
        }

        if let Err(error) = self.clients.insert(&client).await {
            let boot_rollback = self.boot.remove(&reservation).await.err();
            let storage_rollback = self.storage.destroy_client_storage(&storage).err();
            let mut message = format!("failed to persist provisioned client: {error}");
            if let Some(error) = boot_rollback {
                message.push_str(&format!(
                    "; boot configuration rollback also failed: {error:#}"
                ));
            }
            if let Some(error) = storage_rollback {
                message.push_str(&format!("; storage rollback also failed: {error}"));
            }
            bail!(message);
        }

        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{storage::StorageSource, PxeMode},
        infrastructure::{
            dhcp::{BootReservation, BootReservationPublisher},
            image::{ImageBackend, ImageBackendInfo},
            iscsi::{
                ChapCredentials, IscsiLunState, IscsiProvisioner, IscsiTargetSpec, IscsiTargetState,
            },
        },
    };
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct UnexpectedImageBackend;
    impl ImageBackend for UnexpectedImageBackend {
        fn exists(&self, _: &str) -> Result<bool> {
            panic!("storage must not be touched")
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn destroy(&self, _: &str) -> Result<()> {
            unreachable!()
        }
        fn rename(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn clone_image(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn verify(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            unreachable!()
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn image_parent(&self) -> Result<String> {
            unreachable!()
        }
    }

    struct UnexpectedIscsi;
    impl IscsiProvisioner for UnexpectedIscsi {
        fn create_target(&self, _: &IscsiTargetSpec) -> Result<()> {
            panic!("storage must not be touched")
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            unreachable!()
        }
        fn remove_target_with_backstores(&self, _: &str, _: &[String]) -> Result<()> {
            bail!("target has active sessions")
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> Result<()> {
            unreachable!()
        }
        fn list_target_luns(&self, _: &str) -> Result<Vec<IscsiLunState>> {
            unreachable!()
        }
        fn inspect_target(&self, _: &IscsiTargetSpec) -> Result<IscsiTargetState> {
            unreachable!()
        }
        fn reconcile(&self, _: &IscsiTargetSpec) -> Result<()> {
            unreachable!()
        }
    }

    struct UnexpectedBootPublisher;
    impl BootReservationPublisher for UnexpectedBootPublisher {
        fn publish<'a>(
            &'a self,
            _: &'a BootReservation,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
            panic!("DHCP must not be touched")
        }

        fn remove<'a>(
            &'a self,
            _: &'a BootReservation,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
            unreachable!()
        }
    }

    struct ExistingVolumeBackend;
    impl ImageBackend for ExistingVolumeBackend {
        fn exists(&self, _: &str) -> Result<bool> {
            Ok(true)
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn destroy(&self, _: &str) -> Result<()> {
            panic!("shared volume must be preserved")
        }
        fn rename(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn clone_image(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn verify(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            unreachable!()
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn image_parent(&self) -> Result<String> {
            unreachable!()
        }
    }

    struct RecordingIscsi(Arc<AtomicBool>);
    impl IscsiProvisioner for RecordingIscsi {
        fn create_target(&self, _: &IscsiTargetSpec) -> Result<()> {
            Ok(())
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            unreachable!()
        }
        fn remove_target_with_backstores(&self, _: &str, _: &[String]) -> Result<()> {
            self.0.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            // Prune short-circuits on a missing target.
            Ok(false)
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> Result<()> {
            Ok(())
        }
        fn list_target_luns(&self, _: &str) -> Result<Vec<IscsiLunState>> {
            Ok(Vec::new())
        }
        fn inspect_target(&self, _: &IscsiTargetSpec) -> Result<IscsiTargetState> {
            unreachable!()
        }
        fn reconcile(&self, _: &IscsiTargetSpec) -> Result<()> {
            unreachable!()
        }
    }

    struct RecordingBootPublisher {
        published: Arc<AtomicBool>,
        removed: Arc<AtomicBool>,
    }
    impl BootReservationPublisher for RecordingBootPublisher {
        fn publish<'a>(
            &'a self,
            _: &'a BootReservation,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                self.published.store(true, Ordering::SeqCst);
                Ok(())
            })
        }
        fn remove<'a>(
            &'a self,
            reservation: &'a BootReservation,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                assert_eq!(reservation.client_name, "PC-01");
                assert_eq!(reservation.mac, "00:11:22:33:44:66");
                self.removed.store(true, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    fn request_and_spec() -> (CreateClient, ClientStorageSpec) {
        (
            CreateClient {
                name: "PC-01".into(),
                mac: "00:11:22:33:44:66".into(),
                ip: "192.168.1.101".into(),
                master: "tank/win11".into(),
                snapshot: None,
                block_store: None,
                block_device: None,
                target_iqn: None,
                pxe_mode: PxeMode::Uefi,
                keep_writeback: true,
                use_game_disk: false,
                game_disks: Vec::new(),
                chap_enabled: false,
            },
            ClientStorageSpec {
                client_id: "ignored".into(),
                source: StorageSource::ExistingVolume("tank/win11".into()),
                dataset: "tank/win11".into(),
                backstore: "block_pc01".into(),
                target_iqn: "iqn.test:pc01".into(),
                lun: 0,
                use_game_disk: false,
                game_disks: Vec::new(),
                chap: None,
            },
        )
    }

    #[tokio::test]
    async fn duplicate_identity_is_rejected_before_infrastructure_changes() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE clients (name TEXT NOT NULL, mac TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO clients VALUES ('PC-01', '00:11:22:33:44:55')")
            .execute(&pool)
            .await
            .unwrap();
        let storage = Arc::new(StorageService::new(
            Arc::new(UnexpectedImageBackend),
            Arc::new(UnexpectedIscsi),
        ));
        let service = ProvisioningService::new(
            storage,
            ClientRepository::new(pool),
            Arc::new(UnexpectedBootPublisher),
        );
        let request = CreateClient {
            name: "PC-01".into(),
            mac: "00:11:22:33:44:66".into(),
            ip: "192.168.1.101".into(),
            master: "tank/win11".into(),
            snapshot: None,
            block_store: None,
            block_device: None,
            target_iqn: None,
            pxe_mode: PxeMode::Uefi,
            keep_writeback: true,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap_enabled: false,
        };
        let spec = ClientStorageSpec {
            client_id: "ignored".into(),
            source: StorageSource::ExistingVolume("tank/win11".into()),
            dataset: "tank/win11".into(),
            backstore: "block_pc01".into(),
            target_iqn: "iqn.test:pc01".into(),
            lun: 0,
            use_game_disk: false,
            game_disks: Vec::new(),
            chap: None,
        };

        let error = service
            .create_client(request, spec, "192.168.1.250")
            .await
            .expect_err("duplicate must fail");
        assert!(error.to_string().contains("name already exists"));
    }

    #[tokio::test]
    async fn duplicate_ip_is_rejected_before_infrastructure_changes() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE clients (name TEXT NOT NULL, mac TEXT NOT NULL, ip TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO clients VALUES ('OTHER-PC', '00:11:22:33:44:55', '192.168.1.101')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let service = ProvisioningService::new(
            Arc::new(StorageService::new(
                Arc::new(UnexpectedImageBackend),
                Arc::new(UnexpectedIscsi),
            )),
            ClientRepository::new(pool.clone()),
            Arc::new(UnexpectedBootPublisher),
        );
        let (request, spec) = request_and_spec();
        let error = service
            .create_client(request, spec, "192.168.1.250")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("IP address already exists"),
            "{error:#}"
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clients")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn persistence_failure_removes_boot_and_iscsi_but_preserves_shared_volume() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE clients (name TEXT NOT NULL, mac TEXT NOT NULL, ip TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let iscsi_removed = Arc::new(AtomicBool::new(false));
        let boot_published = Arc::new(AtomicBool::new(false));
        let boot_removed = Arc::new(AtomicBool::new(false));
        let storage = Arc::new(StorageService::new(
            Arc::new(ExistingVolumeBackend),
            Arc::new(RecordingIscsi(iscsi_removed.clone())),
        ));
        let service = ProvisioningService::new(
            storage,
            ClientRepository::new(pool),
            Arc::new(RecordingBootPublisher {
                published: boot_published.clone(),
                removed: boot_removed.clone(),
            }),
        );
        let (request, spec) = request_and_spec();

        let error = service
            .create_client(request, spec, "192.168.1.250")
            .await
            .expect_err("incomplete persistence schema must fail");

        assert!(error.to_string().contains("failed to persist"));
        assert!(boot_published.load(Ordering::SeqCst));
        assert!(boot_removed.load(Ordering::SeqCst));
        assert!(iscsi_removed.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn audit_credentials_are_inserted_atomically_and_redacted() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("CREATE TRIGGER no_credential_update BEFORE UPDATE OF chap_user ON clients BEGIN SELECT RAISE(FAIL, 'credentials must be inserted'); END")
            .execute(&pool).await.unwrap();
        let service = ProvisioningService::new(
            Arc::new(StorageService::new(
                Arc::new(ExistingVolumeBackend),
                Arc::new(RecordingIscsi(Arc::new(AtomicBool::new(false)))),
            )),
            ClientRepository::new(pool.clone()),
            Arc::new(RecordingBootPublisher {
                published: Arc::new(AtomicBool::new(false)),
                removed: Arc::new(AtomicBool::new(false)),
            }),
        );
        let (mut request, mut spec) = request_and_spec();
        request.chap_enabled = true;
        spec.chap = Some(ChapCredentials {
            username: "test-user".into(),
            password: "test-secret".into(),
        });
        let client = service
            .create_client(request, spec, "192.168.1.250")
            .await
            .unwrap();
        let credentials = service.clients.chap_credentials(&client.id).await.unwrap();
        assert_eq!(
            credentials,
            (client.chap_user.clone(), client.chap_secret.clone())
        );
        let response = serde_json::to_value(&client).unwrap();
        assert!(response.get("chap_secret").is_none());
        assert!(response.get("chap_user").is_none());
    }

    #[tokio::test]
    async fn audit_game_selection_failure_rolls_back_client_and_infrastructure() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_game BEFORE INSERT ON client_game_disks BEGIN SELECT RAISE(FAIL, 'selection failed'); END").execute(&pool).await.unwrap();
        let removed = Arc::new(AtomicBool::new(false));
        let boot_removed = Arc::new(AtomicBool::new(false));
        let service = ProvisioningService::new(
            Arc::new(StorageService::new(
                Arc::new(ExistingVolumeBackend),
                Arc::new(RecordingIscsi(removed.clone())),
            )),
            ClientRepository::new(pool.clone()),
            Arc::new(RecordingBootPublisher {
                published: Arc::new(AtomicBool::new(false)),
                removed: boot_removed.clone(),
            }),
        );
        let (mut request, spec) = request_and_spec();
        request.game_disks = vec!["tank/games/test".into()];
        assert!(service
            .create_client(request, spec, "192.168.1.250")
            .await
            .is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clients")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert!(removed.load(Ordering::SeqCst));
        assert!(boot_removed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn audit_delete_retains_client_when_storage_mapping_fails() {
        let (mut state, _, _, _) = crate::api::security_tests::setup().await;
        let (mut request, _) = request_and_spec();
        request.master = "invalid".into();
        request.target_iqn = Some("iqn.test:pc01".into());
        let client = Client::create(request).unwrap();
        let repository = ClientRepository::new(state.db_pool.clone());
        repository.insert(&client).await.unwrap();
        let mut application = crate::application::ApplicationServices::new(state.db_pool.clone());
        application.storage = Arc::new(StorageService::new(
            Arc::new(UnexpectedImageBackend),
            Arc::new(UnexpectedIscsi),
        ));
        state.application = Arc::new(application);
        let result = crate::api::handlers::client_delete::delete_client(
            axum::extract::State(state.clone()),
            axum::extract::Path(client.id.to_string()),
        )
        .await;
        assert_eq!(result, Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR));
        assert!(state
            .application
            .clients
            .get_by_string(client.id.as_str())
            .await
            .unwrap()
            .is_some());
    }
    #[tokio::test]
    async fn audit_delete_retains_client_when_target_removal_fails() {
        let (mut state, _, _, _) = crate::api::security_tests::setup().await;
        let (mut request, _) = request_and_spec();
        request.master = format!(
            "{}/image-disk/test",
            crate::config::get_zpool_name(&crate::config::RuntimeConfig::default())
        );
        request.target_iqn = Some("iqn.test:pc01".into());
        let client = Client::create(request).unwrap();
        let repository = ClientRepository::new(state.db_pool.clone());
        repository.insert(&client).await.unwrap();
        let mut application = crate::application::ApplicationServices::new(state.db_pool.clone());
        application.storage = Arc::new(StorageService::new(
            Arc::new(UnexpectedImageBackend),
            Arc::new(UnexpectedIscsi),
        ));
        state.application = Arc::new(application);
        let result = crate::api::handlers::client_delete::delete_client(
            axum::extract::State(state.clone()),
            axum::extract::Path(client.id.to_string()),
        )
        .await;
        assert_eq!(result, Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR));
        assert!(state
            .application
            .clients
            .get_by_string(client.id.as_str())
            .await
            .unwrap()
            .is_some());
    }
}
