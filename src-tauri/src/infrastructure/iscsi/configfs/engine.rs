use super::{fs::ConfigFs, identifier, validate_chap, validate_spec, Operation, Reply};
use crate::infrastructure::iscsi::{
    ChapCredentials, IscsiLunBacking, IscsiLunSpec, IscsiLunState, IscsiProvisionResult,
    IscsiTargetSpec, IscsiTargetState,
};
use anyhow::{bail, ensure, Context, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) struct Engine<F> {
    pub fs: F,
    root: PathBuf,
}
enum Undo {
    Directory(PathBuf),
    Link(PathBuf),
}

/// Compare LIO's FILEIO `info` attribute with the desired backing file.
///
/// The kernel prints `File: <path>  Size: <bytes>` (two spaces) and appends
/// transport attributes, so match the fields rather than one exact layout.
fn file_backing_matches(info: &str, device: &str, size: u64) -> bool {
    let Some(rest) = info
        .split_once(&format!("File: {device}"))
        .map(|(_, rest)| rest)
    else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix("Size: ") else {
        return false;
    };
    rest.split_whitespace()
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        == Some(size)
}

impl<F: ConfigFs> Engine<F> {
    pub fn new(fs: F, root: PathBuf) -> Self {
        Self { fs, root }
    }
    fn target(&self, name: &str) -> PathBuf {
        self.root.join("iscsi").join(name)
    }
    fn tpg(&self, name: &str) -> PathBuf {
        self.target(name).join("tpgt_1")
    }

    pub fn execute(&self, op: Operation, save: &mut impl FnMut() -> Result<()>) -> Result<Reply> {
        match op {
            Operation::Probe => Ok(Reply::Unit),
            Operation::Save => {
                save()?;
                Ok(Reply::Unit)
            }
            Operation::Create(spec) => {
                validate_spec(&spec)?;
                Ok(Reply::Created(self.create(&spec, save)?))
            }
            Operation::EnsureAcl(spec) => {
                validate_spec(&spec)?;
                let mut undo = Vec::new();
                let operation = self
                    .ensure_acl(&spec, &mut undo)
                    .and_then(|_| self.ensure_fileio_acl_mappings_readonly(&spec.target_iqn));
                // A new directory may have promoted a live dynamic ACL; removing it would end its session.
                Self::combine(operation, save())?;
                Ok(Reply::Unit)
            }
            Operation::Inspect(spec) => {
                validate_spec(&spec)?;
                Ok(Reply::State(self.inspect(&spec)?))
            }
            Operation::VerifyFileMappings(spec) => {
                validate_spec(&spec)?;
                self.verify_named_file_lun_mappings(&spec)?;
                Ok(Reply::Unit)
            }
            Operation::Exists { target } => {
                identifier(&target)?;
                Ok(Reply::Exists(self.fs.exists(&self.target(&target))?))
            }
            Operation::List { target } => {
                identifier(&target)?;
                Ok(Reply::Luns(
                    self.luns(&target)?
                        .into_iter()
                        .map(|(lun, (_, path))| IscsiLunState {
                            lun,
                            backstore: path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                            exists: true,
                            backstore_exists: true,
                            block_device_matches: false,
                        })
                        .collect(),
                ))
            }
            Operation::RemoveOwned { target, backstores } => {
                identifier(&target)?;
                ensure!(backstores.len() <= 256, "too many backstores");
                for name in &backstores {
                    identifier(name)?;
                }
                self.disconnected(&target)?;
                let result = self.remove_owned(&target, &backstores);
                // Persist even a partial deletion so reboot cannot resurrect
                // references to storage the caller may subsequently destroy.
                let persisted = save();
                Self::combine(result, persisted)?;
                Ok(Reply::Unit)
            }
            Operation::RemoveTarget { target } => {
                identifier(&target)?;
                self.disconnected(&target)?;
                Self::combine(self.remove_target(&target), save())?;
                Ok(Reply::Unit)
            }
            Operation::SetChap { target, chap } => {
                identifier(&target)?;
                validate_chap(chap.as_ref())?;
                // CHAP changes affect new logins, just as in targetcli.
                Self::combine(self.chap(&target, chap.as_ref()), save())?;
                Ok(Reply::Unit)
            }
        }
    }

