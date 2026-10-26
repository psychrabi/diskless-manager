//! Candidate-only native Windows hive recovery. Never call on a running installation.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsVolumeSafetyResult {
    pub system_recovered: bool,
    pub disk_released: bool,
}

fn hive_dirty(path: &Path) -> Result<bool> {
    let mut header = [0u8; 512];
    std::fs::File::open(path)?.read_exact(&mut header)?;
    if &header[..4] != b"regf" {
        bail!("unsupported SYSTEM hive signature");
    }
    let word =
        |offset| u32::from_le_bytes(header[offset..offset + 4].try_into().expect("four bytes"));
    if word(0x14) != 1 || !(3..=6).contains(&word(0x18)) || word(0x1c) != 0 || word(0x20) != 1 {
        bail!("unsupported SYSTEM hive format; native recovery refused");
    }
    let mut checksum = (0..0x1fc)
        .step_by(4)
        .fold(0, |sum, offset| sum ^ word(offset));
    if checksum == u32::MAX {
        checksum = u32::MAX - 1;
    }
    if checksum == 0 {
        checksum = 1;
    }
    if checksum != word(0x1fc) {
        bail!("SYSTEM hive header checksum is invalid");
    }
    Ok(word(4) != word(8))
}

fn ensure_not_hibernated(root: &Path) -> Result<()> {
    let path = root.join("hiberfil.sys");
    if path.exists() {
        anyhow::ensure!(
            path.canonicalize()?.starts_with(root.canonicalize()?),
            "hibernation file escapes candidate volume"
        );
    }
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("cannot inspect Windows hibernation state"),
    };
    let mut signature = [0u8; 4];
    file.read_exact(&mut signature)
        .context("truncated hibernation header")?;
    // Windows clears the signature after a complete shutdown. Unknown signatures
    // are refused as well: guessing would allow a suspended filesystem to publish.
    if signature != [0; 4] {
        bail!("Windows is hibernated or has an unsupported hibernation header; fully shut down Windows before saving");
    }
    Ok(())
}

