use std::sync::Arc;

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
}

impl ApplicationServices {
    pub fn new(pool: SqlitePool) -> Self {
        let image_backend: Arc<dyn ImageBackend> = Arc::new(ZfsImageBackend::new());

        let iscsi: Arc<dyn IscsiProvisioner> = Arc::new(SafeIscsiProvisioner::new());

        let storage = Arc::new(StorageService::new(image_backend, iscsi));
        let provisioning = ProvisioningService::new(
            storage.clone(),
            ClientRepository::new(pool),
            Arc::new(IscDhcpPublisher),
        );

        Self {
            storage,
            provisioning,
        }
    }
}

pub fn build_storage_service() -> StorageService {
    let image_backend = Arc::new(ZfsImageBackend::new());

    let iscsi: Arc<dyn IscsiProvisioner> = Arc::new(SafeIscsiProvisioner::new());

    StorageService::new(image_backend, iscsi)
}
