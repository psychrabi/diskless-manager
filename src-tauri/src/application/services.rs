use std::sync::Arc;

use super::ClientService;
use super::NvmeOfBootService;
use super::ProvisioningService;
use super::StorageService;
use crate::persistence::{BootLogRepository, ClientRepository};
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
    pub clients: ClientService,
    pub boot_logs: BootLogRepository,
    pub storage: Arc<StorageService>,
    pub provisioning: ProvisioningService,
    pub nvmeof_boot: NvmeOfBootService,
}

impl ApplicationServices {
    pub fn new(pool: SqlitePool) -> Self {
        let image_backend: Arc<dyn ImageBackend> = Arc::new(ZfsImageBackend::new());

        let iscsi: Arc<dyn IscsiProvisioner> = Arc::new(SafeIscsiProvisioner::new());

        let storage = Arc::new(StorageService::new(image_backend, iscsi));
        let boot_logs = BootLogRepository::new(pool.clone());
        let clients = ClientRepository::new(pool);
        let client_service = ClientService::new(clients.clone());
        let provisioning =
            ProvisioningService::new(storage.clone(), clients.clone(), Arc::new(IscDhcpPublisher));
        let nvmeof_boot = NvmeOfBootService::new(clients);

        Self {
            clients: client_service,
            boot_logs,
            storage,
            provisioning,
            nvmeof_boot,
        }
    }
}
