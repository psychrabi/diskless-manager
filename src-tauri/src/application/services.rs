use std::sync::Arc;

use super::StorageService;

use crate::infrastructure::{
    image::{ImageBackend, ZfsImageBackend},
    iscsi::{IscsiProvisioner, TargetCliProvisioner},
};

/// Application service container.
///
/// Infrastructure implementations are constructed here and injected
/// into application services.
pub struct ApplicationServices {
    pub storage: StorageService,
}

impl ApplicationServices {
    pub fn new() -> Self {
        let image_backend: Arc<dyn ImageBackend> = Arc::new(ZfsImageBackend::new());

        let iscsi: Arc<dyn IscsiProvisioner> = Arc::new(TargetCliProvisioner::new());

        let storage = StorageService::new(image_backend, iscsi);

        Self { storage }
    }
}

impl Default for ApplicationServices {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_storage_service() -> StorageService {
    let image_backend = Arc::new(ZfsImageBackend::new());

    let iscsi: Arc<dyn IscsiProvisioner> = Arc::new(TargetCliProvisioner::new());

    StorageService::new(image_backend, iscsi)
}