pub fn check_offline_volume(root: &Path) -> Result<WindowsVolumeSafetyResult> {
    if !cfg!(windows) {
        bail!("native SYSTEM recovery requires the Windows worker");
    }
    let root = root
        .canonicalize()
        .context("candidate Windows volume is unavailable")?;
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        anyhow::ensure!(
            root.join("Windows").canonicalize()? != Path::new(&system_root).canonicalize()?,
            "refusing the worker's running Windows installation"
        );
    }
    ensure_not_hibernated(&root)?;
    let config = root.join("Windows/System32/config").canonicalize()?;
    anyhow::ensure!(
        config.starts_with(&root),
        "SYSTEM configuration escapes candidate volume"
    );
    let hive = config.join("SYSTEM");
    anyhow::ensure!(
        hive.canonicalize()?.starts_with(&config),
        "SYSTEM hive escapes candidate configuration directory"
    );
    let dirty = hive_dirty(&hive)?;
    if dirty {
        for name in ["SYSTEM.LOG1", "SYSTEM.LOG2"] {
            anyhow::ensure!(
                config
                    .join(name)
                    .canonicalize()
                    .with_context(|| format!("dirty SYSTEM requires {name}"))?
                    .starts_with(&config),
                "SYSTEM log escapes candidate configuration directory"
            );
            let metadata = std::fs::metadata(config.join(name)).with_context(|| {
                format!("dirty SYSTEM requires {name}; restore logs or cleanly shut down Windows")
            })?;
            if !metadata.is_file() || metadata.len() < 512 {
                bail!("missing or truncated {name}; native recovery refused");
            }
        }
        // ACL is applied before any hive bytes enter this private recovery directory.
        let recovery_root = std::path::PathBuf::from(
            std::env::var_os("ProgramData")
                .context("Windows ProgramData directory is unavailable")?,
        )
        .join("Diskless Manager/recovery");
        std::fs::create_dir_all(&recovery_root)?;
        let backup = tempfile::Builder::new()
            .prefix("diskless-hive-")
            .tempdir_in(recovery_root)?
            .keep();
        let protected = std::process::Command::new("icacls.exe")
            .arg(&backup)
            .args([
                "/inheritance:r",
                "/grant:r",
                "*S-1-5-18:(OI)(CI)F",
                "*S-1-5-32-544:(OI)(CI)F",
            ])
            .output()?;
        if !protected.status.success() {
            bail!("cannot protect SYSTEM recovery backup directory");
        }
        for name in ["SYSTEM", "SYSTEM.LOG1", "SYSTEM.LOG2"] {
            std::fs::copy(config.join(name), backup.join(name))?;
            std::fs::OpenOptions::new()
                .write(true)
                .open(backup.join(name))?
                .sync_all()?;
        }
        let key = format!(
            r"HKLM\DisklessRecovery_{}",
            backup
                .file_name()
                .context("backup directory name missing")?
                .to_string_lossy()
        );
        let loaded = std::process::Command::new("reg.exe")
            .arg("load")
            .arg(&key)
            .arg(&hive)
            .output()
            .with_context(|| {
                format!(
                    "native SYSTEM load failed; backup {} retained",
                    backup.display()
                )
            })?;
        if !loaded.status.success() {
            // A failed load can leave a key behind; report cleanup uncertainty.
            let cleanup = match std::process::Command::new("reg.exe")
                .args(["unload", &key])
                .output()
            {
                Ok(output) if output.status.success() => "native key unloaded".to_string(),
                Ok(_) => "native key unload was not confirmed; inspect the key before retrying"
                    .to_string(),
                Err(error) => format!("native key unload failed: {error}"),
            };
            bail!(
                "native SYSTEM log replay failed; {cleanup}; key {key}, private backup {} retained",
                backup.display()
            );
        }
        let unloaded = std::process::Command::new("reg.exe")
            .args(["unload", &key])
            .output()
            .with_context(|| {
                format!(
                    "SYSTEM unload failed; key {key}, backup {}",
                    backup.display()
                )
            })?;
        if !unloaded.status.success() {
            bail!(
                "SYSTEM unload failed; key {key}, private backup {} retained",
                backup.display()
            );
        }
        if hive_dirty(&hive).with_context(|| {
            format!(
                "replayed SYSTEM verification failed; backup {} retained",
                backup.display()
            )
        })? {
            bail!(
                "SYSTEM remains dirty after native replay; backup {} retained",
                backup.display()
            );
        }
        std::fs::remove_dir_all(&backup)
            .context("SYSTEM recovered but private backup cleanup failed")?;
    }
    Ok(WindowsVolumeSafetyResult {
        system_recovered: dirty,
        disk_released: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hive(path: &Path, dirty: bool) {
        let mut bytes = [0u8; 512];
        bytes[..4].copy_from_slice(b"regf");
        for (offset, value) in [
            (4, 2u32),
            (8, if dirty { 1 } else { 2 }),
            (0x14, 1),
            (0x18, 5),
            (0x20, 1),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let checksum = (0..0x1fc).step_by(4).fold(0u32, |sum, i| {
            sum ^ u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap())
        });
        bytes[0x1fc..].copy_from_slice(&checksum.to_le_bytes());
        std::fs::write(path, bytes).unwrap();
    }
    #[test]
    fn validates_hive_format_checksum_and_sequences() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("SYSTEM");
        hive(&path, false);
        assert!(!hive_dirty(&path).unwrap());
        hive(&path, true);
        assert!(hive_dirty(&path).unwrap());
        std::fs::write(&path, b"regf").unwrap();
        assert!(hive_dirty(&path).is_err());
        hive(&path, false);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[80] = 42;
        std::fs::write(&path, bytes).unwrap();
        assert!(hive_dirty(&path).is_err());
    }
    #[test]
    fn refuses_suspended_and_unknown_hibernation_headers() {
        let dir = tempfile::tempdir().unwrap();
        assert!(ensure_not_hibernated(dir.path()).is_ok());
        for header in [b"hibr", b"wake", b"abcd", b"HIBR"] {
            std::fs::write(dir.path().join("hiberfil.sys"), header).unwrap();
            assert!(ensure_not_hibernated(dir.path()).is_err());
        }
        std::fs::write(dir.path().join("hiberfil.sys"), [0; 4]).unwrap();
        assert!(ensure_not_hibernated(dir.path()).is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_hibernation_symlink_outside_candidate() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), [0; 4]).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("hiberfil.sys")).unwrap();
        assert!(ensure_not_hibernated(root.path()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn clean_offline_fixture_needs_no_native_replay() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("Windows/System32/config");
        std::fs::create_dir_all(&config).unwrap();
        hive(&config.join("SYSTEM"), false);
        assert!(!check_offline_volume(dir.path()).unwrap().system_recovered);
    }
}