    fn combine(operation: Result<()>, persistence: Result<()>) -> Result<()> {
        match (operation, persistence) {
            (Err(error), Err(save)) => {
                Err(error).context(format!("also failed to persist LIO state: {save:#}"))
            }
            (Err(error), _) => Err(error),
            (_, result) => result,
        }
    }

    fn disconnected(&self, target: &str) -> Result<()> {
        let tpg = self.tpg(target);
        if !self.fs.exists(&self.target(target))? {
            return Ok(());
        }
        ensure!(
            self.fs
                .read(&tpg.join("dynamic_sessions"))?
                .trim()
                .is_empty(),
            "target has active sessions"
        );
        for acl in self.fs.entries(&tpg.join("acls"))? {
            ensure!(
                self.fs
                    .read(&acl.join("info"))?
                    .trim_start()
                    .starts_with("No active iSCSI Session"),
                "target ACL has active or unknown sessions"
            );
        }
        Ok(())
    }

    fn backstores(&self) -> Result<BTreeMap<String, PathBuf>> {
        let mut result = BTreeMap::new();
        for hba in self.fs.entries(&self.root.join("core"))? {
            if !["iblock_", "fileio_"].iter().any(|prefix| {
                hba.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with(prefix)
            }) {
                continue;
            }
            for path in self.fs.entries(&hba)? {
                // hba_info and hba_mode are ordinary attributes, not objects.
                if matches!(
                    path.file_name().and_then(|p| p.to_str()),
                    Some("hba_info" | "hba_mode")
                ) {
                    continue;
                }
                let name = path
                    .file_name()
                    .context("backstore name missing")?
                    .to_string_lossy()
                    .into_owned();
                ensure!(
                    result.insert(name, path).is_none(),
                    "duplicate kernel backstore name"
                );
            }
        }
        Ok(result)
    }

    fn links(&self, directory: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
        let mut links = Vec::new();
        for path in self.fs.entries(directory)? {
            if let Some(target) = self.fs.link_target(&path)? {
                links.push((path, target));
            }
        }
        Ok(links)
    }

    // LUN -> (link path, storage object path). Unknown or ambiguous mappings fail closed.
    fn luns(&self, target: &str) -> Result<BTreeMap<u32, (PathBuf, PathBuf)>> {
        let mut result = BTreeMap::new();
        if !self.fs.exists(&self.target(target))? {
            return Ok(result);
        }
        for path in self.fs.entries(&self.tpg(target).join("lun"))? {
            let name = path
                .file_name()
                .context("LUN name missing")?
                .to_string_lossy();
            let number: u32 = name
                .strip_prefix("lun_")
                .context("unexpected LUN directory")?
                .parse()?;
            let mut links = self.links(&path)?;
            ensure!(links.len() == 1, "incomplete or ambiguous LUN {number}");
            result.insert(number, links.remove(0));
        }
        Ok(result)
    }

    fn matches(&self, path: &Path, lun: &IscsiLunSpec) -> Result<bool> {
        if lun.backing == IscsiLunBacking::File {
            let device = lun.block_device.to_str().context("invalid file path")?;
            let size = std::fs::metadata(&lun.block_device)?.len();
            return Ok(file_backing_matches(
                &self.fs.read(&path.join("info"))?,
                device,
                size,
            ));
        }
        let device = self.fs.read(&path.join("udev_path"))?;
        let info = self.fs.read(&path.join("info"))?;
        let readonly = info
            .split("readonly:")
            .nth(1)
            .and_then(|s| s.split_whitespace().next());
        let readonly = match readonly {
            Some("0") => false,
            Some("1") => true,
            _ => bail!("cannot determine backstore readonly state"),
        };
        Ok(
            device.trim() == lun.block_device.to_str().context("invalid block device")?
                && readonly == lun.readonly,
        )
    }

    fn portal(spec: &IscsiTargetSpec) -> String {
        if spec.portal_address.contains(':') {
            format!("[{}]:{}", spec.portal_address, spec.portal_port)
        } else {
            format!("{}:{}", spec.portal_address, spec.portal_port)
        }
    }

