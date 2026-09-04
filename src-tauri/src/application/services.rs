use std::sync::Arc;

use super::NvmeOfBootService;
use super::ProvisioningService;
use super::StorageService;
use crate::persistence::ClientRepository;
use sqlx::SqlitePool;

use crate::infrastructure::{
    dhcp::IscDhcpPublisher,
    image::{ImageBackend, ZfsImageBackend},
    iscsi::{IscsiProvisioner, SafeIscsiProvisioner},
};

/// Application service container.
///
/// Infrastructure implementations are constructed here and injected
/// into application services.
pub struct ApplicationServices {
    pub storage: Arc<StorageService>,
    pub provisioning: ProvisioningService,
    pub nvmeof_boot: NvmeOfBootService,
}

impl ApplicationServices {
    pub fn new(pool: SqlitePool) -> Self {
        let image_backend: Arc<dyn ImageBackend> = Arc::new(ZfsImageBackend::new());

        let iscsi: Arc<dyn IscsiProvisioner> = Arc::new(SafeIscsiProvisioner::new());

        let storage = Arc::new(StorageService::new(image_backend, iscsi));
        let clients = ClientRepository::new(pool);
        let provisioning = ProvisioningService::new(
            storage.clone(),
            clients.clone(),
            Arc::new(IscDhcpPublisher),
        );
        let nvmeof_boot = NvmeOfBootService::new(clients);

        Self {
            storage,
            provisioning,
            nvmeof_boot,
        }
    }
}

pub fn build_storage_service() -> StorageService {
    let image_backend = Arc::new(ZfsImageBackend::new());

    let iscsi: Arc<dyn IscsiProvisioner> = Arc::new(SafeIscsiProvisioner::new());

    StorageService::new(image_backend, iscsi)
}
