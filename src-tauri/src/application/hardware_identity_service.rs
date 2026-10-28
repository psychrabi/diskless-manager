use crate::{
    application::StorageService,
    domain::{
        storage::{ClientStorageSpec, StorageSource},
        Client, ClientId, ClientStatus,
    },
    infrastructure::{image::ImageBackend, iscsi::IscsiProvisioner},
    persistence::{
        repositories::hardware_identity::{
            HardwareIdentityRepository, HardwareProfile, IdentityCache,
        },
        ClientRepository,
    },
    state::ClientMutationLocks,
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct HardwareIdentityService {
    deps: Arc<HardwareIdentityDeps>,
}
pub struct HardwareIdentityDeps {
    pub repository: HardwareIdentityRepository,
    pub clients: ClientRepository,
    pub backend: Arc<dyn ImageBackend>,
    pub storage: Arc<StorageService>,
    pub iscsi: Arc<dyn IscsiProvisioner>,
    pub mutation_locks: ClientMutationLocks,
}
#[derive(Serialize)]
pub struct IdentityStatus {
    pub profile: Option<HardwareProfile>,
    pub cache: Option<IdentityCache>,
}

/// Resolve an applied client clone to the immutable identity baseline it was created from.
pub async fn cached_source_for_client(
    pool: &SqlitePool,
    client: &Client,
) -> Result<Option<String>> {
    let cache: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT source_revision,dataset,snapshot,replacement,state FROM client_identity_caches WHERE client_id=?",
    )
    .bind(client.id.as_str())
    .fetch_optional(pool)
    .await?;
    let Some((source_revision, dataset, snapshot, replacement, state)) = cache else {
        return Ok(None);
    };
    ensure!(
        !matches!(
            state.as_str(),
            "preparing" | "applying" | "deleting" | "failed"
        ),
        "identity cache operation requires recovery before client boot or storage reset"
    );
    if client.writeback.as_deref() != Some(replacement.as_str()) {
        return Ok(None);
    }
    ensure!(
        client.snapshot.as_deref() == Some(source_revision.as_str()),
        "identity cache source revision changed; restore or remove the cache before client boot"
    );
    Ok(Some(format!("{dataset}@{snapshot}")))
}

/// Canonical hardware identity deliberately excludes MAC, client name and observation time.
fn normalized_profile(
    client_id: &str,
    ids: &[String],
    arch: &str,
    firmware: &str,
) -> Result<HardwareProfile> {
    ensure!(
        !ids.is_empty() && ids.len() <= 32,
        "boot NIC hardware IDs are required (maximum 32)"
    );
    let mut ids = crate::infrastructure::pxe::driver_selection::normalize_hardware_ids(ids)
        .map_err(|_| anyhow::anyhow!("invalid boot NIC hardware IDs"))?
        .into_iter()
        .map(|id| id.to_ascii_uppercase())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    let architecture = match arch.trim().to_ascii_lowercase().as_str() {
        "amd64" | "x64" | "x86_64" => "x86_64",
        "x86" | "i386" | "i686" => "x86",
        "arm64" | "aarch64" => "aarch64",
        _ => anyhow::bail!("unsupported observed architecture"),
    }
    .to_owned();
    let firmware = match firmware.trim().to_ascii_lowercase().as_str() {
        "uefi" | "efi" => "uefi",
        "bios" | "legacy" => "bios",
        _ => anyhow::bail!("unsupported observed firmware"),
    }
    .to_owned();
    let boot_nic_ids = serde_json::to_string(&ids)?;
    let profile_key = serde_json::to_string(&(&ids, &architecture, &firmware))?;
    Ok(HardwareProfile {
        client_id: client_id.to_owned(),
        profile_key,
        boot_nic_ids,
        architecture,
        firmware,
        observed_at: chrono::Utc::now().to_rfc3339(),
    })
}

