use crate::{
    domain::storage::{
        ClientStorage, ClientStorageSpec, StorageReconcileResult, StorageSource, StorageState,
        StorageVolume,
    },
    infrastructure::{
        image::ImageBackend,
        iscsi::{
            ChapCredentials, IscsiLunSpec, IscsiProvisionResult, IscsiProvisioner, IscsiTargetSpec,
        },
    },
};
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::sync::Arc;

mod offline_replacement;
pub use offline_replacement::OfflineReplacement;
mod offline_deletion;
pub use offline_deletion::OfflineDeletion;
pub use offline_deletion::{DeletionSafety, OwnedGameClone};

/// Application-level storage orchestration.
///
/// `StorageService` knows what needs to happen for a diskless client,
/// but does not know how ZFS or targetcli implement the operation.
///
/// Infrastructure dependencies are injected through the constructor:
///
/// ```text
/// StorageService
///     │
///     ├── ImageBackend
///     │      └── ZfsImageBackend
///     │
///     └── IscsiProvisioner
///            └── TargetCliProvisioner
/// ```
#[derive(Clone)]
pub struct StorageService {
    config: crate::config::RuntimeConfig,
    image_backend: Arc<dyn ImageBackend>,
    iscsi: Arc<dyn IscsiProvisioner>,
    #[cfg(test)]
    test_game_masters: Option<Vec<GameMaster>>,
    #[cfg(test)]
    test_game_clones: Option<Arc<std::sync::Mutex<Vec<GameCloneRecord>>>>,
}

/// A game "master" volume: an administrator-managed ZVOL holding game
/// installs that per-client writable clones are created from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameMaster {
    /// Full dataset name, e.g. `diskless/games/steam-games`.
    pub dataset: String,
    /// ZFS `org.diskless:type` value (`game` or `game_disk`).
    pub disk_type: String,
    /// Volume size in bytes, when reported by `zfs list`.
    pub size_bytes: Option<u64>,
}

/// A per-client writable game clone derived from a master.
///
/// Clones are independent ZVOLs: client writes never touch the master or
/// any other client's clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameDiskClone {
    /// Master dataset this clone was derived from.
    pub master_dataset: String,
    /// Full clone dataset, e.g. `diskless/games/client-01-steam-games`.
    pub client_clone_dataset: String,
    /// Linux device path of the clone.
    pub block_device: PathBuf,
    /// Always true: clones are writable by design.
    pub writable: bool,
}

/// A persisted per-client game clone discovered on the pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameCloneRecord {
    /// Master dataset recorded at clone time.
    pub master_dataset: String,
    /// Owning client id (`org.diskless:client`).
    pub client_id: String,
    /// Full clone dataset.
    pub clone_dataset: String,
}

/// Make a client id safe for use in ZFS dataset and backstore names.
fn sanitize_client_id(client_id: &str) -> String {
    let sanitized: String = client_id
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if matches!(c, 'a'..='z' | '0'..='9' | '-' | '_' | '.' | ':') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "client".to_string()
    } else {
        sanitized
    }
}

/// Deterministic per-client clone dataset for a game master.
///
/// `("client-01", "diskless/games/steam")` becomes
/// `"diskless/games/client-01-steam-games"`. A master basename that
/// already ends in `-games` (the on-disk naming for created masters)
/// is stripped first so the suffix is never doubled.
pub fn game_clone_dataset(client_id: &str, master: &str) -> String {
    let parent = master.rfind('/').map(|index| &master[..index]);
    let mut base = master.rsplit('/').next().unwrap_or(master);
    base = base.strip_suffix("-games").unwrap_or(base);
    let client = sanitize_client_id(client_id);
    match parent {
        Some(parent) if !parent.is_empty() => {
            format!("{parent}/{client}-{base}-games")
        }
        _ => format!("{client}-{base}-games"),
    }
}

