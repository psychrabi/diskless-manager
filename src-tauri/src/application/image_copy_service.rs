//! Managed independent image copies (Phase 4C).
//!
//! A copy streams `zfs send` into `zfs recv` for a new, independent master
//! dataset. The destination only becomes a managed image after the transfer
//! finishes and its size is verified; a failed or cancelled transfer destroys
//! the partial dataset. The source image, its snapshots and its clones are
//! never modified, promoted or renamed.
//!
//! The transfer runs outside the client mutation locks: it is a read-only
//! source operation that must not block provisioning for its whole duration.

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use uuid::Uuid;

use crate::{
    core::image::{Image, ImageKind},
    infrastructure::image::ImageBackend,
    persistence::repositories::image::ImageRepository,
    validation::validate_zfs_name,
};

/// How many finished jobs are kept for the status endpoint.
const MAX_JOBS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyState {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CopyJob {
    pub id: String,
    pub source_image_id: String,
    pub source_name: String,
    pub destination_name: String,
    pub state: CopyState,
    /// Bytes the destination currently occupies; progress is approximate.
    pub bytes_copied: u64,
    /// Source virtual size, used as the progress denominator.
    pub total_bytes: u64,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageCopyError {
    #[error("{0}")]
    NotFound(String),
    /// A well-formed request the server refuses to run.
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// In-process job registry for running and recently finished copies.
///
/// Copies are runtime state, not durable work: a server restart leaves a
/// partial destination only if the transfer was killed mid-flight, so the
/// registry deliberately does not pretend to resume one.
#[derive(Clone, Default)]
pub struct ImageCopyRegistry {
    inner: Arc<Mutex<Book>>,
}

#[derive(Default)]
struct Book {
    jobs: VecDeque<CopyJob>,
    cancels: HashMap<String, tokio::sync::mpsc::Sender<()>>,
}

impl ImageCopyRegistry {
    pub fn list(&self) -> Vec<CopyJob> {
        self.lock().jobs.iter().rev().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<CopyJob> {
        self.lock().jobs.iter().find(|job| job.id == id).cloned()
    }

    fn running_for_source(&self, source_image_id: &str) -> bool {
        self.lock()
            .jobs
            .iter()
            .any(|job| job.source_image_id == source_image_id && job.state == CopyState::Running)
    }

    fn insert(&self, job: CopyJob, cancel: tokio::sync::mpsc::Sender<()>) {
        let mut book = self.lock();
        book.cancels.insert(job.id.clone(), cancel);
        book.jobs.push_back(job);
        while book.jobs.len() > MAX_JOBS {
            let Some(finished) = book
                .jobs
                .iter()
                .position(|job| job.state != CopyState::Running)
            else {
                break;
            };
            if let Some(removed) = book.jobs.remove(finished) {
                book.cancels.remove(&removed.id);
            }
        }
    }

    fn update(&self, id: &str, update: impl FnOnce(&mut CopyJob)) {
        let mut book = self.lock();
        if let Some(job) = book.jobs.iter_mut().find(|job| job.id == id) {
            update(job);
        }
    }

    fn finish(&self, id: &str, state: CopyState, error: Option<String>, bytes: Option<u64>) {
        let mut book = self.lock();
        book.cancels.remove(id);
        if let Some(job) = book.jobs.iter_mut().find(|job| job.id == id) {
            job.state = state;
            job.error = error;
            job.finished_at = Some(Utc::now().to_rfc3339());
            if let Some(bytes) = bytes {
                job.bytes_copied = bytes;
            }
        }
    }

    /// Ask a running copy to stop. The transfer notices at its next poll.
    pub fn cancel(&self, id: &str) -> Result<(), ImageCopyError> {
        let book = self.lock();
        let job =
            book.jobs.iter().find(|job| job.id == id).ok_or_else(|| {
                ImageCopyError::NotFound(format!("copy job '{id}' was not found"))
            })?;
        if job.state != CopyState::Running {
            return Err(ImageCopyError::Rejected(format!(
                "copy job '{id}' already finished"
            )));
        }
        let cancel = book.cancels.get(id).ok_or_else(|| {
            ImageCopyError::Rejected(format!("copy job '{id}' cannot be cancelled"))
        })?;
        cancel
            .try_send(())
            .map_err(|_| ImageCopyError::Rejected(format!("copy job '{id}' is stopping")))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Book> {
        // A poisoned lock still exposes a consistent job list to a reader; a
        // panic in another job must not turn every status request into a 500.
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }
}

#[derive(Clone)]
pub struct ImageCopyService {
    pool: sqlx::SqlitePool,
    backend: Arc<dyn ImageBackend>,
    registry: ImageCopyRegistry,
}

impl ImageCopyService {
    pub fn new(
        pool: sqlx::SqlitePool,
        backend: Arc<dyn ImageBackend>,
        registry: ImageCopyRegistry,
    ) -> Self {
        Self {
            pool,
            backend,
            registry,
        }
    }

    pub fn registry(&self) -> ImageCopyRegistry {
        self.registry.clone()
    }

    /// Reserve a destination and start streaming the source into it.
    pub async fn start(
        &self,
        source_id_or_name: &str,
        requested_name: Option<&str>,
    ) -> Result<CopyJob, ImageCopyError> {
        let repository = ImageRepository::new(self.pool.clone());
        let source = repository
            .get(source_id_or_name)
            .await
            .context("failed to load the source image")?
            .ok_or_else(|| {
                ImageCopyError::NotFound(format!("image '{source_id_or_name}' was not found"))
            })?;

        if source.kind == ImageKind::Snapshot {
            return Err(ImageCopyError::Rejected(
                "a snapshot cannot be copied as an image; copy its parent image".to_string(),
            ));
        }
        if source.status != "ready" {
            return Err(ImageCopyError::Rejected(
                "only a ready image can be copied".to_string(),
            ));
        }
        if repository
            .has_pending_installation(&source.id)
            .await
            .context("failed to check installer sessions")?
        {
            return Err(ImageCopyError::Rejected(
                "image is owned by an unfinished installer session".to_string(),
            ));
        }
        if self.registry.running_for_source(&source.id) {
            return Err(ImageCopyError::Rejected(
                "a copy of this image is already running".to_string(),
            ));
        }

        let parent = self
            .backend
            .image_parent()
            .context("failed to resolve the image dataset")?;
        let destination = match requested_name
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            Some(name) => {
                validate_zfs_name(name)
                    .map_err(|error| ImageCopyError::Rejected(error.to_string()))?;
                format!("{parent}/{name}")
            }
            None => self.default_destination(&parent, &source.name)?,
        };

        if self
            .backend
            .exists(&destination)
            .context("failed to inspect the copy destination")?
        {
            return Err(ImageCopyError::Rejected(format!(
                "image '{destination}' already exists"
            )));
        }

        let total_bytes = source.size_gb.saturating_mul(1024 * 1024 * 1024);
        let job = CopyJob {
            id: Uuid::new_v4().to_string(),
            source_image_id: source.id.clone(),
            source_name: source.name.clone(),
            destination_name: destination.clone(),
            state: CopyState::Running,
            bytes_copied: 0,
            total_bytes,
            error: None,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
        };

        let (cancel_tx, cancel_rx) = tokio::sync::mpsc::channel(1);
        self.registry.insert(job.clone(), cancel_tx);

        let registry = self.registry.clone();
        let backend = self.backend.clone();
        let pool = self.pool.clone();
        let job_id = job.id.clone();
        tokio::spawn(async move {
            let outcome = transfer(
                backend.clone(),
                &source.name,
                &destination,
                total_bytes,
                cancel_rx,
                &registry,
                &job_id,
            )
            .await;

            match outcome {
                Ok(Transfer::Completed) => {
                    match register_copy(&pool, &backend, &source, &destination).await {
                        Ok(image) => {
                            tracing::info!(
                                source = %source.name,
                                copy = %image.name,
                                "copied image"
                            );
                            registry.finish(&job_id, CopyState::Completed, None, Some(total_bytes));
                        }
                        Err(error) => {
                            let message = format!("{error:#}");
                            destroy_dataset(backend.clone(), destination.clone()).await;
                            registry.finish(&job_id, CopyState::Failed, Some(message), None);
                        }
                    }
                }
                Ok(Transfer::Cancelled) => {
                    destroy_dataset(backend.clone(), destination.clone()).await;
                    registry.finish(&job_id, CopyState::Cancelled, None, None);
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    destroy_dataset(backend.clone(), destination.clone()).await;
                    registry.finish(&job_id, CopyState::Failed, Some(message), None);
                }
            }
        });

        Ok(job)
    }

    fn default_destination(&self, parent: &str, source_name: &str) -> Result<String> {
        let base = source_name.rsplit('/').next().unwrap_or(source_name);
        Ok(format!("{parent}/{base}-copy"))
    }
}

enum Transfer {
    Completed,
    Cancelled,
}

async fn transfer(
    backend: Arc<dyn ImageBackend>,
    source: &str,
    destination: &str,
    total_bytes: u64,
    mut cancel: tokio::sync::mpsc::Receiver<()>,
    registry: &ImageCopyRegistry,
    job_id: &str,
) -> Result<Transfer> {
    // The sender uses the standard library so its stdout can be handed to the
    // async receiver as `Stdio`; its liveness is polled without blocking.
    let mut send = std::process::Command::new("sudo")
        .arg("-n")
        .args(["zfs", "send", "-c", source])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start zfs send")?;
    let stream = send
        .stdout
        .take()
        .context("zfs send did not expose its stream")?;

    let mut recv = Command::new("sudo")
        .arg("-n")
        .args(["zfs", "recv", "-u", destination])
        .stdin(Stdio::from(stream))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("failed to start zfs recv")?;

    let mut ticker = tokio::time::interval(Duration::from_secs(2));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut send_finished = false;
    loop {
        if cancel.try_recv().is_ok() {
            let _ = send.kill();
            let _ = recv.start_kill();
            let _ = send.wait();
            let _ = recv.wait().await;
            return Ok(Transfer::Cancelled);
        }

        if !send_finished {
            send_finished = send.try_wait()?.is_some();
        }
        if let Some(status) = recv.try_wait()? {
            if !status.success() {
                let message = read_stderr(&mut recv).await;
                bail!("zfs recv failed: {}", failure(&message, status.code()));
            }
            break;
        }

        ticker.tick().await;
        let used = destination_used_bytes(destination).await.unwrap_or(0);
        registry.update(job_id, |job| job.bytes_copied = used.min(total_bytes));
    }

    // The stream must also have ended cleanly; `zfs send` reports a source
    // that changed underneath it only through its exit status.
    if !send_finished {
        let status = send.wait().context("zfs send did not finish")?;
        if !status.success() {
            let message = read_stderr_std(&mut send);
            bail!("zfs send failed: {}", failure(&message, status.code()));
        }
    }

    if !backend
        .exists(destination)
        .context("failed to verify the copied dataset")?
    {
        bail!("zfs recv reported success but '{destination}' does not exist");
    }

    Ok(Transfer::Completed)
}

fn failure(stderr: &str, code: Option<i32>) -> String {
    let stderr = stderr.trim();
    if stderr.is_empty() {
        format!("exit status {}", code.unwrap_or(-1))
    } else {
        stderr.to_string()
    }
}

async fn read_stderr(child: &mut tokio::process::Child) -> String {
    let mut buffer = Vec::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_end(&mut buffer).await;
    }
    String::from_utf8_lossy(&buffer).to_string()
}

fn read_stderr_std(child: &mut std::process::Child) -> String {
    let mut buffer = Vec::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = std::io::Read::read_to_end(&mut stderr, &mut buffer);
    }
    String::from_utf8_lossy(&buffer).to_string()
}

/// Exact used bytes of the destination, or `None` while it is not visible yet.
async fn destination_used_bytes(destination: &str) -> Option<u64> {
    let output = Command::new("sudo")
        .arg("-n")
        .args(["zfs", "list", "-H", "-p", "-o", "used", destination])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

async fn destroy_dataset(backend: Arc<dyn ImageBackend>, name: String) {
    let destroyed = tokio::task::spawn_blocking(move || backend.destroy(&name)).await;
    match destroyed {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::error!(%error, "failed to remove a partial image copy");
        }
        Err(error) => {
            tracing::error!(%error, "image copy cleanup worker failed");
        }
    }
}

/// Verify the copy and publish it as an independent master image record.
async fn register_copy(
    pool: &sqlx::SqlitePool,
    backend: &Arc<dyn ImageBackend>,
    source: &Image,
    destination: &str,
) -> Result<Image> {
    let backend = backend.clone();
    let name = destination.to_string();
    let info = tokio::task::spawn_blocking(move || backend.info(&name))
        .await
        .context("image copy verification worker failed")??;

    let expected = source.size_gb.saturating_mul(1024 * 1024 * 1024);
    if info.virtual_size != expected {
        bail!(
            "copied volume is {} bytes but '{}' is {} bytes",
            info.virtual_size,
            source.name,
            expected
        );
    }

    let now = Utc::now();
    let image = Image {
        id: Uuid::new_v4().to_string(),
        name: destination.to_string(),
        kind: ImageKind::Master,
        os_type: source.os_type,
        size_gb: source.size_gb,
        path: format!("/dev/zvol/{destination}").into(),
        format: source.format,
        status: "ready".to_string(),
        description: Some(format!("Copy of {}", source.name)),
        parent_id: None,
        source_snapshot: None,
        checksum: None,
        is_default: false,
        created_at: now,
        updated_at: now,
    };
    ImageRepository::new(pool.clone())
        .insert(&image)
        .await
        .context("failed to persist the copied image")?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, state: CopyState) -> CopyJob {
        CopyJob {
            id: id.to_string(),
            source_image_id: "source".to_string(),
            source_name: "pool/image-disk/windows11".to_string(),
            destination_name: "pool/image-disk/windows11-copy".to_string(),
            state,
            bytes_copied: 0,
            total_bytes: 1024,
            error: None,
            started_at: "2026-11-05T00:00:00Z".to_string(),
            finished_at: None,
        }
    }

    #[test]
    fn a_running_copy_blocks_a_second_copy_of_the_same_source() {
        let registry = ImageCopyRegistry::default();
        let (cancel, _receiver) = tokio::sync::mpsc::channel(1);
        registry.insert(job("one", CopyState::Running), cancel);
        assert!(registry.running_for_source("source"));
    }

    #[test]
    fn a_finished_copy_no_longer_blocks_the_source() {
        let registry = ImageCopyRegistry::default();
        let (cancel, _receiver) = tokio::sync::mpsc::channel(1);
        registry.insert(job("one", CopyState::Running), cancel);
        registry.finish("one", CopyState::Completed, None, Some(1024));

        assert!(!registry.running_for_source("source"));
        assert_eq!(registry.get("one").unwrap().state, CopyState::Completed);
        assert!(registry.get("one").unwrap().finished_at.is_some());
    }

    #[test]
    fn cancelling_a_finished_job_is_rejected() {
        let registry = ImageCopyRegistry::default();
        let (cancel, _receiver) = tokio::sync::mpsc::channel(1);
        registry.insert(job("one", CopyState::Running), cancel);
        registry.finish("one", CopyState::Failed, Some("boom".into()), None);

        let error = registry.cancel("one").unwrap_err();
        assert!(matches!(error, ImageCopyError::Rejected(_)));
    }

    #[test]
    fn cancelling_an_unknown_job_is_not_found() {
        let registry = ImageCopyRegistry::default();
        let error = registry.cancel("missing").unwrap_err();
        assert!(matches!(error, ImageCopyError::NotFound(_)));
    }

    #[test]
    fn only_recent_jobs_are_kept() {
        let registry = ImageCopyRegistry::default();
        for index in 0..(MAX_JOBS + 5) {
            let (cancel, _receiver) = tokio::sync::mpsc::channel(1);
            registry.insert(job(&format!("job-{index}"), CopyState::Completed), cancel);
        }
        assert_eq!(registry.list().len(), MAX_JOBS);
    }
}