impl HardwareIdentityService {
    pub fn new(deps: HardwareIdentityDeps) -> Self {
        Self {
            deps: Arc::new(deps),
        }
    }
    pub async fn observe(
        &self,
        client_id: &str,
        boot_nic_ids: &[String],
        architecture: &str,
        firmware: &str,
    ) -> Result<()> {
        let profile = normalized_profile(client_id, boot_nic_ids, architecture, firmware)?;
        let _guard = self.deps.mutation_locks.lock_client(client_id).await;
        self.deps.repository.observe(&profile).await
    }
    pub async fn status(&self, client_id: &str) -> Result<IdentityStatus> {
        let client = self.client(client_id).await?;
        let mut cache = self.deps.repository.cache(client_id).await?;
        if let Some(value) = &mut cache {
            if client.snapshot.as_deref() != Some(&value.source_revision)
                && !matches!(value.state.as_str(), "applying" | "deleting")
            {
                self.deps
                    .repository
                    .state(client_id, "stale", Some("assigned image revision changed"))
                    .await?;
                value.state = "stale".into();
            }
        }
        Ok(IdentityStatus {
            profile: self.deps.repository.profile(client_id).await?,
            cache,
        })
    }
    async fn client(&self, id: &str) -> Result<Client> {
        self.deps
            .clients
            .find_by_id(&ClientId::from_string(id.to_owned())?)
            .await?
            .context("client does not exist")
    }
    /// Return a replacement origin only while the immutable source/profile bindings still match.
    pub async fn resolved_source(&self, client_id: &str) -> Result<Option<String>> {
        let client = self.client(client_id).await?;
        let Some(cache) = self.deps.repository.cache(client_id).await? else {
            return Ok(None);
        };
        if client.writeback.as_deref() != Some(&cache.replacement) {
            return Ok(None);
        }
        // Already-applied storage retains its physical origin even if a later observation invalidates future applies.
        ensure!(
            client.snapshot.as_deref() == Some(&cache.source_revision),
            "identity cache source revision changed; select a new client disk"
        );
        Ok(Some(format!("{}@{}", cache.dataset, cache.snapshot)))
    }
    pub async fn validated_source(&self, client: &Client) -> Result<Option<String>> {
        self.resolved_source(client.id.as_str()).await
    }
    async fn spec(&self, client: &Client) -> Result<ClientStorageSpec> {
        let dataset = client
            .writeback
            .clone()
            .context("client has no persisted owned writeback disk")?;
        let source = self
            .resolved_source(client.id.as_str())
            .await?
            .or_else(|| client.snapshot.clone())
            .context("client requires an immutable source revision")?;
        Ok(ClientStorageSpec {
            client_id: client.id.to_string(),
            client_name: client.name.clone(),
            source: StorageSource::Snapshot(source),
            dataset,
            backstore: client
                .block_store
                .clone()
                .context("client has no persisted backstore")?,
            target_iqn: client
                .target_iqn
                .clone()
                .context("client has no persisted target")?,
            lun: 0,
            use_game_disk: client.use_game_disk,
            game_disks: self.deps.clients.game_selection(&client.id).await?,
            chap: if client.chap_enabled {
                Some(crate::infrastructure::iscsi::ChapCredentials {
                    username: client
                        .chap_user
                        .clone()
                        .context("client CHAP username missing")?,
                    password: client
                        .chap_secret
                        .clone()
                        .context("client CHAP password missing")?,
                })
            } else {
                None
            },
            installer_iso: None,
        })
    }
    async fn checked(&self, client_id: &str) -> Result<(Client, ClientStorageSpec)> {
        let client = self.client(client_id).await?;
        ensure!(
            client.status == ClientStatus::Offline && !client.is_super_mode(),
            "identity cache requires an offline normal client"
        );
        let image = crate::persistence::repositories::image::ImageRepository::new(
            self.deps.repository.pool.clone(),
        )
        .get(&client.master)
        .await?
        .context("assigned image does not exist")?;
        ensure!(
            image.os_type == crate::core::image::OsType::Windows,
            "identity caches require a Windows image"
        );
        let spec = self.spec(&client).await?;
        let others: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clients WHERE id<>? AND (writeback=? OR block_device=? OR target_iqn=? OR block_store=? OR master=?)")
            .bind(client_id).bind(&spec.dataset).bind(spec.block_device().to_string_lossy().as_ref()).bind(&spec.target_iqn).bind(&spec.backstore).bind(&spec.dataset).fetch_one(&self.deps.repository.pool).await?;
        ensure!(
            others == 0,
            "client disk or target is shared with another client"
        );
        let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM client_offline_resets WHERE client_id=? AND operation IS NOT NULL").bind(client_id).fetch_one(&self.deps.repository.pool).await?;
        ensure!(pending == 0, "client storage recovery is pending");
        let pending_super: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM super_sessions WHERE client_id=? AND state<>'completed'",
        )
        .bind(client_id)
        .fetch_one(&self.deps.repository.pool)
        .await?;
        ensure!(pending_super == 0, "managed Super session is pending");
        let pending_installation: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM installation_sessions WHERE client_id=? AND closed_at IS NULL",
        )
        .bind(client_id)
        .fetch_one(&self.deps.repository.pool)
        .await?;
        ensure!(
            pending_installation == 0,
            "managed installation session is pending"
        );
        ensure!(
            self.deps.backend.clone_origin(&spec.dataset)?.as_deref() == Some(spec.source.value()),
            "client disk does not belong to the assigned immutable revision"
        );
        self.deps.storage.validate_unmounted(&spec.dataset)?;
        ensure!(
            self.deps
                .iscsi
                .active_session_peer_ips(&spec.target_iqn, &client.name)?
                .is_empty(),
            "client has active iSCSI sessions"
        );
        if let Err(original_error) = self
            .deps
            .storage
            .validate_client_target_for_replacement(&spec)
        {
            let cache = self
                .deps
                .repository
                .cache(client_id)
                .await?
                .context("target does not match client storage")?;
            ensure!(
                cache.prior_dataset.is_some(),
                "target ownership cannot be recovered: {original_error}"
            );
            let mut recovery_spec = spec.clone();
            if cache.state == "deleting" {
                recovery_spec.dataset = cache.dataset.clone();
                recovery_spec.source = StorageSource::Snapshot(cache.source_revision.clone());
            } else {
                ensure!(
                    self.deps
                        .backend
                        .clone_origin(&cache.replacement)?
                        .as_deref()
                        == Some(format!("{}@{}", cache.dataset, cache.snapshot).as_str()),
                    "recovery replacement origin changed"
                );
                recovery_spec.dataset = cache.replacement;
                recovery_spec.source =
                    StorageSource::ExistingClientVolume(recovery_spec.dataset.clone());
            }
            self.deps
                .storage
                .validate_client_target_for_replacement(&recovery_spec)?;
        }
        Ok((client, spec))
    }
    /// Resume interrupted target switches without deleting either disk. Failures remain durable.
    pub async fn recover_pending(&self) -> Result<()> {
        let clients: Vec<(String, String, Option<String>)> = sqlx::query_as("SELECT client_id,state,prior_dataset FROM client_identity_caches WHERE state IN ('preparing','deleting','applying','failed')")
            .fetch_all(&self.deps.repository.pool).await?;
        let mut failures = Vec::new();
        for (id, state, prior_dataset) in clients {
            let result = if state == "deleting" {
                self.delete(&id).await
            } else if prior_dataset.is_none() {
                self.capture(&id).await
            } else {
                self.apply(&id).await
            };
            if let Err(error) = result {
                failures.push(format!("{id}: {error:#}"));
            }
        }
        ensure!(
            failures.is_empty(),
            "identity cache recovery remains pending: {}",
            failures.join("; ")
        );
        Ok(())
    }
    pub async fn capture(&self, client_id: &str) -> Result<IdentityStatus> {
        self.mutate(client_id, "capture").await
    }
    pub async fn apply(&self, client_id: &str) -> Result<IdentityStatus> {
        self.mutate(client_id, "apply").await
    }
    pub async fn delete(&self, client_id: &str) -> Result<IdentityStatus> {
        self.mutate(client_id, "delete").await
    }
    async fn mutate(&self, client_id: &str, action: &'static str) -> Result<IdentityStatus> {
        let service = self.clone();
        let id = client_id.to_owned();
        let locks = self.deps.mutation_locks.clone();
        crate::api::util::spawn_client_locked(locks, id.clone(), None, async move {
            let runtime = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                runtime.block_on(service.mutate_locked(&id, action))
            })
            .await
            .context("identity cache worker failed")?
        })
        .await
        .context("identity cache lock worker failed")???;
        self.status(client_id).await
    }
    async fn exclusive_dataset(&self, client_id: &str, dataset: &str) -> Result<()> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clients WHERE id<>? AND (writeback=? OR master=? OR block_device=?)")
            .bind(client_id).bind(dataset).bind(dataset).bind(format!("/dev/zvol/{dataset}")).fetch_one(&self.deps.repository.pool).await?;
        ensure!(count == 0, "identity storage belongs to another client");
        Ok(())
    }
    async fn delete_locked(
        &self,
        client: &mut Client,
        spec: &ClientStorageSpec,
        cache: &IdentityCache,
    ) -> Result<()> {
        ensure!(
            client.snapshot.as_deref() == Some(&cache.source_revision),
            "original assignment changed; retain identity recovery resources"
        );
        let mut original = spec.clone();
        original.dataset = cache.dataset.clone();
        original.source = StorageSource::Snapshot(cache.source_revision.clone());
        ensure!(
            self.deps
                .backend
                .clone_origin(&original.dataset)?
                .as_deref()
                == Some(&cache.source_revision),
            "original disk ownership changed"
        );
        self.deps.storage.validate_unmounted(&original.dataset)?;
        let others: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clients WHERE id<>? AND (writeback=? OR master=? OR block_device=?)")
            .bind(client.id.as_str()).bind(&original.dataset).bind(&original.dataset).bind(original.block_device().to_string_lossy().as_ref()).fetch_one(&self.deps.repository.pool).await?;
        ensure!(others == 0, "original disk belongs to another client");
        // Target provisioning must never own or delete either retained disk on failure.
        original.source = StorageSource::ExistingVolume(original.dataset.clone());
        if client.writeback.as_deref() == Some(&cache.replacement) {
            self.deps
                .storage
                .remove_client_target(&spec.target_iqn, std::slice::from_ref(&spec.backstore))?;
            self.deps.storage.create_client_storage(&original)?;
            client.writeback = Some(original.dataset.clone());
            client.block_device = Some(original.block_device().to_string_lossy().into_owned());
            self.deps.clients.update(client).await?;
        } else {
            // Retry after the durable client update may still need the original target repaired.
            self.deps.storage.create_client_storage(&original)?;
        }
        self.exclusive_dataset(client.id.as_str(), &cache.replacement)
            .await?;
        if self.deps.backend.exists(&cache.replacement)? {
            ensure!(
                self.deps
                    .backend
                    .clone_origin(&cache.replacement)?
                    .as_deref()
                    == Some(format!("{}@{}", cache.dataset, cache.snapshot).as_str()),
                "replacement disk ownership changed"
            );
            self.deps.storage.validate_unmounted(&cache.replacement)?;
            self.deps.backend.destroy(&cache.replacement)?;
        }
        if self
            .deps
            .backend
            .exists(&format!("{}@{}", cache.dataset, cache.snapshot))?
        {
            self.deps
                .backend
                .destroy_snapshot(&cache.dataset, &cache.snapshot)?;
        }
        sqlx::query("DELETE FROM client_identity_caches WHERE client_id=?")
            .bind(client.id.as_str())
            .execute(&self.deps.repository.pool)
            .await?;
        Ok(())
    }
    async fn mutate_locked(&self, client_id: &str, action: &str) -> Result<()> {
        let (mut client, spec) = self.checked(client_id).await?;
        let profile = self
            .deps
            .repository
            .profile(client_id)
            .await?
            .context("observe client boot NIC hardware before capturing identity")?;
        let existing = self.deps.repository.cache(client_id).await?;
        if action == "delete" {
            let cache = existing.context("identity cache does not exist")?;
            self.deps
                .repository
                .state(client_id, "deleting", None)
                .await?;
            let result = self.delete_locked(&mut client, &spec, &cache).await;
            if let Err(error) = &result {
                self.deps
                    .repository
                    .state(client_id, "deleting", Some(&error.to_string()))
                    .await?;
            }
            return result;
        }
        let cache = if let Some(cache) = existing {
            ensure!(
                cache.source_revision == client.snapshot.as_deref().unwrap_or_default()
                    && cache.profile_key == profile.profile_key
                    && !matches!(cache.state.as_str(), "stale" | "deleting"),
                "identity cache is stale; delete it and capture a new baseline"
            );
            cache
        } else {
            ensure!(action == "capture", "capture an identity baseline first");
            let id = uuid::Uuid::new_v4().simple().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let cache = IdentityCache {
                client_id: client_id.to_owned(),
                id: id.clone(),
                profile_key: profile.profile_key,
                source_revision: client
                    .snapshot
                    .clone()
                    .context("immutable source revision is required")?,
                dataset: spec.dataset.clone(),
                snapshot: format!("identity_{id}"),
                replacement: format!("{}_identity_{id}", spec.dataset),
                prior_dataset: None,
                state: "preparing".into(),
                last_error: None,
                created_at: now.clone(),
                updated_at: now,
            };
            self.deps.repository.insert(&cache).await?;
            cache
        };
        let result: Result<()> = async {
            let snapshot = format!("{}@{}",cache.dataset,cache.snapshot);
            if action == "capture" {
                ensure!(cache.dataset == spec.dataset, "cannot recapture an applied identity baseline");
                if !self.deps.backend.exists(&snapshot)? { self.deps.backend.create_snapshot(&cache.dataset,&cache.snapshot)?; }
            } else {
                ensure!(self.deps.backend.exists(&snapshot)?, "identity baseline snapshot is missing; retry capture");
                if client.writeback.as_deref() != Some(&cache.replacement) {
                    sqlx::query("UPDATE client_identity_caches SET state='applying',prior_dataset=?,updated_at=? WHERE client_id=?")
                        .bind(&spec.dataset).bind(chrono::Utc::now().to_rfc3339()).bind(client_id).execute(&self.deps.repository.pool).await?;
                    self.exclusive_dataset(client_id,&cache.replacement).await?;
                    if !self.deps.backend.exists(&cache.replacement)? { self.deps.backend.clone_image(&snapshot,&cache.replacement)?; }
                    ensure!(self.deps.backend.clone_origin(&cache.replacement)?.as_deref() == Some(&snapshot), "identity replacement has unexpected ownership");
                    self.deps.storage.validate_unmounted(&cache.replacement)?;
                    self.deps.storage.remove_client_target(&spec.target_iqn,std::slice::from_ref(&spec.backstore))?;
                    let mut desired = spec.clone(); desired.dataset = cache.replacement.clone(); desired.source = StorageSource::ExistingVolume(cache.replacement.clone());
                    if let Err(error) = self.deps.storage.create_client_storage(&desired) {
                        self.deps.storage.create_client_storage(&ClientStorageSpec { source: StorageSource::ExistingVolume(spec.dataset.clone()), ..spec.clone() }).context("identity apply failed and original target restoration failed; original disk retained")?;
                        return Err(error);
                    }
                    client.writeback = Some(cache.replacement.clone()); client.block_device = Some(desired.block_device().to_string_lossy().into_owned());
                    self.deps.clients.update(&client).await.context("replacement target retained; retry identity apply to persist client disk")?;
                }
            }
            self.deps.repository.state(client_id,"ready",None).await
        }.await;
        if let Err(error) = result {
            self.deps
                .repository
                .state(client_id, "failed", Some(&error.to_string()))
                .await
                .context("failed to persist identity recovery error")?;
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{cached_source_for_client, normalized_profile};
    #[test]
    fn profile_normalizes_order_case_architecture_and_excludes_client_identity() {
        let a = normalized_profile(
            "a",
            &["pci/ven_1234".into(), "PCI/ven_5678".into()],
            "amd64",
            "EFI",
        )
        .unwrap();
        let b = normalized_profile(
            "b",
            &[
                "pci\\VEN_5678".into(),
                "PCI\\VEN_1234".into(),
                "PCI\\VEN_1234".into(),
            ],
            "x86_64",
            "uefi",
        )
        .unwrap();
        assert_eq!(a.profile_key, b.profile_key);
        assert_ne!(
            a.profile_key,
            normalized_profile("a", &["PCI\\VEN_1234".into()], "amd64", "bios")
                .unwrap()
                .profile_key
        );
        assert!(normalized_profile("a", &[], "amd64", "uefi").is_err());
    }

    #[tokio::test]
    async fn applied_identity_source_tracks_its_baseline_and_fails_closed_during_recovery() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql("CREATE TABLE client_identity_caches(client_id TEXT,source_revision TEXT,dataset TEXT,snapshot TEXT,replacement TEXT,state TEXT);")
            .execute(&pool).await.unwrap();
        let mut client = crate::domain::Client::create(crate::domain::CreateClient {
            installer_media_id: None,
            enabled: None,
            name: "client".into(),
            mac: "02:00:00:00:00:01".into(),
            ip: "192.0.2.1".into(),
            master: "pool/master".into(),
            snapshot: Some("pool/master@v1".into()),
            block_store: Some("block_client".into()),
            block_device: Some("/dev/zvol/pool/replacement".into()),
            target_iqn: Some("iqn.test:client".into()),
            pxe_mode: crate::domain::PxeMode::default(),
            keep_writeback: true,
            use_game_disk: false,
            game_disks: vec![],
            chap_enabled: false,
        })
        .unwrap();
        client.id = crate::domain::ClientId::from_string("client").unwrap();
        client.writeback = Some("pool/replacement".into());
        sqlx::query("INSERT INTO client_identity_caches VALUES('client','pool/master@v1','pool/client','identity','pool/replacement','ready')")
            .execute(&pool).await.unwrap();
        assert_eq!(
            cached_source_for_client(&pool, &client)
                .await
                .unwrap()
                .as_deref(),
            Some("pool/client@identity")
        );
        sqlx::query("UPDATE client_identity_caches SET state='failed'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(cached_source_for_client(&pool, &client).await.is_err());
    }
}

#[cfg(test)]
mod operation_tests {
    use super::*;
    use crate::{
        core::image::ImageFormat,
        domain::{CreateClient, PxeMode},
        infrastructure::{
            image::ImageBackendInfo,
            iscsi::{
                ChapCredentials, IscsiLunBacking, IscsiLunState, IscsiTargetSpec, IscsiTargetState,
            },
        },
    };
    use std::{
        collections::{HashMap, HashSet},
        path::Path,
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        },
    };
    #[derive(Default)]
    struct FakeImages {
        datasets: Mutex<HashSet<String>>,
        snapshots: Mutex<HashSet<String>>,
        origins: Mutex<HashMap<String, String>>,
        mounted: AtomicBool,
        fail_destroy_once: AtomicBool,
    }

    impl ImageBackend for FakeImages {
        fn validate_unmounted(&self, _: &str) -> Result<()> {
            if self.mounted.load(Ordering::SeqCst) {
                anyhow::bail!("injected mounted client volume")
            }
            Ok(())
        }
        fn settle_device_changes(&self) -> Result<()> {
            Ok(())
        }
        fn clone_origin(&self, name: &str) -> Result<Option<String>> {
            Ok(self.origins.lock().unwrap().get(name).cloned())
        }
        fn exists(&self, name: &str) -> Result<bool> {
            Ok(self.datasets.lock().unwrap().contains(name)
                || self.snapshots.lock().unwrap().contains(name))
        }
        fn create_volume(&self, name: &str, _: u64) -> Result<()> {
            self.datasets.lock().unwrap().insert(name.to_owned());
            Ok(())
        }
        fn destroy(&self, name: &str) -> Result<()> {
            ensure!(
                !self.fail_destroy_once.swap(false, Ordering::SeqCst),
                "injected destroy failure"
            );
            self.datasets.lock().unwrap().remove(name);
            self.origins.lock().unwrap().remove(name);
            Ok(())
        }
        fn rename(&self, old: &str, new: &str) -> Result<()> {
            if self.datasets.lock().unwrap().remove(old) {
                self.datasets.lock().unwrap().insert(new.into());
            }
            Ok(())
        }
        fn clone_image(&self, source: &str, destination: &str) -> Result<()> {
            self.datasets.lock().unwrap().insert(destination.into());
            self.origins
                .lock()
                .unwrap()
                .insert(destination.into(), source.into());
            Ok(())
        }
        fn promote_clone(&self, name: &str) -> Result<()> {
            self.origins.lock().unwrap().remove(name);
            Ok(())
        }
        fn create_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()> {
            self.snapshots
                .lock()
                .unwrap()
                .insert(format!("{dataset}@{snapshot}"));
            Ok(())
        }
        fn destroy_snapshot(&self, dataset: &str, snapshot: &str) -> Result<()> {
            self.snapshots
                .lock()
                .unwrap()
                .remove(&format!("{dataset}@{snapshot}"));
            Ok(())
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn import_raw(&self, _: &Path, _: &str, _: u64) -> Result<()> {
            Ok(())
        }
        fn verify(&self, name: &str) -> Result<bool> {
            self.exists(name)
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            Ok(ImageBackendInfo {
                virtual_size: 50,
                actual_size: 50,
                format: ImageFormat::Raw,
                backing_file: None,
                snapshots: Vec::new(),
            })
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        fn image_parent(&self) -> Result<String> {
            Ok("pool/images".into())
        }
    }

    #[derive(Default)]
    struct FakeIscsi {
        target: Mutex<Option<IscsiTargetSpec>>,
        fail_create_once: AtomicBool,
        extra_after_create: AtomicBool,
        active_session: AtomicBool,
        seen_lun_counts: Mutex<Vec<usize>>,
    }
    impl IscsiProvisioner for FakeIscsi {
        fn active_session_peer_ips(&self, _: &str, _: &str) -> Result<Vec<std::net::IpAddr>> {
            Ok(if self.active_session.load(Ordering::SeqCst) {
                vec!["192.0.2.99".parse().unwrap()]
            } else {
                vec![]
            })
        }
        fn create_target(&self, spec: &IscsiTargetSpec) -> Result<()> {
            if self.fail_create_once.swap(false, Ordering::SeqCst) {
                anyhow::bail!("injected target create failure");
            }
            self.seen_lun_counts.lock().unwrap().push(spec.luns.len());
            let mut created = spec.clone();
            if self.extra_after_create.swap(false, Ordering::SeqCst) {
                let mut extra = created.luns[1].clone();
                extra.lun = 2;
                extra.backstore = "stale_game_lun".into();
                created.luns.push(extra);
            }
            *self.target.lock().unwrap() = Some(created);
            Ok(())
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            *self.target.lock().unwrap() = None;
            Ok(())
        }
        fn remove_target_with_backstores(&self, _: &str, _: &[String]) -> Result<()> {
            *self.target.lock().unwrap() = None;
            Ok(())
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            Ok(self.target.lock().unwrap().is_some())
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> Result<()> {
            Ok(())
        }
        fn list_target_luns(&self, _: &str) -> Result<Vec<IscsiLunState>> {
            Ok(self
                .target
                .lock()
                .unwrap()
                .as_ref()
                .map(|s| {
                    s.luns
                        .iter()
                        .map(|l| IscsiLunState {
                            lun: l.lun,
                            backstore: l.backstore.clone(),
                            exists: true,
                            backstore_exists: true,
                            block_device_matches: true,
                        })
                        .collect()
                })
                .unwrap_or_default())
        }
        fn inspect_target(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
            let ready = self.target.lock().unwrap().as_ref().is_some_and(|current| {
                spec.luns.iter().all(|wanted| {
                    current.luns.iter().any(|actual| {
                        actual.lun == wanted.lun
                            && actual.backstore == wanted.backstore
                            && actual.block_device == wanted.block_device
                    })
                })
            });
            Ok(IscsiTargetState::from_luns(
                spec.target_iqn.clone(),
                ready,
                spec.luns
                    .iter()
                    .map(|lun| IscsiLunState {
                        lun: lun.lun,
                        backstore: lun.backstore.clone(),
                        exists: ready,
                        backstore_exists: ready,
                        block_device_matches: ready,
                    })
                    .collect(),
                ready,
            ))
        }
        fn verify_named_file_lun_mappings(&self, spec: &IscsiTargetSpec) -> Result<()> {
            if spec
                .luns
                .iter()
                .any(|lun| lun.backing == IscsiLunBacking::File)
            {
                anyhow::ensure!(
                    spec.initiator_iqn.is_some(),
                    "named file LUN mapping is missing"
                );
                let target = self.target.lock().unwrap();
                let actual = target.as_ref().context("target missing")?;
                anyhow::ensure!(
                    actual.initiator_iqn == spec.initiator_iqn,
                    "named file LUN mapping is missing"
                );
            }
            Ok(())
        }
        fn reconcile(&self, spec: &IscsiTargetSpec) -> Result<()> {
            self.create_target(spec)
        }
    }

    async fn fixture() -> (
        HardwareIdentityService,
        Arc<FakeImages>,
        Arc<FakeIscsi>,
        Client,
    ) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let clients = ClientRepository::new(pool.clone());
        sqlx::query("INSERT INTO images(id,name,os_type,size_gb,path,created_at,updated_at) VALUES('image-master','pool/master','windows',50,'/dev/zvol/pool/master',?,?)")
            .bind(chrono::Utc::now().to_rfc3339()).bind(chrono::Utc::now().to_rfc3339()).execute(&pool).await.unwrap();
        let mut client = Client::create(CreateClient {
            installer_media_id: None,
            enabled: None,
            name: "client-a".into(),
            mac: "02:00:00:00:00:01".into(),
            ip: "192.0.2.1".into(),
            master: "pool/master".into(),
            snapshot: Some("pool/master@v1".into()),
            block_store: Some("block_client_a".into()),
            block_device: Some("/dev/zvol/pool/client-writeback".into()),
            target_iqn: Some("iqn.test:client-a".into()),
            pxe_mode: PxeMode::default(),
            keep_writeback: true,
            use_game_disk: false,
            game_disks: vec![],
            chap_enabled: false,
        })
        .unwrap();
        client.status = ClientStatus::Offline;
        client.writeback = Some("pool/client-writeback".into());
        clients.insert(&client).await.unwrap();
        let backend = Arc::new(FakeImages::default());
        backend
            .datasets
            .lock()
            .unwrap()
            .insert("pool/client-writeback".into());
        backend
            .origins
            .lock()
            .unwrap()
            .insert("pool/client-writeback".into(), "pool/master@v1".into());
        backend
            .snapshots
            .lock()
            .unwrap()
            .insert("pool/master@v1".into());
        let iscsi = Arc::new(FakeIscsi::default());
        let storage = Arc::new(StorageService::new(backend.clone(), iscsi.clone()));
        let service = HardwareIdentityService::new(HardwareIdentityDeps {
            repository: HardwareIdentityRepository::new(pool),
            clients,
            backend: backend.clone(),
            storage,
            iscsi: iscsi.clone(),
            mutation_locks: ClientMutationLocks::default(),
        });
        let mut spec = service.spec(&client).await.unwrap();
        spec.source = StorageSource::ExistingVolume(spec.dataset.clone());
        service.deps.storage.create_client_storage(&spec).unwrap();
        service
            .observe(
                client.id.as_str(),
                &["PCI\\VEN_1234&DEV_5678".into()],
                "amd64",
                "uefi",
            )
            .await
            .unwrap();
        (service, backend, iscsi, client)
    }
    #[tokio::test]
    async fn capture_apply_and_delete_preserve_and_restore_original_disk() {
        let (service, backend, _, client) = fixture().await;
        let id = client.id.as_str();
        let captured = service.capture(id).await.unwrap().cache.unwrap();
        assert!(backend
            .exists(&format!("{}@{}", captured.dataset, captured.snapshot))
            .unwrap());
        service.apply(id).await.unwrap();
        let applied = service.client(id).await.unwrap();
        assert_eq!(
            applied.writeback.as_deref(),
            Some(captured.replacement.as_str())
        );
        assert_eq!(applied.snapshot, client.snapshot);
        assert!(
            backend.exists(&captured.dataset).unwrap(),
            "original disk remains intact"
        );
        service.delete(id).await.unwrap();
        assert_eq!(
            service.client(id).await.unwrap().writeback,
            client.writeback
        );
        assert!(backend.exists(&captured.dataset).unwrap());
        assert!(!backend.exists(&captured.replacement).unwrap());
        assert!(service.status(id).await.unwrap().cache.is_none());
    }
    #[tokio::test]
    async fn target_failure_retains_baseline_and_original_assignment_then_retry_succeeds() {
        let (service, backend, iscsi, client) = fixture().await;
        let id = client.id.as_str();
        let cache = service.capture(id).await.unwrap().cache.unwrap();
        iscsi.fail_create_once.store(true, Ordering::SeqCst);
        assert!(service.apply(id).await.is_err());
        assert_eq!(
            service.client(id).await.unwrap().writeback,
            client.writeback
        );
        assert!(backend.exists(&cache.dataset).unwrap());
        assert!(backend.exists(&cache.replacement).unwrap());
        assert_eq!(
            service.status(id).await.unwrap().cache.unwrap().state,
            "failed"
        );
        service.recover_pending().await.unwrap();
        assert_eq!(
            service.client(id).await.unwrap().writeback,
            Some(cache.replacement)
        );
    }
    #[tokio::test]
    async fn concurrent_capture_keeps_one_baseline_and_cleanup_failure_can_retry() {
        let (service, backend, _, client) = fixture().await;
        let id = client.id.as_str();
        let (a, b) = tokio::join!(service.capture(id), service.capture(id));
        assert_eq!(a.unwrap().cache.unwrap().id, b.unwrap().cache.unwrap().id);
        let cache = service.apply(id).await.unwrap().cache.unwrap();
        backend.fail_destroy_once.store(true, Ordering::SeqCst);
        assert!(service.delete(id).await.is_err());
        assert_eq!(
            service.client(id).await.unwrap().writeback,
            client.writeback
        );
        assert!(backend.exists(&cache.replacement).unwrap());
        assert_eq!(
            service.status(id).await.unwrap().cache.unwrap().state,
            "deleting"
        );
        service.recover_pending().await.unwrap();
        assert!(service.status(id).await.unwrap().cache.is_none());
        assert!(backend.exists(&cache.dataset).unwrap());
    }
    #[tokio::test]
    async fn mounted_active_online_shared_stale_and_changed_source_are_rejected() {
        let (service, backend, iscsi, mut client) = fixture().await;
        let id = client.id.to_string();
        sqlx::query("UPDATE images SET os_type='linux'")
            .execute(&service.deps.repository.pool)
            .await
            .unwrap();
        assert!(service.capture(&id).await.is_err());
        sqlx::query("UPDATE images SET os_type='windows'")
            .execute(&service.deps.repository.pool)
            .await
            .unwrap();
        backend.mounted.store(true, Ordering::SeqCst);
        assert!(service.capture(&id).await.is_err());
        backend.mounted.store(false, Ordering::SeqCst);
        iscsi.active_session.store(true, Ordering::SeqCst);
        assert!(service.capture(&id).await.is_err());
        iscsi.active_session.store(false, Ordering::SeqCst);
        client.status = ClientStatus::Ready;
        service.deps.clients.update(&client).await.unwrap();
        assert!(service.capture(&id).await.is_err());
        client.status = ClientStatus::Offline;
        service.deps.clients.update(&client).await.unwrap();
        let mut other = client.clone();
        other.id = ClientId::new();
        other.name = "other".into();
        other.ip = "192.0.2.2".parse().unwrap();
        other.mac = crate::domain::MacAddress::parse("02:00:00:00:00:02").unwrap();
        service.deps.clients.insert(&other).await.unwrap();
        assert!(service.capture(&id).await.is_err());
        sqlx::query("DELETE FROM clients WHERE id=?")
            .bind(other.id.as_str())
            .execute(&service.deps.repository.pool)
            .await
            .unwrap();
        service.capture(&id).await.unwrap();
        service
            .observe(&id, &["PCI\\VEN_ABCD&DEV_5678".into()], "amd64", "uefi")
            .await
            .unwrap();
        assert!(service.apply(&id).await.is_err());
        assert_eq!(
            service.status(&id).await.unwrap().cache.unwrap().state,
            "stale"
        );
        client.snapshot = Some("pool/master@v2".into());
        service.deps.clients.update(&client).await.unwrap();
        assert!(service.apply(&id).await.is_err());
        assert_eq!(
            service.client(&id).await.unwrap().writeback,
            client.writeback
        );
    }
}
