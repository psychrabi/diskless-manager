use crate::state::ClientMutationLocks;
use std::process::{Command, Output};

/// Run a blocking `Command` on the blocking thread pool, keeping the
/// async runtime responsive. Returns the same `Output` as `.output()`.
pub async fn run_command(cmd: &mut Command) -> std::io::Result<Output> {
    let args: Vec<std::ffi::OsString> = cmd.get_args().map(|a| a.to_os_string()).collect();
    let prog = cmd.get_program().to_os_string();

    tokio::task::spawn_blocking(move || {
        let mut c = Command::new(&prog);
        c.args(&args);
        c.output()
    })
    .await
    .map_err(std::io::Error::other)?
}

/// Detached work retains its mutation guard after the requesting task is cancelled.
/// A wait limit bounds lock acquisition only; timed-out work is never started.
pub fn spawn_locked<T: Send + 'static>(
    lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    wait: Option<std::time::Duration>,
    task: impl std::future::Future<Output = T> + Send + 'static,
) -> tokio::task::JoinHandle<anyhow::Result<T>> {
    tokio::spawn(async move {
        let acquire = lock.lock_owned();
        let _guard = match wait {
            Some(wait) => tokio::time::timeout(wait, acquire)
                .await
                .map_err(|_| anyhow::anyhow!("client storage is busy; retry boot"))?,
            None => acquire.await,
        };
        Ok(task.await)
    })
}

pub fn spawn_client_locked<T: Send + 'static>(
    locks: ClientMutationLocks,
    client_id: String,
    wait: Option<std::time::Duration>,
    task: impl std::future::Future<Output = T> + Send + 'static,
) -> tokio::task::JoinHandle<anyhow::Result<T>> {
    tokio::spawn(async move {
        let acquire = locks.lock_client(&client_id);
        let _guard = match wait {
            Some(wait) => tokio::time::timeout(wait, acquire)
                .await
                .map_err(|_| anyhow::anyhow!("client storage is busy; retry boot"))?,
            None => acquire.await,
        };
        Ok(task.await)
    })
}

#[cfg(test)]
mod lock_tests {
    use super::{spawn_client_locked, spawn_locked};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn expired_wait_never_runs_queued_work() {
        let lock = Arc::new(Mutex::new(()));
        let guard = lock.lock().await;
        let ran = Arc::new(AtomicBool::new(false));
        let task_ran = ran.clone();
        let task = spawn_locked(
            lock.clone(),
            Some(std::time::Duration::from_millis(1)),
            async move {
                task_ran.store(true, Ordering::SeqCst);
            },
        );
        assert!(task.await.unwrap().is_err());
        drop(guard);
        tokio::task::yield_now().await;
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn detached_client_work_holds_only_its_client_lock_until_completion() {
        let locks = crate::state::ClientMutationLocks::default();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
        let task = spawn_client_locked(locks.clone(), "client-a".into(), None, async move {
            let _ = started_tx.send(());
            let _ = finish_rx.await;
        });
        started_rx.await.unwrap();
        drop(task);

        assert!(locks.try_lock_client("client-a").await.is_none());
        assert!(locks.try_lock_client("client-b").await.is_some());
        finish_tx.send(()).unwrap();
        let _guard = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            locks.lock_client("client-a"),
        )
        .await
        .unwrap();
    }
}
