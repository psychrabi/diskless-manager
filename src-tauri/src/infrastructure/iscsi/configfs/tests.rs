use super::{
    engine::Engine,
    fs::{ConfigFs, KernelFs},
    *,
};
use crate::infrastructure::iscsi::IscsiLunSpec;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    path::{Path, PathBuf},
};

// Ordinary directories do not create kernel attributes. This adapter emulates
// only that behavior; path traversal, writes, symlinks and removal use real I/O.
struct FixtureFs {
    implicit: RefCell<BTreeSet<PathBuf>>,
    fail: RefCell<Option<String>>,
}
impl FixtureFs {
    fn new() -> Self {
        Self {
            implicit: RefCell::new(BTreeSet::new()),
            fail: RefCell::new(None),
        }
    }
    fn attribute(&self, path: PathBuf, value: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
                self.implicit.borrow_mut().insert(parent.to_owned());
            }
        }
        std::fs::write(&path, value)?;
        self.implicit.borrow_mut().insert(path);
        Ok(())
    }
    fn maybe_fail(&self, path: &Path) -> Result<()> {
        if self
            .fail
            .borrow()
            .as_ref()
            .is_some_and(|s| path.to_string_lossy().contains(s))
        {
            bail!("injected filesystem failure");
        }
        Ok(())
    }
}
impl ConfigFs for FixtureFs {
    fn exists(&self, path: &Path) -> Result<bool> {
        if path.starts_with("/dev/zvol") {
            Ok(true)
        } else {
            KernelFs.exists(path)
        }
    }
    fn entries(&self, path: &Path) -> Result<Vec<PathBuf>> {
        KernelFs.entries(path)
    }
    fn read(&self, path: &Path) -> Result<String> {
        KernelFs.read(path)
    }
    fn write(&self, path: &Path, value: &str) -> Result<()> {
        self.maybe_fail(path)?;
        if path.file_name().and_then(|s| s.to_str()) == Some("control")
            && value.starts_with("readonly=")
        {
            self.attribute(
                path.with_file_name("info"),
                &value.replace("readonly=", "readonly: "),
            )?;
        }
        KernelFs.write(path, value)
    }
    fn mkdir(&self, path: &Path) -> Result<()> {
        self.maybe_fail(path)?;
        KernelFs.mkdir(path)?;
        let name = path.file_name().unwrap().to_string_lossy();
        if name == "tpgt_1" {
            for dir in ["lun", "acls", "np"] {
                let directory = path.join(dir);
                std::fs::create_dir(&directory)?;
                self.implicit.borrow_mut().insert(directory);
            }
            for (attr, value) in [
                ("enable", "0"),
                ("dynamic_sessions", ""),
                ("attrib/authentication", "0"),
                ("attrib/generate_node_acls", "0"),
                ("attrib/cache_dynamic_acls", "0"),
                ("attrib/demo_mode_write_protect", "1"),
                ("auth/userid", ""),
                ("auth/password", ""),
            ] {
                self.attribute(path.join(attr), value)?;
            }
        } else if name.starts_with("iblock_") {
            self.attribute(path.join("hba_info"), "")?;
            self.attribute(path.join("hba_mode"), "0")?;
        } else if path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("iblock_")
        {
            for attr in [
                "udev_path",
                "control",
                "info",
                "enable",
                "wwn/vpd_unit_serial",
            ] {
                self.attribute(path.join(attr), "")?;
            }
        } else if name.starts_with("lun_") {
            self.attribute(path.join("write_protect"), "0")?;
        }
        Ok(())
    }
    fn rmdir(&self, path: &Path) -> Result<()> {
        self.maybe_fail(path)?;
        let mut children: Vec<_> = self
            .implicit
            .borrow()
            .iter()
            .filter(|p| p.starts_with(path) && *p != path)
            .cloned()
            .collect();
        children.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
        for child in children {
            if child.is_dir() {
                std::fs::remove_dir(&child)?;
            } else {
                std::fs::remove_file(&child)?;
            }
            self.implicit.borrow_mut().remove(&child);
        }
        KernelFs.rmdir(path)
    }
    fn link(&self, source: &Path, destination: &Path) -> Result<()> {
        self.maybe_fail(destination)?;
        KernelFs.link(source, destination)
    }
    fn link_target(&self, path: &Path) -> Result<Option<PathBuf>> {
        KernelFs.link_target(path)
    }
    fn unlink(&self, path: &Path) -> Result<()> {
        self.maybe_fail(path)?;
        KernelFs.unlink(path)
    }
    fn is_block(&self, path: &Path) -> Result<bool> {
        Ok(path.starts_with("/dev/zvol/"))
    }
}

