use std::sync::Arc;

use super::BootHistoryService;
use super::ClientService;
use super::NvmeOfBootService;
use super::ProvisioningService;
use super::StorageService;
use crate::persistence::{BootLogRepository, ClientRepository};
use sqlx::SqlitePool;

use crate::core::config::IscsiBackend;
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
    pub iscsi_backend: IscsiBackend,
    pub clients: ClientService,
    pub boot_history: BootHistoryService,
    pub storage: Arc<StorageService>,
    pub provisioning: ProvisioningService,
    pub nvmeof_boot: NvmeOfBootService,
}

impl ApplicationServices {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_iscsi_backend(
            pool,
            IscsiBackend::default(),
            crate::config::RuntimeConfig::default(),
        )
    }

    pub fn with_iscsi_backend(
        pool: SqlitePool,
        backend: IscsiBackend,
        config: crate::config::RuntimeConfig,
    ) -> Self {
        let image_backend: Arc<dyn ImageBackend> =
            Arc::new(ZfsImageBackend::with_config(config.clone()));

        let safe_iscsi = SafeIscsiProvisioner::with_backend(backend);
        let iscsi_backend = safe_iscsi.backend();
        let iscsi: Arc<dyn IscsiProvisioner> = Arc::new(safe_iscsi);

        let storage = Arc::new(StorageService::with_config(
            image_backend,
            iscsi,
            config.clone(),
        ));
        let boot_history = BootHistoryService::new(BootLogRepository::new(pool.clone()));
        let clients = ClientRepository::new(pool);
        let client_service = ClientService::new(clients.clone());
        let provisioning = ProvisioningService::new(
            storage.clone(),
            clients.clone(),
            Arc::new(IscDhcpPublisher {
                config: config.clone(),
            }),
        );
        let nvmeof_boot = NvmeOfBootService::new(clients, config);

        Self {
            iscsi_backend,
            clients: client_service,
            boot_history,
            storage,
            provisioning,
            nvmeof_boot,
        }
    }
}
