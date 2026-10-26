use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};
use tokio::sync::{Mutex, OwnedMutexGuard, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

/// Coordinates client-scoped work with fleet-wide operations.
///
/// Client operations share a read gate and serialize by key. A fleet-wide
/// operation takes the write gate and waits for all client work.
#[derive(Clone, Default)]
pub struct ClientMutationLocks {
    gate: Arc<RwLock<()>>,
    keys: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
}

pub struct ClientMutationGuard {
    _gate: OwnedRwLockReadGuard<()>,
    _keys: Vec<OwnedMutexGuard<()>>,
}

impl ClientMutationLocks {
    /// Exclude all client and resource work for a fleet-wide mutation.
    pub async fn lock(&self) -> OwnedRwLockWriteGuard<()> {
        self.gate.clone().write_owned().await
    }

    /// Try to exclude all client and resource work without waiting.
    pub fn try_lock(&self) -> Result<OwnedRwLockWriteGuard<()>, tokio::sync::TryLockError> {
        self.gate.clone().try_write_owned()
    }

    /// Serialize work for one client while allowing other client keys to proceed.
    pub async fn lock_client(&self, client_id: &str) -> ClientMutationGuard {
        self.lock_keys(vec![format!("client:{client_id}")]).await
    }

    /// Serialize work for one shared resource without pausing client work.
    pub async fn lock_resource(&self, resource: &str) -> ClientMutationGuard {
        self.lock_keys(vec![format!("resource:{resource}")]).await
    }

    /// Hold a client and shared-resource lock under one fleet-gate read guard.
    pub async fn lock_client_and_resource(
        &self,
        client_id: &str,
        resource: &str,
    ) -> ClientMutationGuard {
        self.lock_keys(vec![
            format!("client:{client_id}"),
            format!("resource:{resource}"),
        ])
        .await
    }

    pub async fn try_lock_client(&self, client_id: &str) -> Option<ClientMutationGuard> {
        let gate = self.gate.clone().try_read_owned().ok()?;
        let key = format!("client:{client_id}");
        let client = self.key_locks(&[key]).await.pop()?.try_lock_owned().ok()?;
        Some(ClientMutationGuard {
            _gate: gate,
            _keys: vec![client],
        })
    }

    async fn lock_keys(&self, mut keys: Vec<String>) -> ClientMutationGuard {
        let gate = self.gate.clone().read_owned().await;
        // Stable ordering prevents deadlocks when operations share multiple keys.
        keys.sort_unstable();
        keys.dedup();
        let locks = self.key_locks(&keys).await;
        let mut guards = Vec::with_capacity(locks.len());
        for lock in locks {
            guards.push(lock.lock_owned().await);
        }
        ClientMutationGuard {
            _gate: gate,
            _keys: guards,
        }
    }

    async fn key_locks(&self, keys: &[String]) -> Vec<Arc<Mutex<()>>> {
        let mut existing = self.keys.lock().await;
        existing.retain(|_, lock| lock.strong_count() > 0);
        keys.iter()
            .map(|key| {
                if let Some(lock) = existing.get(key).and_then(Weak::upgrade) {
                    lock
                } else {
                    let lock = Arc::new(Mutex::new(()));
                    existing.insert(key.clone(), Arc::downgrade(&lock));
                    lock
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::ClientMutationLocks;

    #[tokio::test]
    async fn client_work_serializes_per_client_but_runs_across_clients() {
        let locks = ClientMutationLocks::default();
        let first = locks.lock_client("client-a").await;
        let other_client = locks.try_lock_client("client-b").await;
        let same_client = locks.try_lock_client("client-a").await;

        assert!(other_client.is_some());
        assert!(same_client.is_none());
        drop((first, other_client));

        let fleet = locks.lock().await;
        assert!(locks.try_lock_client("client-a").await.is_none());
        drop(fleet);
        assert!(locks.try_lock_client("client-a").await.is_some());
    }

    #[tokio::test]
    async fn client_and_shared_resource_work_stays_outside_the_fleet_barrier() {
        let locks = ClientMutationLocks::default();
        let guard = locks
            .lock_client_and_resource("test-client", "windows-revisions")
            .await;

        assert!(locks.try_lock_client("other-client").await.is_some());
        assert!(locks.try_lock_client("test-client").await.is_none());

        let resource_locks = locks.clone();
        let (acquired_tx, mut acquired_rx) = tokio::sync::oneshot::channel();
        let waiter = tokio::spawn(async move {
            let _guard = resource_locks.lock_resource("windows-revisions").await;
            let _ = acquired_tx.send(());
        });
        tokio::task::yield_now().await;
        assert!(acquired_rx.try_recv().is_err());

        drop(guard);
        tokio::time::timeout(std::time::Duration::from_secs(1), acquired_rx)
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
    }
}