fn fixture() -> (tempfile::TempDir, Engine<FixtureFs>, IscsiTargetSpec) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("iscsi")).unwrap();
    std::fs::create_dir(dir.path().join("core")).unwrap();
    let engine = Engine::new(FixtureFs::new(), dir.path().into());
    let spec = IscsiTargetSpec::with_luns(
        "iqn.2024-01.com.diskless:test",
        vec![
            IscsiLunSpec::new(0, "block_test", "/dev/zvol/diskless/test"),
            IscsiLunSpec::new(1, "game_test", "/dev/zvol/diskless/game"),
        ],
    )
    .unwrap();
    (dir, engine, spec)
}

#[test]
fn native_create_is_idempotent_and_preserves_existing_resources() {
    let (_dir, engine, spec) = fixture();
    let Reply::Created(created) = engine
        .execute(Operation::Create(spec.clone()), &mut || Ok(()))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(created.luns_created, [0, 1]);
    assert_eq!(created.backstores_created, ["block_test", "game_test"]);
    let Reply::State(state) = engine
        .execute(Operation::Inspect(spec.clone()), &mut || {
            panic!("inspection must not save")
        })
        .unwrap()
    else {
        panic!()
    };
    assert!(state.is_ready());
    let Reply::Created(second) = engine
        .execute(Operation::Create(spec), &mut || Ok(()))
        .unwrap()
    else {
        panic!()
    };
    assert!(second.is_empty());
}

#[test]
fn failed_second_backstore_rolls_back_only_new_objects() {
    let (dir, engine, mut spec) = fixture();
    let game = spec.luns.pop().unwrap();
    engine
        .execute(Operation::Create(spec.clone()), &mut || Ok(()))
        .unwrap();
    let original = dir
        .path()
        .join("core/iblock_0/block_test/wwn/vpd_unit_serial");
    let serial = std::fs::read_to_string(&original).unwrap();
    spec.luns.push(game);
    *engine.fs.fail.borrow_mut() = Some("game_test/enable".into());
    assert!(engine
        .execute(Operation::Create(spec), &mut || Ok(()))
        .is_err());
    assert_eq!(std::fs::read_to_string(original).unwrap(), serial);
    assert!(!dir.path().join("core/iblock_1").exists());
    assert!(dir
        .path()
        .join("iscsi/iqn.2024-01.com.diskless:test/tpgt_1/lun/lun_0/diskless")
        .exists());
}