/// Parse `zfs list -o name,org.diskless:type` output into game masters.
///
/// Only volumes tagged `game` or `game_disk` qualify. Per-client
/// clones (`client_game`), untagged volumes, and every other type are
/// excluded so clones are never discovered as masters.
pub fn parse_game_master_datasets(zfs_list_output: &str) -> Vec<String> {
    zfs_list_output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let dataset = fields.next()?.trim();
            let disk_type = fields.next().map(str::trim).unwrap_or_default();
            if !dataset.is_empty() && matches!(disk_type, "game" | "game_disk") {
                Some(dataset.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Describe the per-client writable clones for every game master in a
/// `zfs list` output. Pure metadata: creates nothing.
pub fn game_disks_from_zfs_list(zfs_list_output: &str, client_id: &str) -> Vec<GameDiskClone> {
    parse_game_master_datasets(zfs_list_output)
        .into_iter()
        .map(|master| {
            let clone = game_clone_dataset(client_id, &master);
            GameDiskClone {
                block_device: PathBuf::from(format!("/dev/zvol/{clone}")),
                master_dataset: master,
                client_clone_dataset: clone,
                writable: true,
            }
        })
        .collect()
}

/// Resolve the effective game masters for a client.
///
/// An explicit non-empty stored selection wins (intersected with
/// discovered masters so deleted masters drop out). An empty stored
/// selection combined with the master switch resolves to all
/// discovered masters; a cleared switch resolves to none. Output is
/// sorted so LUN numbering is stable.
pub fn resolve_game_selection(
    use_game_disk: bool,
    stored: &[String],
    discovered: &[String],
) -> Vec<String> {
    let mut selected: Vec<String> = if stored.is_empty() {
        if !use_game_disk {
            return Vec::new();
        }
        discovered.to_vec()
    } else {
        stored
            .iter()
            .filter(|master| discovered.contains(master))
            .cloned()
            .collect()
    };
    selected.sort();
    selected.dedup();
    selected
}

impl StorageService {
    pub fn new(image_backend: Arc<dyn ImageBackend>, iscsi: Arc<dyn IscsiProvisioner>) -> Self {
        Self::with_config(
            image_backend,
            iscsi,
            crate::config::RuntimeConfig::default(),
        )
    }

    pub fn with_config(
        image_backend: Arc<dyn ImageBackend>,
        iscsi: Arc<dyn IscsiProvisioner>,
        config: crate::config::RuntimeConfig,
    ) -> Self {
        Self {
            config,
            image_backend,
            iscsi,
            #[cfg(test)]
            test_game_masters: None,
            #[cfg(test)]
            test_game_clones: None,
        }
    }

    #[cfg(test)]
    pub fn with_test_game_masters(mut self, masters: Vec<GameMaster>) -> Self {
        self.test_game_masters = Some(masters);
        self.test_game_clones = Some(Arc::new(std::sync::Mutex::new(Vec::new())));
        self
    }

    /// Return a shared handle to the startup-selected iSCSI provisioner.
    pub fn iscsi_provisioner(&self) -> Arc<dyn IscsiProvisioner> {
        Arc::clone(&self.iscsi)
    }

    pub fn validate_unmounted(&self, dataset: &str) -> Result<()> {
        self.image_backend
            .validate_unmounted(dataset)
            .with_context(|| format!("storage volume '{dataset}' must be unmounted"))
    }

    pub fn clone_origin(&self, dataset: &str) -> Result<Option<String>> {
        self.image_backend.clone_origin(dataset)
    }

    /// Validate a Super-mode master before removing any existing client storage.
    ///
    /// `dataset` must be an existing managed volume with no local mounts.
    /// Returns an error for invalid names, missing volumes, or failed mount inspection.
    pub fn validate_existing_dataset(&self, dataset: &str) -> Result<()> {
        crate::validation::validate_managed_dataset(
            dataset,
            &crate::config::get_zpool_name(&self.config),
        )
        .map_err(|error| anyhow::anyhow!("invalid existing volume: {error}"))?;
        if !self.image_backend.exists(dataset)? {
            bail!("existing ZFS volume does not exist: {dataset}");
        }
        self.image_backend.validate_unmounted(dataset)
    }

    /// ZFS snapshot on a game master that per-client clones are created from.
    ///
    /// Created on demand when the first client clone is provisioned. ZFS
    /// refuses to destroy a snapshot with dependent clones, which protects
    /// the base while clones exist.
    pub const GAME_BASE_SNAPSHOT: &'static str = "diskless-game-base";

    /// ZFS property value tagging a per-client game clone.
    pub const GAME_CLONE_TYPE: &'static str = "client_game";

    /// Discover all ZFS game-master volumes.
    ///
    /// Masters are volumes tagged `game`/`game_disk`, plus untagged
    /// volumes under a `/games/` path (older installations). Per-client
    /// clones are always tagged `client_game` and are excluded here.
    pub fn discover_game_masters(&self) -> Result<Vec<GameMaster>> {
        #[cfg(test)]
        if let Some(masters) = &self.test_game_masters {
            return Ok(masters.clone());
        }
        use crate::config::get_zpool_name;
        use crate::infrastructure::command::run_command_output_no_sudo;

        let zpool = get_zpool_name(&self.config);
        let output = run_command_output_no_sudo([
            "zfs",
            "list",
            "-H",
            "-p",
            "-t",
            "volume",
            "-o",
            "name,volsize,org.diskless:type",
            "-r",
            &zpool,
        ])
        .map_err(|error| anyhow::anyhow!("failed to list ZFS game disks: {}", error))?;

        Ok(output
            .lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let dataset = fields.next()?.trim();
                let size = fields.next().map(str::trim).unwrap_or_default();
                let disk_type = fields.next().map(str::trim).unwrap_or_default();
                if dataset.is_empty() {
                    return None;
                }
                if disk_type == Self::GAME_CLONE_TYPE {
                    return None;
                }
                if !(matches!(disk_type, "game" | "game_disk") || dataset.contains("/games/")) {
                    return None;
                }
                Some(GameMaster {
                    dataset: dataset.to_string(),
                    disk_type: if disk_type.is_empty() || disk_type == "-" {
                        "game".to_string()
                    } else {
                        disk_type.to_string()
                    },
                    size_bytes: size.parse::<u64>().ok(),
                })
            })
            .collect())
    }

    /// List every per-client game clone on the pool with its owner.
    pub fn list_game_clones(&self) -> Result<Vec<GameCloneRecord>> {
        #[cfg(test)]
        if let Some(records) = &self.test_game_clones {
            return Ok(records
                .lock()
                .map_err(|_| anyhow::anyhow!("test clone registry lock poisoned"))?
                .clone());
        }
        use crate::config::get_zpool_name;
        use crate::infrastructure::command::run_command_output_no_sudo;

        let zpool = get_zpool_name(&self.config);
        let output = run_command_output_no_sudo([
            "zfs",
            "list",
            "-H",
            "-t",
            "volume",
            "-o",
            "name,org.diskless:type,org.diskless:client,org.diskless:master",
            "-r",
            &zpool,
        ])
        .map_err(|error| anyhow::anyhow!("failed to list ZFS game clones: {}", error))?;

        Ok(output
            .lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let dataset = fields.next()?.trim();
                let disk_type = fields.next().map(str::trim).unwrap_or_default();
                let client = fields.next().map(str::trim).unwrap_or_default();
                let master = fields.next().map(str::trim).unwrap_or_default();
                if dataset.is_empty()
                    || disk_type != Self::GAME_CLONE_TYPE
                    || client.is_empty()
                    || client == "-"
                    || master.is_empty()
                    || master == "-"
                {
                    return None;
                }
                Some(GameCloneRecord {
                    master_dataset: master.to_string(),
                    client_id: client.to_string(),
                    clone_dataset: dataset.to_string(),
                })
            })
            .collect())
    }

    /// Stable LIO backstore name for a client's clone of a master.
    pub fn game_backstore_name(client_id: &str, master: &str) -> String {
        let base = master
            .rsplit('/')
            .next()
            .unwrap_or(master)
            .strip_suffix("-games")
            .unwrap_or_else(|| master.rsplit('/').next().unwrap_or(master));
        let sanitize = |value: &str| {
            value
                .chars()
                .map(|c| {
                    if matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_') {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        };
        format!(
            "game_{}_{}",
            sanitize(&sanitize_client_id(client_id)),
            sanitize(base)
        )
    }

    /// Build the desired writable game LUN specs for resolved masters.
    ///
    /// LUN numbering follows the sorted master order so it is stable
    /// across provisions.
    pub fn build_game_luns(client_id: &str, masters: &[String]) -> Vec<IscsiLunSpec> {
        let mut sorted = masters.to_vec();
        sorted.sort();
        sorted
            .iter()
            .enumerate()
            .map(|(index, master)| {
                let clone = game_clone_dataset(client_id, master);
                IscsiLunSpec::new(
                    (index + 1) as u32,
                    Self::game_backstore_name(client_id, master),
                    format!("/dev/zvol/{clone}"),
                )
            })
            .collect()
    }

    /// Build the temporary Super target: writable working image at LUN 0 and
    /// a read-only installer ISO at LUN 1, with game disks omitted.
    pub fn build_super_session_iscsi_spec(
        spec: &ClientStorageSpec,
        installer_iso: impl Into<PathBuf>,
    ) -> Result<IscsiTargetSpec> {
        let mut target = IscsiTargetSpec::with_luns(
            &spec.target_iqn,
            vec![
                IscsiLunSpec::new(0, &spec.backstore, spec.block_device()),
                IscsiLunSpec::readonly_file(
                    1,
                    format!("installer_{}", sanitize_client_id(&spec.client_id)),
                    installer_iso,
                ),
            ],
        )?;
        target.initiator_iqn = Some(spec.initiator_iqn());
        target.chap = spec.chap.clone();
        Ok(target)
    }

    pub fn super_session_working_backstore(working_clone: &str) -> String {
        let working_name = working_clone.rsplit('/').next().unwrap_or(working_clone);
        format!("super_{working_name}")
    }

    pub fn super_session_backstores(client_id: &str, working_clone: &str) -> Vec<String> {
        vec![
            Self::super_session_working_backstore(working_clone),
            format!("installer_{}", sanitize_client_id(client_id)),
        ]
    }

    pub fn verify_super_session_target(&self, desired: &IscsiTargetSpec) -> Result<()> {
        self.iscsi.verify_named_file_lun_mappings(desired)?;
        let actual = self.iscsi.list_target_luns(&desired.target_iqn)?;
        anyhow::ensure!(
            actual.len() == desired.luns.len()
                && desired.luns.iter().all(|expected| {
                    actual
                        .iter()
                        .any(|lun| lun.lun == expected.lun && lun.backstore == expected.backstore)
                }),
            "Super-session target must expose exactly the working disk and installer LUNs"
        );
        anyhow::ensure!(
            self.iscsi.inspect_target(desired)?.is_ready(),
            "Super-session target is incomplete"
        );
        Ok(())
    }

    /// Replace the client's current target with an isolated Super-session target.
    /// The old target object is removed while its normal and game backstores remain owned by the client.
    pub fn create_super_session_target(
        &self,
        spec: &ClientStorageSpec,
        installer_iso: impl Into<PathBuf>,
    ) -> Result<()> {
        self.image_backend
            .validate_unmounted(&spec.dataset)
            .context("Super working image must be unmounted before export")?;
        self.iscsi
            .remove_target(&spec.target_iqn)
            .context("failed to remove the prior client target")?;
        let target = Self::build_super_session_iscsi_spec(spec, installer_iso)?;
        self.iscsi
            .create_target_transaction(&target)
            .context("failed to create the Super-session target")?;
        self.verify_super_session_target(&target)
            .context("Super-session target did not reach the requested LUN state")
    }

    /// Refuse to replace a target unless every current LUN matches this client's normal storage.
    pub fn validate_client_target_for_replacement(&self, spec: &ClientStorageSpec) -> Result<()> {
        self.validate_spec(spec)?;
        if !self.iscsi.target_exists(&spec.target_iqn)? {
            return Ok(());
        }
        let desired = self.desired_iscsi_spec(spec)?;
        let actual = self.iscsi.list_target_luns(&spec.target_iqn)?;
        anyhow::ensure!(
            actual.len() == desired.luns.len()
                && desired.luns.iter().all(|expected| {
                    actual
                        .iter()
                        .any(|lun| lun.lun == expected.lun && lun.backstore == expected.backstore)
                }),
            "existing client target contains unexpected or missing LUN mappings"
        );
        self.iscsi.verify_named_file_lun_mappings(&desired)?;
        anyhow::ensure!(
            self.iscsi.inspect_target(&desired)?.is_ready(),
            "existing client target does not match its assigned storage"
        );
        Ok(())
    }

    /// Accept only the expected LUN subset left by an interrupted Super target creation.
    pub fn validate_super_session_target_for_replacement(
        &self,
        desired: &IscsiTargetSpec,
    ) -> Result<()> {
        if !self.iscsi.target_exists(&desired.target_iqn)? {
            return Ok(());
        }
        let actual = self.iscsi.list_target_luns(&desired.target_iqn)?;
        anyhow::ensure!(
            actual.len() <= desired.luns.len(),
            "existing Super-session target contains unexpected LUN mappings"
        );
        let mut present = desired.clone();
        present.luns.clear();
        for lun in actual {
            let expected = desired
                .lun(lun.lun)
                .filter(|expected| expected.backstore == lun.backstore)
                .context("existing Super-session target contains unexpected LUN mappings")?;
            anyhow::ensure!(
                lun.exists && lun.backstore_exists,
                "existing Super-session target contains an invalid LUN mapping"
            );
            anyhow::ensure!(
                !present.luns.iter().any(|mapped| mapped.lun == lun.lun),
                "existing Super-session target contains duplicate LUN mappings"
            );
            present.luns.push(expected.clone());
        }
        if present.luns.is_empty() {
            // The target may have been created immediately before an interrupted LUN transaction.
            return Ok(());
        }
        self.iscsi.verify_named_file_lun_mappings(&present)?;
        let state = self.iscsi.inspect_target(&present)?;
        anyhow::ensure!(
            state.exists
                && state.luns.len() == present.luns.len()
                && state.luns.iter().all(|lun| lun.is_ready()),
            "existing Super-session target contains an invalid LUN mapping"
        );
        Ok(())
    }

    /// Remove the target and backstores uniquely owned by one Super session.
    /// Normal client and game backstores are intentionally left untouched.
    pub fn remove_super_session_target(
        &self,
        target_iqn: &str,
        owned_backstores: &[String],
    ) -> Result<()> {
        self.iscsi
            .remove_target_with_backstores(target_iqn, owned_backstores)
            .context("failed to remove session-owned target resources")
    }

    fn set_game_clone_props(client_id: &str, master: &str, clone: &str) -> Result<()> {
        #[cfg(test)]
        {
            let _ = (client_id, master, clone);
            Ok(())
        }
        #[cfg(not(test))]
        {
            // The image backend has no generic property setter; tag clones the
            // same way the dataset API tags masters (sudo -n zfs set).
            for (property, value) in [
                ("org.diskless:type", Self::GAME_CLONE_TYPE),
                ("org.diskless:client", client_id),
                ("org.diskless:master", master),
            ] {
                let output = std::process::Command::new("sudo")
                    .args(["-n", "zfs", "set", &format!("{property}={value}"), clone])
                    .output()
                    .with_context(|| {
                        format!("failed to tag game clone '{clone}' ({property}={value})")
                    })?;
                if !output.status.success() {
                    bail!(
                        "failed to tag game clone '{clone}' ({property}={value}): {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                }
            }
            Ok(())
        }
    }

    /// Ensure the base snapshot a client's clone is created from.
    ///
    /// Snapshot creation is idempotent; an existing snapshot is reused so
    /// established clones keep their origin.
    fn ensure_game_base_snapshot(&self, master: &str) -> Result<String> {
        self.image_backend
            .create_snapshot(master, Self::GAME_BASE_SNAPSHOT)
            .with_context(|| format!("failed to ensure game base snapshot for '{master}'"))?;
        Ok(format!("{master}@{}", Self::GAME_BASE_SNAPSHOT))
    }

    /// Ensure a client's writable clone of a master exists and return it.
    pub fn ensure_game_clone(&self, client_id: &str, master: &str) -> Result<String> {
        let clone = game_clone_dataset(client_id, master);
        if !self
            .image_backend
            .exists(&clone)
            .with_context(|| format!("failed to inspect game clone '{clone}'"))?
        {
            let snapshot = self.ensure_game_base_snapshot(master)?;
            self.image_backend
                .clone_image(&snapshot, &clone)
                .with_context(|| {
                    format!("failed to clone game master '{master}' for client '{client_id}'")
                })?;
            Self::set_game_clone_props(client_id, master, &clone)?;
            #[cfg(test)]
            if let Some(records) = &self.test_game_clones {
                records
                    .lock()
                    .expect("test clone registry lock")
                    .push(GameCloneRecord {
                        master_dataset: master.to_string(),
                        client_id: client_id.to_string(),
                        clone_dataset: clone.clone(),
                    });
            }
        }
        if !self
            .image_backend
            .verify(&clone)
            .with_context(|| format!("failed to verify game clone '{clone}'"))?
        {
            bail!("game clone is unavailable after provisioning: '{clone}'");
        }
        Ok(clone)
    }

    /// Enforce (or clear with `None`) one-way CHAP on a live target.
    ///
    /// Applies immediately to new logins; existing sessions are
    /// unaffected. Skips targets that were never provisioned (their first
    /// provision enforces). Used by auth flips and secret rotation to
    /// avoid a full storage rebuild.
    pub fn set_target_chap(&self, target_iqn: &str, chap: Option<&ChapCredentials>) -> Result<()> {
        if !self.iscsi.target_exists(target_iqn)? {
            return Ok(());
        }
        self.iscsi
            .set_chap_auth(target_iqn, chap)
            .with_context(|| format!("failed to apply CHAP change on target '{target_iqn}'"))
    }

    /// Remove every `game_*` LUN/backstore on a target that is not desired.
    ///
    /// Only backstores starting with `game_` are touched; the boot
    /// backstore (`block_*`) and anything else are left intact. This both
    /// migrates legacy shared read-only attachments and enforces
    /// deselection.
    pub fn prune_game_luns(&self, target_iqn: &str, desired_backstores: &[String]) -> Result<()> {
        let stale: Vec<String> = self
            .iscsi
            .list_target_backstores(target_iqn)?
            .into_iter()
            .filter(|backstore| {
                backstore.starts_with("game_") && !desired_backstores.contains(backstore)
            })
            .collect();
        if stale.is_empty() {
            return Ok(());
        }
        tracing::info!(
            target_iqn = %target_iqn,
            backstores = ?stale,
            "pruning stale game LUNs"
        );
        self.iscsi
            .remove_target_with_backstores(target_iqn, &stale)
            .with_context(|| format!("failed to prune game LUNs from target '{target_iqn}'"))?;
        Ok(())
    }

    /// Ensure all resolved game clones exist and are exposed, then prune
    /// anything stale. Used by provisioning and repair paths.
    pub fn ensure_game_storage(
        &self,
        client_id: &str,
        target_iqn: &str,
        masters: &[String],
    ) -> Result<Vec<IscsiLunSpec>> {
        for master in masters {
            self.ensure_game_clone(client_id, master)?;
        }
        let luns = Self::build_game_luns(client_id, masters);
        let desired: Vec<String> = luns.iter().map(|lun| lun.backstore.clone()).collect();
        self.prune_game_luns(target_iqn, &desired)?;
        Ok(luns)
    }

    /// Destroy and recreate a client's game clones from the base snapshot.
    ///
    /// The clone keeps its dataset name, so the `/dev/zvol` path and any
    /// attached LUN survive the reset. The caller must establish that the
    /// client is offline. Super (persistent) clients never reach this:
    /// their clones are intentionally kept across reboots.
    ///
    /// `chap` mirrors portal enforcement (see `sync_game_storage`).
    pub fn reset_game_clones(
        &self,
        client_id: &str,
        client_name: &str,
        target_iqn: &str,
        masters: &[String],
        chap: Option<&ChapCredentials>,
    ) -> Result<()> {
        for master in masters {
            let clone = game_clone_dataset(client_id, master);
            if self.image_backend.exists(&clone)? {
                tracing::info!(
                    client_id = %client_id,
                    clone = %clone,
                    "resetting per-client game clone"
                );
                self.image_backend
                    .destroy(&clone)
                    .with_context(|| format!("failed to destroy game clone '{clone}' for reset"))?;
            }
            self.ensure_game_clone(client_id, master)?;
        }
        // Re-add any LUN missing after the swap; existing LUNs keep
        // working because the device path is unchanged. Stale game LUNs
        // are pruned first so their numbers are free: `create_target`
        // only adds LUNs whose numbers are unoccupied.
        let desired: Vec<String> = Self::build_game_luns(client_id, masters)
            .into_iter()
            .map(|lun| lun.backstore)
            .collect();
        self.prune_game_luns(target_iqn, &desired)?;
        let luns = Self::build_game_luns(client_id, masters);
        if !luns.is_empty() {
            let mut spec = IscsiTargetSpec::with_luns(target_iqn, luns)?;
            spec.initiator_iqn = Some(crate::infrastructure::pxe::client_initiator_iqn(
                client_name,
            ));
            spec.chap = chap.cloned();
            self.iscsi.create_target(&spec).with_context(|| {
                format!("failed to re-expose game LUNs for client '{client_id}'")
            })?;
        }
        Ok(())
    }

    /// Detach and destroy every game clone owned by a client.
    ///
    /// Used by client deletion and full game disablement. LUNs and
    /// backstores are removed before the datasets so no dangling
    /// references survive. Failures to remove already-absent resources
    /// are tolerated by the underlying idempotent calls.
    pub fn destroy_client_game_clones(
        &self,
        client_id: &str,
        target_iqn: Option<&str>,
    ) -> Result<()> {
        if let Some(target_iqn) = target_iqn {
            // Prune with an empty desired set: every game LUN goes away.
            self.prune_game_luns(target_iqn, &[])?;
        }
        let owned: Vec<GameCloneRecord> = self
            .list_game_clones()?
            .into_iter()
            .filter(|record| record.client_id == client_id)
            .collect();
        for record in owned {
            if self.image_backend.exists(&record.clone_dataset)? {
                tracing::info!(
                    client_id = %client_id,
                    clone = %record.clone_dataset,
                    "destroying per-client game clone"
                );
                self.image_backend.destroy(&record.clone_dataset)?;
            }
        }
        Ok(())
    }

    /// Synchronize game attachments without touching the boot clone.
    ///
    /// Used when only the game configuration changed: missing clones are
    /// created, desired LUNs are (re-)exposed, stale LUNs are pruned, and
    /// clone datasets that are no longer selected are destroyed. The boot
    /// LUN is never modified.
    ///
    /// `chap` must mirror the target's enforcement: `create_target`
    /// reconfigures the portal, and a `None` here would silently drop
    /// authentication.
    pub fn sync_game_storage(
        &self,
        client_id: &str,
        client_name: &str,
        target_iqn: &str,
        masters: &[String],
        chap: Option<&ChapCredentials>,
    ) -> Result<()> {
        for master in masters {
            self.ensure_game_clone(client_id, master)?;
        }
        let luns = Self::build_game_luns(client_id, masters);
        let desired: Vec<String> = luns.iter().map(|lun| lun.backstore.clone()).collect();
        // Prune before exposing: stale LUNs occupying desired numbers are
        // removed first so `create_target` attaches the fresh clones.
        self.prune_game_luns(target_iqn, &desired)?;
        if !luns.is_empty() {
            let mut spec = IscsiTargetSpec::with_luns(target_iqn, luns)?;
            spec.initiator_iqn = Some(crate::infrastructure::pxe::client_initiator_iqn(
                client_name,
            ));
            spec.chap = chap.cloned();
            // Additive and idempotent: existing LUNs are left alone.
            self.iscsi
                .create_target(&spec)
                .with_context(|| format!("failed to expose game LUNs for client '{client_id}'"))?;
        }

        // Destroy clone datasets that are no longer selected. Their LUNs
        // are already detached by the prune above.
        self.destroy_game_clones_except(client_id, masters)?;
        Ok(())
    }

    /// Destroy a client's game clones whose master is not wanted.
    ///
    /// LUNs must already be detached (prune first); this only removes the
    /// ZVOLs so deselected data does not accumulate on the pool.
    pub fn destroy_game_clones_except(
        &self,
        client_id: &str,
        wanted_masters: &[String],
    ) -> Result<()> {
        let wanted: Vec<String> = wanted_masters
            .iter()
            .map(|master| game_clone_dataset(client_id, master))
            .collect();
        for record in self
            .list_game_clones()?
            .into_iter()
            .filter(|record| record.client_id == client_id)
        {
            if !wanted.contains(&record.clone_dataset)
                && self.image_backend.exists(&record.clone_dataset)?
            {
                tracing::info!(
                    client_id = %client_id,
                    clone = %record.clone_dataset,
                    "destroying deselected game clone"
                );
                self.image_backend.destroy(&record.clone_dataset)?;
            }
        }
        Ok(())
    }

    // ====================================================================
    // CREATE
    // ====================================================================

    /// Create client storage.
    ///
    /// This compatibility method preserves the original API.
    ///
    /// When callers need transaction ownership information they should
    /// use `create_client_storage_transaction()`.
    pub fn create_client_storage(&self, spec: &ClientStorageSpec) -> Result<ClientStorage> {
        let (storage, _) = self.create_client_storage_transaction(spec)?;
        Ok(storage)
    }

    /// Create client storage and return the exact iSCSI resources
    /// created by this operation.
    ///
    /// The returned `IscsiProvisionResult` is authoritative for
    /// transaction rollback.
    ///
    /// Existing resources are deliberately not reported as created.
    pub fn create_client_storage_transaction(
        &self,
        spec: &ClientStorageSpec,
    ) -> Result<(ClientStorage, IscsiProvisionResult)> {
        self.create_client_storage_transaction_inner(spec, true, false)
    }

    /// Ensure boot storage additively. Existing client data is never removed;
    /// missing target pieces are created and conflicting pieces fail safely.
    pub fn ensure_client_storage_for_boot(
        &self,
        spec: &ClientStorageSpec,
    ) -> Result<ClientStorage> {
        let mut spec = spec.clone();
        let original_source = spec.source.clone();
        let expected_origin = match &original_source {
            StorageSource::Snapshot(snapshot) => Some(snapshot.as_str()),
            _ => None,
        };
        let exists = self.image_backend.exists(&spec.dataset)?;
        if exists {
            self.image_backend.validate_unmounted(&spec.dataset)?;
            if let Some(origin) = expected_origin {
                let actual = self.image_backend.clone_origin(&spec.dataset)?;
                anyhow::ensure!(
                    actual.as_deref() == Some(origin),
                    "client clone '{}' has unexpected origin",
                    spec.dataset
                );
                spec.source = StorageSource::ExistingVolume(spec.dataset.clone());
            }
        } else if !matches!(spec.source, StorageSource::Snapshot(_)) {
            bail!("configured client volume is missing: {}", spec.dataset);
        }

        if spec.use_game_disk {
            let discovered = self.discover_game_masters()?;
            let names = discovered
                .into_iter()
                .map(|master| master.dataset)
                .collect::<Vec<_>>();
            let selected = resolve_game_selection(true, &spec.game_disks, &names);
            for master in &selected {
                let clone = game_clone_dataset(&spec.client_id, master);
                if self.image_backend.exists(&clone)? {
                    let owned = self.list_game_clones()?;
                    anyhow::ensure!(
                        owned.iter().any(|record| {
                            record.client_id == spec.client_id
                                && record.master_dataset == *master
                                && record.clone_dataset == clone
                        }),
                        "existing game clone '{}' is not recorded as owned by this client",
                        clone
                    );
                    self.image_backend.validate_unmounted(&clone)?;
                    let origin = self.image_backend.clone_origin(&clone)?;
                    let expected = format!("{master}@{}", Self::GAME_BASE_SNAPSHOT);
                    anyhow::ensure!(
                        origin.as_deref() == Some(expected.as_str()),
                        "game clone '{}' has unexpected origin",
                        clone
                    );
                }
            }
        }

        let discovered = if spec.use_game_disk {
            let masters = self.discover_game_masters()?;
            masters
                .into_iter()
                .map(|master| master.dataset)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let selected = resolve_game_selection(spec.use_game_disk, &spec.game_disks, &discovered);
        for master in &selected {
            let clone = self.ensure_game_clone(&spec.client_id, master)?;
            self.image_backend.validate_unmounted(&clone)?;
            let origin = self.image_backend.clone_origin(&clone)?;
            let expected = format!("{master}@{}", Self::GAME_BASE_SNAPSHOT);
            anyhow::ensure!(
                origin.as_deref() == Some(expected.as_str()),
                "game clone '{}' has unexpected origin",
                clone
            );
        }
        self.image_backend.settle_device_changes()?;
        if exists && !self.image_backend.verify(&spec.dataset)? {
            bail!("configured client volume is unavailable: {}", spec.dataset);
        }
        for master in &selected {
            let clone = game_clone_dataset(&spec.client_id, master);
            if !self.image_backend.verify(&clone)? {
                bail!("selected game clone is unavailable: {clone}");
            }
        }

        let desired = self.desired_iscsi_spec(&spec)?;
        let current = self.iscsi.inspect_target(&desired)?;
        if exists && current.is_ready() {
            self.iscsi.ensure_initiator_acl(&desired)?;
            self.set_target_chap(&spec.target_iqn, spec.chap.as_ref())?;
            let mut storage = self.storage_from_spec(&spec);
            storage.source = original_source;
            self.verify_boot_target(&desired)?;
            return Ok(storage);
        }
        if current.exists {
            anyhow::ensure!(
                !self.target_connected_if_present(&spec.target_iqn)?,
                "client storage is incomplete while the client is connected"
            );
        }
        let (mut storage, _) = self.create_client_storage_transaction_inner(&spec, false, true)?;
        // Keep ownership/source metadata authoritative for callers even though
        // the additive transaction treated a pre-existing clone as shared.
        storage.source = original_source;
        let desired = self.desired_iscsi_spec(&spec)?;
        self.iscsi.ensure_initiator_acl(&desired)?;
        self.set_target_chap(&spec.target_iqn, spec.chap.as_ref())?;
        self.image_backend.settle_device_changes()?;
        anyhow::ensure!(
            self.image_backend.verify(&spec.dataset)?,
            "configured client volume is unavailable: {}",
            spec.dataset
        );
        for master in &selected {
            let clone = game_clone_dataset(&spec.client_id, master);
            anyhow::ensure!(
                self.image_backend.verify(&clone)?,
                "selected game clone is unavailable: {clone}"
            );
        }
        self.verify_boot_target(&desired)?;
        Ok(storage)
    }

    fn verify_boot_target(&self, desired: &IscsiTargetSpec) -> Result<()> {
        let actual = self.iscsi.inspect_target(desired)?;
        anyhow::ensure!(
            actual.is_ready(),
            "iSCSI target is not ready after provisioning"
        );
        Ok(())
    }

    fn create_client_storage_transaction_inner(
        &self,
        spec: &ClientStorageSpec,
        prune_stale_game_luns: bool,
        settle_devices: bool,
    ) -> Result<(ClientStorage, IscsiProvisionResult)> {
        let started = std::time::Instant::now();
        self.validate_spec(spec)?;

        let block_device = spec.block_device();

        tracing::info!(
            client_id = %spec.client_id,
            dataset = %spec.dataset,
            target_iqn = %spec.target_iqn,
            source = %spec.source.value(),
            use_game_disk = spec.use_game_disk,
            "creating client storage"
        );

        // ---------------------------------------------------------------
        // Step 1: Prepare ZFS resource.
        // ---------------------------------------------------------------

        match &spec.source {
            StorageSource::Snapshot(snapshot) => {
                if self.image_backend.exists(&spec.dataset)? {
                    bail!("client storage dataset already exists: {}", spec.dataset);
                }

                self.image_backend
                    .clone_image(snapshot, &spec.dataset)
                    .with_context(|| {
                        format!(
                            "failed to create ZFS clone '{}' from '{}'",
                            spec.dataset, snapshot
                        )
                    })?;

                let exists = self
                    .image_backend
                    .exists(&spec.dataset)
                    .context("failed to verify newly created client ZFS clone")?;

                if !exists {
                    let _ = self.image_backend.destroy(&spec.dataset);

                    bail!(
                        "ZFS clone was reported successful but does not exist: {}",
                        spec.dataset
                    );
                }
            }

            StorageSource::ExistingVolume(dataset) => {
                crate::validation::validate_dataset_name(dataset)
                    .map_err(|error| anyhow::anyhow!("invalid existing volume: {error}"))?;
                if dataset != &spec.dataset {
                    bail!(
                        "existing volume source '{}' does not match destination '{}'",
                        dataset,
                        spec.dataset
                    );
                }

                if !self.image_backend.exists(&spec.dataset)? {
                    bail!("existing ZFS volume does not exist: {}", spec.dataset);
                }

                tracing::debug!(
                    client_id = %spec.client_id,
                    dataset = %spec.dataset,
                    "using existing ZFS volume; resource is not client-owned"
                );
            }

            StorageSource::ExistingClientVolume(dataset) => {
                crate::validation::validate_dataset_name(dataset)
                    .map_err(|error| anyhow::anyhow!("invalid existing client volume: {error}"))?;
                if dataset != &spec.dataset {
                    bail!(
                        "existing client volume source '{}' does not match destination '{}'",
                        dataset,
                        spec.dataset
                    );
                }

                if !self.image_backend.exists(&spec.dataset)? {
                    bail!(
                        "existing client ZFS volume does not exist: {}",
                        spec.dataset
                    );
                }

                tracing::debug!(
                    client_id = %spec.client_id,
                    dataset = %spec.dataset,
                    "using existing client-owned ZFS volume"
                );
            }
        }

        // ---------------------------------------------------------------
        // Step 2: Build iSCSI LUN specification.
        // ---------------------------------------------------------------

        let mut luns = vec![IscsiLunSpec::new(spec.lun, &spec.backstore, &block_device)];

        // Desired per-client game backstores, used to prune stale
        // attachments after the target transaction succeeds.
        let mut desired_game_backstores: Vec<String> = Vec::new();

        if spec.use_game_disk {
            let discovered = self.discover_game_masters()?;
            let discovered_names: Vec<String> = discovered
                .into_iter()
                .map(|master| master.dataset)
                .collect();
            let selected =
                resolve_game_selection(spec.use_game_disk, &spec.game_disks, &discovered_names);
            for master in &selected {
                // Clones must exist before the target transaction
                // validates the LUN device paths.
                self.ensure_game_clone(&spec.client_id, master)?;
            }
            let game_luns = Self::build_game_luns(&spec.client_id, &selected);
            desired_game_backstores = game_luns.iter().map(|lun| lun.backstore.clone()).collect();
            tracing::info!(
                client_id = %spec.client_id,
                game_disk_count = game_luns.len(),
                "attaching per-client game clones to client iSCSI target"
            );
            luns.extend(game_luns);
        }

        if settle_devices {
            self.image_backend
                .settle_device_changes()
                .context("failed to settle client ZVOL devices before iSCSI provisioning")?;
        }

        let mut iscsi_spec = IscsiTargetSpec::with_luns(&spec.target_iqn, luns)?;
        iscsi_spec.initiator_iqn = Some(spec.initiator_iqn());
        // Enforcement travels with every provision: configure_tpg applies
        // (or explicitly clears) target authentication from this field.
        iscsi_spec.chap = spec.chap.clone();

        // ---------------------------------------------------------------
        // Step 2b: Prune stale game attachments BEFORE creating LUNs.
        // ---------------------------------------------------------------
        //
        // Legacy shared read-only LUNs and deselected clones are removed
        // here, freeing their LUN numbers for the desired attachments:
        // `create_lun` skips occupied numbers, so pruning must come first.
        // Only `game_*` backstores are touched. A prune failure is logged
        // but does not fail provisioning: the transaction below still
        // converges the desired state, and the next provision or repair
        // retries the prune.
        if prune_stale_game_luns {
            if let Err(error) = self.prune_game_luns(&spec.target_iqn, &desired_game_backstores) {
                tracing::error!(
                    client_id = %spec.client_id,
                    target_iqn = %spec.target_iqn,
                    error = %error,
                    "failed to prune stale game LUNs before provisioning"
                );
            }
        }

        // ---------------------------------------------------------------
        // Step 3: Create iSCSI transactionally.
        // ---------------------------------------------------------------

        let iscsi_result = match self.iscsi.create_target_transaction(&iscsi_spec) {
            Ok(result) => result,

            Err(error) => {
                tracing::error!(
                    client_id = %spec.client_id,
                    dataset = %spec.dataset,
                    target_iqn = %spec.target_iqn,
                    error = %error,
                    "iSCSI provisioning failed"
                );

                // The iSCSI provisioner already rolled back its own
                // partially-created resources.
                //
                // StorageService only owns the ZFS resource when
                // the source declares ownership.
                if spec.owns_dataset() {
                    if let Err(cleanup_error) = self.image_backend.destroy(&spec.dataset) {
                        tracing::error!(
                            client_id = %spec.client_id,
                            dataset = %spec.dataset,
                            error = %cleanup_error,
                            "failed to rollback ZFS client resource after iSCSI failure"
                        );

                        return Err(error).context(format!(
                            "iSCSI provisioning failed and ZFS rollback also failed: {}",
                            cleanup_error
                        ));
                    }
                }

                return Err(error).with_context(|| {
                    format!(
                        "failed to provision iSCSI storage for client '{}'",
                        spec.client_id
                    )
                });
            }
        };

        // ---------------------------------------------------------------
        // Step 4: Build application result.
        // ---------------------------------------------------------------

        let volume = StorageVolume::new(
            spec.dataset.clone(),
            block_device,
            spec.backstore.clone(),
            spec.target_iqn.clone(),
            spec.lun,
        );

        let storage = ClientStorage {
            client_id: spec.client_id.clone(),
            source: spec.source.clone(),
            volume,
            use_game_disk: spec.use_game_disk,
        };

        tracing::info!(
            client_id = %spec.client_id,
            target_iqn = %spec.target_iqn,
            target_created = iscsi_result.target_created,
            portal_created = iscsi_result.portal_created,
            luns_created = ?iscsi_result.luns_created,
            backstores_created = ?iscsi_result.backstores_created,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "client storage provisioned"
        );

        Ok((storage, iscsi_result))
    }

    // ====================================================================
    // DESTROY
    // ====================================================================

    /// Destroy the client's storage resources.
    pub fn destroy_client_storage(&self, storage: &ClientStorage) -> Result<()> {
        tracing::info!(
            client_id = %storage.client_id,
            dataset = %storage.dataset(),
            target_iqn = %storage.target_iqn(),
            owns_dataset = storage.owns_dataset(),
            use_game_disk = storage.use_game_disk,
            "destroying client storage"
        );

        let owned_backstores = vec![storage.backstore().to_string()];

        self.iscsi
            .remove_target_with_backstores(storage.target_iqn(), &owned_backstores)
            .with_context(|| format!("failed to remove iSCSI target '{}'", storage.target_iqn()))?;

        if storage.owns_dataset() && self.image_backend.exists(storage.dataset())? {
            self.image_backend
                .destroy(storage.dataset())
                .with_context(|| {
                    format!(
                        "failed to destroy client ZFS storage '{}'",
                        storage.dataset()
                    )
                })?;
        } else {
            tracing::debug!(
                client_id = %storage.client_id,
                dataset = %storage.dataset(),
                "no owned ZFS volume to remove"
            );
        }

        Ok(())
    }

    // ====================================================================
    // RESET
    // ====================================================================

    /// Reset a client's storage from its desired source.
    pub fn reset_client_storage(
        &self,
        current: &ClientStorage,
        spec: &ClientStorageSpec,
    ) -> Result<ClientStorage> {
        if current.client_id != spec.client_id {
            bail!(
                "storage client mismatch: current='{}', requested='{}'",
                current.client_id,
                spec.client_id
            );
        }

        if !spec.owns_dataset() {
            bail!("cannot reset client storage from an existing shared volume");
        }

        tracing::info!(
            client_id = %spec.client_id,
            old_dataset = %current.dataset(),
            new_dataset = %spec.dataset,
            "resetting client storage"
        );

        let owned_backstores = vec![current.backstore().to_string()];

        self.iscsi
            .remove_target_with_backstores(current.target_iqn(), &owned_backstores)
            .with_context(|| {
                format!(
                    "failed to remove existing iSCSI target '{}'",
                    current.target_iqn()
                )
            })?;

        if current.owns_dataset() && self.image_backend.exists(current.dataset())? {
            self.image_backend
                .destroy(current.dataset())
                .with_context(|| {
                    format!(
                        "failed to destroy existing client storage '{}'",
                        current.dataset()
                    )
                })?;
        }

        self.create_client_storage(spec)
            .context("client storage reset failed")
    }

    // ====================================================================
    // TARGET REMOVAL
    // ====================================================================

    /// Remove a client's iSCSI target without destroying ZFS storage.
    ///
    /// `backstores` must contain only backstores owned by the caller.
    pub fn remove_client_target(&self, target_iqn: &str, backstores: &[String]) -> Result<()> {
        if target_iqn.trim().is_empty() {
            bail!("target IQN cannot be empty");
        }

        let owned_backstores: Vec<String> = backstores
            .iter()
            .filter(|item| !item.trim().is_empty())
            .cloned()
            .collect();

        if owned_backstores.is_empty() {
            self.iscsi
                .remove_target(target_iqn)
                .with_context(|| format!("failed to remove iSCSI target '{}'", target_iqn))
        } else {
            self.iscsi
                .remove_target_with_backstores(target_iqn, &owned_backstores)
                .with_context(|| {
                    format!(
                        "failed to remove iSCSI target '{}' and owned backstores",
                        target_iqn
                    )
                })
        }
    }

    /// Replace a legacy direct-master mapping with the selected client-owned clone.
    /// The old assignment is only an iSCSI mapping; this never destroys its ZVOL.
    pub fn exit_legacy_super(
        &self,
        target_iqn: &str,
        old_backstore: &str,
        spec: &ClientStorageSpec,
        selected_snapshot: &str,
    ) -> Result<ClientStorage> {
        let StorageSource::Snapshot(snapshot) = &spec.source else {
            bail!("legacy Super exit requires a snapshot-backed assignment");
        };
        anyhow::ensure!(
            snapshot == selected_snapshot
                && snapshot.split('@').next() != Some(spec.dataset.as_str()),
            "selected snapshot does not match the client clone assignment"
        );
        self.remove_client_target(target_iqn, &[old_backstore.to_owned()])?;
        let mut desired = spec.clone();
        if self.image_backend.exists(&desired.dataset)? {
            self.image_backend.validate_unmounted(&desired.dataset)?;
            anyhow::ensure!(
                self.image_backend
                    .clone_origin(&desired.dataset)?
                    .as_deref()
                    == Some(selected_snapshot),
                "existing recovery clone has unexpected snapshot origin"
            );
            desired.source = StorageSource::ExistingClientVolume(desired.dataset.clone());
        }
        self.create_client_storage(&desired)
    }

    // ====================================================================
    // RECONCILIATION
    // ====================================================================

    /// Inspect current storage state without changing anything.
    pub fn reconcile_client_storage(
        &self,
        spec: &ClientStorageSpec,
    ) -> Result<StorageReconcileResult> {
        self.validate_spec(spec)?;

        let zfs_present = self
            .image_backend
            .exists(&spec.dataset)
            .context("failed to inspect ZFS client storage")?;

        let iscsi_spec = self.desired_iscsi_spec(spec)?;

        let iscsi_state = self
            .iscsi
            .inspect_target(&iscsi_spec)
            .context("failed to inspect iSCSI client storage")?;

        let iscsi_present = iscsi_state.is_ready();

        let state = match (zfs_present, iscsi_present) {
            (false, false) => StorageState::Missing,
            (true, true) => StorageState::Ready,
            _ => StorageState::Partial,
        };

        Ok(StorageReconcileResult {
            state,
            zfs_present,
            iscsi_present,
            target_iqn: spec.target_iqn.clone(),
            dataset: spec.dataset.clone(),
        })
    }

    fn desired_iscsi_spec(&self, spec: &ClientStorageSpec) -> Result<IscsiTargetSpec> {
        let mut iscsi_spec = if spec.use_game_disk {
            let mut luns = vec![IscsiLunSpec::new(
                spec.lun,
                &spec.backstore,
                spec.block_device(),
            )];

            // Desired state only: clone dataset names are deterministic,
            // so missing clones inspect as not-ready and repair creates
            // them. Inspect never mutates storage.
            let discovered = self.discover_game_masters()?;
            let discovered_names: Vec<String> = discovered
                .into_iter()
                .map(|master| master.dataset)
                .collect();
            let selected =
                resolve_game_selection(spec.use_game_disk, &spec.game_disks, &discovered_names);
            luns.extend(Self::build_game_luns(&spec.client_id, &selected));

            IscsiTargetSpec::with_luns(&spec.target_iqn, luns)?
        } else {
            IscsiTargetSpec::new(
                &spec.target_iqn,
                &spec.backstore,
                spec.block_device(),
                spec.lun,
            )
        };
        iscsi_spec.initiator_iqn = Some(spec.initiator_iqn());
        iscsi_spec.chap = spec.chap.clone();

        Ok(iscsi_spec)
    }

    /// Reconcile actual infrastructure to desired state.
    pub fn reconcile_client_storage_in_place(
        &self,
        spec: &ClientStorageSpec,
    ) -> Result<ClientStorage> {
        let state = self.reconcile_client_storage(spec)?;

        match state.state {
            StorageState::Ready => {
                self.iscsi
                    .ensure_initiator_acl(&self.desired_iscsi_spec(spec)?)?;
                Ok(self.storage_from_spec(spec))
            }

            StorageState::Missing => self.create_client_storage(spec),

            StorageState::Partial => self.repair_partial_storage(spec),

            StorageState::InUse => {
                bail!("client storage '{}' is currently in use", spec.client_id);
            }

            StorageState::Error => {
                bail!("client storage '{}' is in an error state", spec.client_id);
            }
        }
    }

    // ====================================================================
    // PARTIAL REPAIR
    // ====================================================================

    fn repair_partial_storage(&self, spec: &ClientStorageSpec) -> Result<ClientStorage> {
        tracing::warn!(
            client_id = %spec.client_id,
            dataset = %spec.dataset,
            target_iqn = %spec.target_iqn,
            "repairing partially provisioned client storage"
        );

        if self.iscsi.target_exists(&spec.target_iqn)? {
            let owned_backstores = vec![spec.backstore.clone()];

            self.iscsi
                .remove_target_with_backstores(&spec.target_iqn, &owned_backstores)
                .with_context(|| {
                    format!(
                        "failed to remove partial iSCSI target '{}'",
                        spec.target_iqn
                    )
                })?;
        } else {
            self.iscsi
                .remove_target_with_backstores(
                    &spec.target_iqn,
                    std::slice::from_ref(&spec.backstore),
                )
                .with_context(|| {
                    format!(
                        "failed to remove partial iSCSI backstore '{}'",
                        spec.backstore
                    )
                })?;
        }

        if spec.owns_dataset() && self.image_backend.exists(&spec.dataset)? {
            self.image_backend.destroy(&spec.dataset).with_context(|| {
                format!(
                    "failed to remove partial client ZFS storage '{}'",
                    spec.dataset
                )
            })?;
        }

        self.create_client_storage(spec)
            .context("failed to repair partial client storage")
    }

    // ====================================================================
    // HELPERS
    // ====================================================================

    fn storage_from_spec(&self, spec: &ClientStorageSpec) -> ClientStorage {
        ClientStorage {
            client_id: spec.client_id.clone(),
            source: spec.source.clone(),
            volume: StorageVolume::new(
                spec.dataset.clone(),
                spec.block_device(),
                spec.backstore.clone(),
                spec.target_iqn.clone(),
                spec.lun,
            ),
            use_game_disk: spec.use_game_disk,
        }
    }

    fn validate_spec(&self, spec: &ClientStorageSpec) -> Result<()> {
        if spec.client_id.trim().is_empty() {
            bail!("client ID cannot be empty");
        }

        if spec.client_name.trim().is_empty() {
            bail!("client name cannot be empty");
        }

        if spec.dataset.trim().is_empty() {
            bail!("ZFS client dataset cannot be empty");
        }

        if spec.backstore.trim().is_empty() {
            bail!("iSCSI backstore cannot be empty");
        }

        if spec.target_iqn.trim().is_empty() {
            bail!("iSCSI target IQN cannot be empty");
        }

        match &spec.source {
            StorageSource::Snapshot(snapshot) => {
                if snapshot.trim().is_empty() {
                    bail!("source snapshot cannot be empty");
                }

                if !snapshot.contains('@') {
                    bail!("source snapshot must be a ZFS snapshot: '{}'", snapshot);
                }
            }

            StorageSource::ExistingVolume(volume) => {
                if volume.trim().is_empty() {
                    bail!("existing volume cannot be empty");
                }
            }

            StorageSource::ExistingClientVolume(volume) => {
                if volume.trim().is_empty() {
                    bail!("existing client volume cannot be empty");
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod game_storage_tests {
    use super::{
        game_clone_dataset, game_disks_from_zfs_list, parse_game_master_datasets, StorageService,
    };
    use crate::domain::storage::StorageSource;
    use std::path::PathBuf;

    #[test]
    fn clone_dataset_name_is_deterministic_for_client_and_master() {
        assert_eq!(
            game_clone_dataset("client-01", "diskless/games/steam"),
            "diskless/games/client-01-steam-games"
        );
        assert_eq!(
            game_clone_dataset("client-01", "diskless/games/steam"),
            "diskless/games/client-01-steam-games"
        );
    }

    #[test]
    fn zfs_volume_parser_returns_only_tagged_game_masters() {
        let output = "diskless/games/steam\tgame\n\
                      diskless/archive/gog\tgame_disk\n\
                      diskless/CLIENT-01-disk\twriteback\n\
                      diskless/images/windows\timage\n";

        assert_eq!(
            parse_game_master_datasets(output),
            vec!["diskless/games/steam", "diskless/archive/gog"]
        );
    }

    #[test]
    fn client_game_clone_is_never_discovered_as_a_master() {
        let output = "diskless/games/steam\tgame\n\
                      diskless/games/client-01-steam-games\tclient_game\n\
                      diskless/games/untagged-legacy-clone\t-\n";

        assert_eq!(
            parse_game_master_datasets(output),
            vec!["diskless/games/steam"]
        );
    }

    #[test]
    fn client_game_disk_metadata_uses_the_clone_as_a_writable_block_device() {
        let output = "diskless/games/steam\tgame\n";

        let disks = game_disks_from_zfs_list(output, "client-01");

        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].master_dataset, "diskless/games/steam");
        assert_eq!(
            disks[0].client_clone_dataset,
            "diskless/games/client-01-steam-games"
        );
        assert_eq!(
            disks[0].block_device,
            PathBuf::from("/dev/zvol/diskless/games/client-01-steam-games")
        );
        assert!(disks[0].writable);
    }

    #[test]
    fn super_session_target_uses_lun_zero_and_readonly_iso_lun_one_without_games() {
        let spec = crate::domain::storage::ClientStorageSpec {
            client_id: "pc001".into(),
            client_name: "pc001".into(),
            source: StorageSource::ExistingClientVolume("pool/pc001".into()),
            dataset: "pool/pc001".into(),
            backstore: "block_pc001".into(),
            target_iqn: "iqn.test:pc001".into(),
            lun: 0,
            use_game_disk: true,
            game_disks: vec!["pool/game".into()],
            chap: None,
        };
        let target =
            StorageService::build_super_session_iscsi_spec(&spec, "/srv/iso/install.iso").unwrap();
        assert_eq!(target.luns.len(), 2);
        assert_eq!(target.luns[0].lun, 0);
        assert_eq!(target.luns[1].lun, 1);
        assert_eq!(
            target.luns[1].backing,
            crate::infrastructure::iscsi::IscsiLunBacking::File
        );
        assert!(target.luns[1].readonly);
    }

    #[test]
    fn super_session_cleanup_names_only_session_owned_luns() {
        assert_eq!(
            StorageService::super_session_backstores("PC 001", "pool/super-session-123"),
            vec!["super_super-session-123", "installer_pc_001"]
        );
    }
}