    /// Mirror targetcli's portal equivalence: targetcli-fb auto-creates a
    /// `[::0]:port` wildcard portal, which already listens for the configured
    /// address. Creating `0.0.0.0:port` alongside it fails in the kernel.
    fn portal_satisfied(&self, spec: &IscsiTargetSpec) -> Result<bool> {
        let np = self.tpg(&spec.target_iqn).join("np");
        for name in [
            Self::portal(spec),
            format!("0.0.0.0:{}", spec.portal_port),
            format!("[::]:{}", spec.portal_port),
            format!("[::0]:{}", spec.portal_port),
        ] {
            if self.fs.exists(&np.join(name))? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn inspect(&self, spec: &IscsiTargetSpec) -> Result<IscsiTargetState> {
        let exists = self.fs.exists(&self.target(&spec.target_iqn))?;
        let luns = self.luns(&spec.target_iqn)?;
        let backstores = self.backstores()?;
        let states = spec
            .luns
            .iter()
            .map(|lun| -> Result<_> {
                let backstore = backstores.get(&lun.backstore);
                let mapped = luns.get(&lun.lun).map(|(_, p)| p);
                Ok(IscsiLunState {
                    lun: lun.lun,
                    backstore: lun.backstore.clone(),
                    exists: mapped.is_some(),
                    backstore_exists: backstore.is_some(),
                    block_device_matches: match backstore {
                        Some(path) if Some(path) == mapped => self.matches(path, lun)?,
                        _ => false,
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let portal = exists && self.portal_satisfied(spec)?;
        Ok(IscsiTargetState::from_luns(
            spec.target_iqn.clone(),
            exists,
            states,
            portal,
        ))
    }

    fn verify_named_file_lun_mappings(&self, spec: &IscsiTargetSpec) -> Result<()> {
        let files: Vec<_> = spec
            .luns
            .iter()
            .filter(|lun| lun.backing == IscsiLunBacking::File)
            .collect();
        if files.is_empty() {
            return Ok(());
        }
        let initiator = spec
            .initiator_iqn
            .as_deref()
            .context("file LUN mapping verification requires a named initiator")?;
        let acl = self.tpg(&spec.target_iqn).join("acls").join(initiator);
        ensure!(self.fs.exists(&acl)?, "named initiator ACL is missing");
        let luns = self.luns(&spec.target_iqn)?;
        let backstores = self.backstores()?;
        for lun in files {
            let (_, actual_storage) = luns
                .get(&lun.lun)
                .context("expected target LUN is missing")?;
            let expected_storage = backstores
                .get(&lun.backstore)
                .context("expected file backstore is missing")?;
            ensure!(
                actual_storage == expected_storage && self.matches(expected_storage, lun)?,
                "target LUN does not map the expected file backstore"
            );
            let target_lun = self
                .tpg(&spec.target_iqn)
                .join("lun")
                .join(format!("lun_{}", lun.lun));
            let mapping = acl.join(format!("lun_{}", lun.lun));
            ensure!(
                self.fs.exists(&mapping)?,
                "named initiator file LUN mapping is missing"
            );
            let links = self.links(&mapping)?;
            ensure!(
                links.len() == 1 && links[0].1 == target_lun,
                "named initiator mapping points at a different target LUN"
            );
            ensure!(
                self.fs.read(&mapping.join("write_protect"))?.trim() == "1",
                "named initiator file LUN mapping is writable"
            );
        }
        Ok(())
    }

    fn mkdir(&self, path: &Path, undo: &mut Vec<Undo>) -> Result<()> {
        self.fs
            .mkdir(path)
            .with_context(|| format!("create {}", path.display()))?;
        undo.push(Undo::Directory(path.to_owned()));
        Ok(())
    }
    fn link(&self, source: &Path, destination: &Path, undo: &mut Vec<Undo>) -> Result<()> {
        self.fs.link(source, destination)?;
        undo.push(Undo::Link(destination.to_owned()));
        Ok(())
    }
    fn chap(&self, target: &str, chap: Option<&ChapCredentials>) -> Result<()> {
        let tpg = self.tpg(target);
        if let Some(chap) = chap {
            self.fs.write(&tpg.join("attrib/authentication"), "1")?;
            self.write_chap(&tpg, chap)?;
            for acl in self.fs.entries(&tpg.join("acls"))? {
                self.write_chap(&acl, chap)?;
            }
        } else {
            self.fs.write(&tpg.join("attrib/authentication"), "0")?;
        }
        Ok(())
    }

    fn write_chap(&self, directory: &Path, chap: &ChapCredentials) -> Result<()> {
        self.fs
            .write(&directory.join("auth/userid"), &chap.username)?;
        self.fs
            .write(&directory.join("auth/password"), &chap.password)
    }

    fn ensure_acl(&self, spec: &IscsiTargetSpec, undo: &mut Vec<Undo>) -> Result<()> {
        let Some(initiator) = spec.initiator_iqn.as_ref() else {
            return Ok(());
        };
        let acl = self.tpg(&spec.target_iqn).join("acls").join(initiator);
        let existing = self.luns(&spec.target_iqn)?;
        for lun in &spec.luns {
            let (_, storage) = existing
                .get(&lun.lun)
                .context("desired LUN is not present")?;
            ensure!(
                storage.file_name().and_then(|name| name.to_str()) == Some(lun.backstore.as_str())
                    && self.matches(storage, lun)?,
                "desired LUN has a different backstore, device, or readonly state"
            );
        }
        if !self.fs.exists(&acl)? {
            self.mkdir(&acl, undo)?;
        }
        // Promoted ACLs use node_auth rather than the TPG's dynamic credentials.
        if let Some(chap) = spec.chap.as_ref() {
            self.write_chap(&acl, chap)?;
        }
        for lun in &spec.luns {
            let directory = self
                .tpg(&spec.target_iqn)
                .join("lun")
                .join(format!("lun_{}", lun.lun));
            let mapped = acl.join(format!("lun_{}", lun.lun));
            if self.fs.exists(&mapped)? {
                let links = self.links(&mapped)?;
                ensure!(
                    links.len() == 1 && links[0].1 == directory,
                    "existing ACL mapping conflicts with desired LUN"
                );
                ensure!(
                    self.fs.read(&mapped.join("write_protect"))?.trim()
                        == if lun.readonly { "1" } else { "0" },
                    "existing ACL write protection conflicts with desired LUN"
                );
            } else {
                self.mkdir(&mapped, undo)?;
                self.link(&directory, &mapped.join("diskless"), undo)?;
                self.fs.write(
                    &mapped.join("write_protect"),
                    if lun.readonly { "1" } else { "0" },
                )?;
            }
        }
        Ok(())
    }

    fn ensure_fileio_acl_mappings_readonly(&self, target: &str) -> Result<()> {
        let tpg = self.tpg(target);
        for (number, (_, storage)) in self.luns(target)? {
            if !storage
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name.to_string_lossy().starts_with("fileio_"))
            {
                continue;
            }
            for acl in self.fs.entries(&tpg.join("acls"))? {
                let mapped = acl.join(format!("lun_{number}"));
                if self.fs.exists(&mapped)? {
                    ensure!(
                        self.fs.read(&mapped.join("write_protect"))?.trim() == "1",
                        "existing ACL mapping for file LUN is writable"
                    );
                }
            }
        }
        Ok(())
    }

    fn create(
        &self,
        spec: &IscsiTargetSpec,
        save: &mut impl FnMut() -> Result<()>,
    ) -> Result<IscsiProvisionResult> {
        // Validate devices before modifying kernel state; udev may lag ZFS clone.
        for lun in &spec.luns {
            if lun.backing == IscsiLunBacking::File {
                ensure!(
                    std::fs::symlink_metadata(&lun.block_device)?
                        .file_type()
                        .is_file(),
                    "file LUN backing is not a regular file"
                );
            } else {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !self.fs.exists(&lun.block_device)? && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(50));
                }
                ensure!(
                    self.fs.is_block(&lun.block_device)?,
                    "device is not a block device"
                );
            }
        }
        self.disconnected(&spec.target_iqn)?;
        let mut undo = Vec::new();
        let mut created = IscsiProvisionResult::default();
        let operation = (|| -> Result<()> {
            let target = self.target(&spec.target_iqn);
            let tpg = self.tpg(&spec.target_iqn);
            if !self.fs.exists(&target)? {
                self.mkdir(&target, &mut undo)?;
                created.target_created = true;
            }
            if !self.fs.exists(&tpg)? {
                self.mkdir(&tpg, &mut undo)?;
            }
            // Enable authentication first; never expose a CHAP target unauthenticated.
            self.chap(&spec.target_iqn, spec.chap.as_ref())?;
            let existing = self.luns(&spec.target_iqn)?;
            let has_fileio_lun = spec
                .luns
                .iter()
                .any(|lun| lun.backing == IscsiLunBacking::File)
                || existing.values().any(|(_, storage)| {
                    storage
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|name| name.to_string_lossy().starts_with("fileio_"))
                });
            for (attribute, value) in [
                ("generate_node_acls", "1"),
                ("cache_dynamic_acls", "1"),
                (
                    "demo_mode_write_protect",
                    if has_fileio_lun { "1" } else { "0" },
                ),
            ] {
                self.fs.write(&tpg.join("attrib").join(attribute), value)?;
            }
            let mut backstores = self.backstores()?;
            for lun in &spec.luns {
                if let Some(path) = backstores.get(&lun.backstore) {
                    ensure!(
                        self.matches(path, lun)?,
                        "existing backstore has a different device or readonly state"
                    );
                    continue;
                }
                let transport = if lun.backing == IscsiLunBacking::File {
                    "fileio"
                } else {
                    "iblock"
                };
                let hba = (0..1_048_576)
                    .map(|i| self.root.join("core").join(format!("{transport}_{i}")))
                    .find_map(|p| match self.fs.exists(&p) {
                        Ok(false) => Some(Ok(p)),
                        Ok(true) => None,
                        Err(e) => Some(Err(e)),
                    })
                    .context("no available iblock index")??;
                self.mkdir(&hba, &mut undo)?;
                let path = hba.join(&lun.backstore);
                self.mkdir(&path, &mut undo)?;
                let device = lun.block_device.to_str().context("invalid backing path")?;
                if lun.backing == IscsiLunBacking::File {
                    self.fs
                        .write(&path.join("control"), &format!("fd_dev_name={device}"))?;
                    self.fs.write(
                        &path.join("control"),
                        &format!(
                            "fd_dev_size={}",
                            std::fs::metadata(&lun.block_device)?.len()
                        ),
                    )?;
                } else {
                    self.fs.write(&path.join("udev_path"), device)?;
                    self.fs
                        .write(&path.join("control"), &format!("udev_path={device}"))?;
                    self.fs.write(
                        &path.join("control"),
                        &format!("readonly={}", u8::from(lun.readonly)),
                    )?;
                }
                self.fs.write(&path.join("enable"), "1")?;
                self.fs.write(
                    &path.join("wwn/vpd_unit_serial"),
                    &uuid::Uuid::new_v4().to_string(),
                )?;
                backstores.insert(lun.backstore.clone(), path);
                created.backstores_created.push(lun.backstore.clone());
            }
            for lun in &spec.luns {
                let storage = backstores
                    .get(&lun.backstore)
                    .context("missing prepared backstore")?;
                let directory = tpg.join("lun").join(format!("lun_{}", lun.lun));
                if let Some((_, mapped)) = existing.get(&lun.lun) {
                    ensure!(
                        mapped == storage,
                        "existing LUN maps to a different backstore"
                    );
                } else {
                    self.mkdir(&directory, &mut undo)?;
                    self.link(storage, &directory.join("diskless"), &mut undo)?;
                    created.luns_created.push(lun.lun);
                }
                // Match targetcli's auto_add_mapped_luns for cached/explicit ACLs.
                for acl in self.fs.entries(&tpg.join("acls"))? {
                    let mapped = acl.join(format!("lun_{}", lun.lun));
                    if self.fs.exists(&mapped)? {
                        let links = self.links(&mapped)?;
                        ensure!(
                            links.len() == 1 && links[0].1 == directory,
                            "existing ACL mapping conflicts with desired LUN"
                        );
                        if lun.backing == IscsiLunBacking::File {
                            ensure!(
                                self.fs.read(&mapped.join("write_protect"))?.trim() == "1",
                                "existing ACL mapping for file LUN is writable"
                            );
                        }
                    } else {
                        self.mkdir(&mapped, &mut undo)?;
                        self.link(&directory, &mapped.join("diskless"), &mut undo)?;
                        self.fs.write(
                            &mapped.join("write_protect"),
                            if lun.readonly { "1" } else { "0" },
                        )?;
                    }
                }
            }
            self.ensure_fileio_acl_mappings_readonly(&spec.target_iqn)?;
            self.ensure_acl(spec, &mut undo)?;
            if !self.portal_satisfied(spec)? {
                self.mkdir(&tpg.join("np").join(Self::portal(spec)), &mut undo)?;
                created.portal_created = true;
            }
            self.fs.write(&tpg.join("enable"), "1")?;
            save()?;
            Ok(())
        })();
        if let Err(error) = operation {
            let mut failures = Vec::new();
            for item in undo.into_iter().rev() {
                let result = match item {
                    Undo::Directory(p) => self.fs.rmdir(&p),
                    Undo::Link(p) => self.fs.unlink(&p),
                };
                if let Err(e) = result {
                    failures.push(format!("{e:#}"));
                }
            }
            if let Err(e) = save() {
                failures.push(format!("persist rollback: {e:#}"));
            }
            if !failures.is_empty() {
                return Err(error).context(format!("rollback incomplete: {}", failures.join("; ")));
            }
            return Err(error);
        }
        Ok(created)
    }

    fn remove_lun(&self, target: &str, number: u32, link: &Path) -> Result<()> {
        let directory = self.tpg(target).join("lun").join(format!("lun_{number}"));
        for acl in self.fs.entries(&self.tpg(target).join("acls"))? {
            for mapping in self.fs.entries(&acl)? {
                if !mapping
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with("lun_")
                {
                    continue;
                }
                let links = self.links(&mapping)?;
                if links.iter().any(|(_, target)| target == &directory) {
                    ensure!(links.len() == 1, "ambiguous ACL mapping");
                    self.fs.unlink(&links[0].0)?;
                    self.fs.rmdir(&mapping)?;
                }
            }
        }
        self.fs.unlink(link)?;
        self.fs.rmdir(&directory)
    }

    fn remove_owned(&self, target: &str, owned: &[String]) -> Result<()> {
        let backstores = self.backstores()?;
        for (number, (link, storage)) in self.luns(target)? {
            if owned
                .iter()
                .any(|name| backstores.get(name) == Some(&storage))
            {
                self.remove_lun(target, number, &link)?;
            }
        }
        for name in owned {
            if let Some(path) = backstores.get(name) {
                self.fs
                    .rmdir(path)
                    .context("remove backstore (it may still be referenced by another target)")?;
                // The HBA is 1:1 for newly created objects, but preserve any
                // other objects created by an external administrator.
                let hba = path.parent().context("backstore parent missing")?;
                let remaining = self.fs.entries(hba)?;
                if remaining.iter().all(|p| {
                    matches!(
                        p.file_name().and_then(|s| s.to_str()),
                        Some("hba_info" | "hba_mode")
                    )
                }) {
                    self.fs.rmdir(hba)?;
                }
            }
        }
        Ok(())
    }

    fn remove_target(&self, target: &str) -> Result<()> {
        if !self.fs.exists(&self.target(target))? {
            return Ok(());
        }
        // Do not silently delete other portal groups owned outside the manager.
        ensure!(
            self.fs
                .entries(&self.target(target))?
                .iter()
                .all(|p| matches!(
                    p.file_name().and_then(|s| s.to_str()),
                    Some("tpgt_1" | "param" | "fabric_statistics")
                )),
            "target contains unmanaged portal groups"
        );
        let tpg = self.tpg(target);
        self.fs.write(&tpg.join("enable"), "0")?;
        for (number, (link, _)) in self.luns(target)? {
            self.remove_lun(target, number, &link)?;
        }
        for acl in self.fs.entries(&tpg.join("acls"))? {
            self.fs.rmdir(&acl)?;
        }
        for portal in self.fs.entries(&tpg.join("np"))? {
            self.fs.rmdir(&portal)?;
        }
        self.fs.rmdir(&tpg)?;
        self.fs.rmdir(&self.target(target))
    }
}

#[cfg(test)]
mod tests {
    use super::file_backing_matches;

    #[test]
    fn fileio_info_matches_kernel_layout() {
        let info = "Status: DEACTIVATED  Max Queue Depth: 0  SectorSize: 512  HwMaxSectors: 16384\n        TCM FILEIO ID: 0        File: /srv/tftp/installer-media/installer.iso  Size: 6615449600  Mode: O_DSYNC Async: 0\n";
        assert!(file_backing_matches(
            info,
            "/srv/tftp/installer-media/installer.iso",
            6_615_449_600
        ));
        assert!(!file_backing_matches(
            info,
            "/srv/tftp/installer-media/installer.iso",
            1
        ));
        assert!(!file_backing_matches(
            info,
            "/srv/tftp/installer-media/installer",
            6_615_449_600
        ));
        assert!(!file_backing_matches(
            info,
            "/srv/tftp/installer-media/other.iso",
            6_615_449_600
        ));
    }
}