#[test]
fn save_failure_rolls_back_created_resources_and_saves_rollback() {
    let (dir, engine, spec) = fixture();
    let mut saves = 0;
    let result = engine.execute(Operation::Create(spec), &mut || {
        saves += 1;
        if saves == 1 {
            bail!("save failure");
        }
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(saves, 2);
    assert_eq!(
        std::fs::read_dir(dir.path().join("core")).unwrap().count(),
        0
    );
    assert_eq!(
        std::fs::read_dir(dir.path().join("iscsi")).unwrap().count(),
        0
    );
}

#[test]
fn existing_wildcard_portal_satisfies_portal_requirement() {
    let (dir, engine, spec) = fixture();
    // Simulate targetcli's auto_add_default_portal on a pre-existing target.
    let tpg = dir
        .path()
        .join("iscsi")
        .join(&spec.target_iqn)
        .join("tpgt_1");
    std::fs::create_dir_all(tpg.join("np/[::0]:3260")).unwrap();
    std::fs::create_dir(tpg.join("lun")).unwrap();
    std::fs::write(tpg.join("dynamic_sessions"), "").unwrap();
    std::fs::create_dir(tpg.join("acls")).unwrap();
    for attr in [
        "attrib/authentication",
        "attrib/generate_node_acls",
        "attrib/cache_dynamic_acls",
        "attrib/demo_mode_write_protect",
        "auth/userid",
        "auth/password",
        "enable",
    ] {
        let path = tpg.join(attr);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }
    let Reply::Created(created) = engine
        .execute(Operation::Create(spec.clone()), &mut || Ok(()))
        .unwrap()
    else {
        panic!()
    };
    assert!(!created.portal_created);
    assert!(!tpg.join("np/0.0.0.0:3260").exists());
    let Reply::State(state) = engine
        .execute(Operation::Inspect(spec), &mut || {
            panic!("inspection must not save")
        })
        .unwrap()
    else {
        panic!()
    };
    assert!(state.portal_exists);
}

#[test]
fn save_delegates_persistence_to_caller() {
    let (_dir, engine, _spec) = fixture();
    let mut saves = 0;
    let reply = engine
        .execute(Operation::Save, &mut || {
            saves += 1;
            Ok(())
        })
        .unwrap();
    assert!(matches!(reply, Reply::Unit));
    assert_eq!(saves, 1);
}

#[test]
fn traversal_and_device_escape_are_rejected_before_mutation() {
    let (dir, engine, spec) = fixture();
    for bad in ["../outside", "a/b", ".", "..", "bad\nname"] {
        let mut invalid = spec.clone();
        invalid.target_iqn = bad.into();
        assert!(engine
            .execute(Operation::Create(invalid), &mut || panic!())
            .is_err());
    }
    let mut invalid = spec;
    invalid.luns[0].block_device = "/dev/zvol/../../sda".into();
    assert!(engine
        .execute(Operation::Create(invalid), &mut || panic!())
        .is_err());
    assert_eq!(
        std::fs::read_dir(dir.path().join("iscsi")).unwrap().count(),
        0
    );
}

#[test]
fn mismatched_backstore_and_active_sessions_fail_closed() {
    let (dir, engine, spec) = fixture();
    engine
        .execute(Operation::Create(spec.clone()), &mut || Ok(()))
        .unwrap();
    let mut invalid = spec.clone();
    invalid.luns[0].readonly = true;
    assert!(engine
        .execute(Operation::Create(invalid), &mut || Ok(()))
        .is_err());
    std::fs::write(
        dir.path()
            .join("iscsi")
            .join(&spec.target_iqn)
            .join("tpgt_1/dynamic_sessions"),
        "iqn.active",
    )
    .unwrap();
    assert!(engine
        .execute(
            Operation::RemoveOwned {
                target: spec.target_iqn,
                backstores: vec!["block_test".into()]
            },
            &mut || panic!()
        )
        .is_err());
    assert!(dir.path().join("core/iblock_0/block_test").exists());
}

#[test]
fn owned_removal_preserves_game_disk_and_target() {
    let (dir, engine, spec) = fixture();
    engine
        .execute(Operation::Create(spec.clone()), &mut || Ok(()))
        .unwrap();
    engine
        .execute(
            Operation::RemoveOwned {
                target: spec.target_iqn.clone(),
                backstores: vec!["block_test".into()],
            },
            &mut || Ok(()),
        )
        .unwrap();
    assert!(!dir.path().join("core/iblock_0").exists());
    assert!(dir.path().join("core/iblock_1/game_test").exists());
    let Reply::Luns(luns) = engine
        .execute(
            Operation::List {
                target: spec.target_iqn,
            },
            &mut || panic!(),
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(luns.len(), 1);
    assert_eq!(luns[0].backstore, "game_test");
}

#[test]
#[ignore = "requires root, initialized LIO, and DISKLESS_TEST_ZVOL pointing to a disposable, unexported ZFS volume"]
fn real_lio_create_inspect_and_remove() {
    let device = std::env::var("DISKLESS_TEST_ZVOL").expect("set disposable test ZVOL");
    let name = format!("diskless_test_{}", uuid::Uuid::new_v4().simple());
    let mut spec =
        IscsiTargetSpec::new(format!("iqn.2024-01.com.diskless:{name}"), &name, device, 0);
    spec.portal_address = "127.0.0.1".into();
    spec.portal_port = 43260;
    validate_spec(&spec).unwrap();
    let _lock = super::lock("/run/targetcli.lock").unwrap();
    let engine = Engine::new(KernelFs, ROOT.into());
    // Deliberately do not persist ephemeral integration-test resources.
    engine
        .execute(Operation::Create(spec.clone()), &mut || Ok(()))
        .unwrap();
    let inspected = engine.execute(Operation::Inspect(spec.clone()), &mut || Ok(()));
    let cleanup = engine.execute(
        Operation::RemoveOwned {
            target: spec.target_iqn.clone(),
            backstores: vec![name],
        },
        &mut || Ok(()),
    );
    let removed = engine.execute(
        Operation::RemoveTarget {
            target: spec.target_iqn,
        },
        &mut || Ok(()),
    );
    cleanup.unwrap();
    removed.unwrap();
    let Reply::State(state) = inspected.unwrap() else {
        panic!()
    };
    assert!(state.is_ready());
}
