use anyhow::Result;

use super::provider::{ZfsDatasetInfo, ZfsProvider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZfsResourceState {
    Present,
    Missing,
}

#[derive(Debug, Clone)]
pub struct ZfsResourceCheck {
    pub name: String,
    pub state: ZfsResourceState,
}

#[derive(Clone)]
pub struct ZfsReconciler<P>
where
    P: ZfsProvider,
{
    provider: P,
}

impl<P> ZfsReconciler<P>
where
    P: ZfsProvider,
{
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    pub fn check(&self, resource: &str) -> Result<ZfsResourceCheck> {
        let exists = self.provider.exists(resource)?;

        Ok(ZfsResourceCheck {
            name: resource.to_string(),
            state: if exists {
                ZfsResourceState::Present
            } else {
                ZfsResourceState::Missing
            },
        })
    }

    pub fn list(&self, root: &str) -> Result<Vec<ZfsDatasetInfo>> {
        self.provider.list_datasets(root)
    }
}
