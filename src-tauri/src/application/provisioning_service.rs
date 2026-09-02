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

        storage_spec.client_id = client.id.to_string();
        let storage = self
            .storage
            .create_client_storage(&storage_spec)
            .with_context(|| format!("failed to provision storage for client '{}'", client.name))?;

        client.block_store = Some(format!("/dev/zvol/{}", storage.dataset()));
        client.block_device = Some(storage.block_device().display().to_string());
        client.target_iqn = Some(storage.target_iqn().to_string());
        client.mark_ready();

        let reservation = BootReservation {
            client_name: client.name.clone(),
            mac: client.mac.to_string(),
            ip: client.ip.to_string(),
            target_iqn: storage.target_iqn().to_string(),
            server_ip: server_ip.to_string(),
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
            let boot_rollback = self.boot.remove(&client.name).await.err();
            let storage_rollback = self.storage.destroy_client_storage(&storage).err();
            let mut message = format!("failed to persist provisioned client: {error}");
            if let Some(error) = boot_rollback {
                message.push_str(&format!("; DHCP rollback also failed: {error}"));
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
            iscsi::{IscsiProvisioner, IscsiTargetSpec, IscsiTargetState},
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
            unreachable!()
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
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
            _: &'a str,
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
            unreachable!()
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
            _: &'a str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
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
            },
            ClientStorageSpec {
                client_id: "ignored".into(),
                source: StorageSource::ExistingVolume("tank/win11".into()),
                dataset: "tank/win11".into(),
                backstore: "block_pc01".into(),
                target_iqn: "iqn.test:pc01".into(),
                lun: 0,
                use_game_disk: false,
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
        };
        let spec = ClientStorageSpec {
            client_id: "ignored".into(),
            source: StorageSource::ExistingVolume("tank/win11".into()),
            dataset: "tank/win11".into(),
            backstore: "block_pc01".into(),
            target_iqn: "iqn.test:pc01".into(),
            lun: 0,
            use_game_disk: false,
        };

        let error = service
            .create_client(request, spec, "192.168.1.250")
            .await
            .expect_err("duplicate must fail");
        assert!(error.to_string().contains("name already exists"));
    }

    #[tokio::test]
    async fn persistence_failure_removes_boot_and_iscsi_but_preserves_shared_volume() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE clients (name TEXT NOT NULL, mac TEXT NOT NULL)")
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
}
