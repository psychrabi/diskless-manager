use crate::domain::{BootLogEntry, ClientId};
use crate::persistence::BootLogRepository;
use anyhow::Result;

#[derive(Clone)]
pub struct BootHistoryService {
    repository: BootLogRepository,
}

impl BootHistoryService {
    pub fn new(repository: BootLogRepository) -> Self {
        Self { repository }
    }

    pub async fn list_for_client(&self, client_id: &str, limit: i32) -> Result<Vec<BootLogEntry>> {
        let client_id = ClientId::from_string(client_id.to_owned())?;
        self.repository.list_for_client(&client_id, limit).await
    }
}
