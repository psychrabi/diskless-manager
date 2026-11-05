//! Blocking, clean-room adapter for the external hivexregedit tool.
//!
//! Callers must serialize offline-hive mutations and run these methods on a blocking
//! worker. No transaction logs are removed and dirty hives are never accepted.
//!
//! Only REG_SZ, REG_EXPAND_SZ, REG_MULTI_SZ and DWORD values are exposed.
//! Other well-formed hex types are omitted, never exported back over the hive.
//! Paths are relative to SYSTEM, case-folded to lowercase, with no leading slash.
//! This adapter checks REGF magic, minimum size and sequence equality, not the
//! complete REGF cell graph. The trusted external tool validates hive internals.
//! Callers own offline/ownership checks and must exclude concurrent writers.
//! Atomic replacement preserves permission bits, not ownership, ACLs or xattrs.
use super::windows_boot_arming::{RegistryArmChange, RegistryArmValue};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;

const MAX_HIVE: u64 = 128 * 1024 * 1024;
const MAX_EXPORT: u64 = 128 * 1024 * 1024;
const PREFIX: &str = "HKEY_LOCAL_MACHINE\\SYSTEM";

pub struct LinuxRegistryHive {
    tool: PathBuf,
}

#[derive(Debug, Default)]
pub struct RegistryHiveSnapshot {
    pub values: BTreeMap<String, BTreeMap<String, RegistryArmValue>>,
}

impl RegistryHiveSnapshot {
    pub fn string(&self, key: &str, name: &str) -> Option<String> {
        match self
            .values
            .get(&key.to_lowercase())?
            .get(&name.to_lowercase())?
        {
            RegistryArmValue::String(value) | RegistryArmValue::ExpandString(value) => {
                Some(value.clone())
            }
            _ => None,
        }
    }

    pub fn dword(&self, key: &str, name: &str) -> Option<u32> {
        match self
            .values
            .get(&key.to_lowercase())?
            .get(&name.to_lowercase())?
        {
            RegistryArmValue::Dword(value) => Some(*value),
            _ => None,
        }
    }

    pub fn key_exists(&self, key: &str) -> bool {
        self.values.contains_key(&key.to_lowercase())
    }
}

impl LinuxRegistryHive {
    pub fn new() -> Result<Self> {
        let tool = std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
            .map(|directory| directory.join("hivexregedit"))
            .find(|path| {
                fs::metadata(path)
                    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            })
            .context("Linux SYSTEM hive servicing requires hivexregedit installed in PATH")?;
        Ok(Self { tool })
    }

    pub fn with_tool(path: impl Into<PathBuf>) -> Self {
        Self { tool: path.into() }
    }

    pub fn read(&self, hive: &Path) -> Result<RegistryHiveSnapshot> {
        checked_hive(hive)?;
        let hive = if hive.is_absolute() {
            hive.to_owned()
        } else {
            std::env::current_dir()?.join(hive)
        };
        let mut child = Command::new(&self.tool)
            .arg("--export")
            .arg(&hive)
            .arg("\\")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start registry export")?;
        let mut bytes = Vec::new();
        let read_result = child
            .stdout
            .take()
            .context("registry export pipe unavailable")?
            .take(MAX_EXPORT + 1)
            .read_to_end(&mut bytes);
        if read_result.is_err() || bytes.len() as u64 > MAX_EXPORT {
            let kill = child.kill();
            let wait = child.wait();
            kill.context("failed to stop oversized registry export")?;
            wait.context("failed to reap registry export")?;
            read_result.context("failed to read registry export")?;
            bail!("registry export exceeds size limit");
        }
        if !child
            .wait()
            .context("failed to wait for registry export")?
            .success()
        {
            bail!("registry export failed (tool output withheld)");
        }
        parse_export(std::str::from_utf8(&bytes).context("registry export is not UTF-8")?)
    }

    /// Merge only requested values into a private copy, verify, then atomically replace.
    /// A directory-fsync failure after replacement is reported, but cannot undo rename.
    pub fn apply(&self, hive: &Path, changes: &[RegistryArmChange]) -> Result<()> {
        let absolute = if hive.is_absolute() {
            hive.to_owned()
        } else {
            std::env::current_dir()?.join(hive)
        };
        let hive = absolute.as_path();
        let mut original = checked_hive(hive)?;
        let metadata = original.metadata()?;
        let parent = hive
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let reg = encode_changes(changes)?;
        if changes.is_empty() {
            return Ok(());
        }
        let mut stage =
            NamedTempFile::new_in(parent).context("create private hive staging file")?;
        let mut regfile = match NamedTempFile::new_in(parent) {
            Ok(file) => file,
            Err(error) => {
                stage.close().context("remove private hive staging file")?;
                return Err(error).context("create private registry merge file");
            }
        };
        let result = (|| -> Result<()> {
            original.seek(SeekFrom::Start(0))?;
            let copied = std::io::copy(&mut original.take(MAX_HIVE + 1), stage.as_file_mut())?;
            if copied > MAX_HIVE {
                bail!("SYSTEM hive exceeds size limit");
            }
            stage.as_file().sync_all()?;
            checked_hive(stage.path())?;
            regfile.write_all(reg.as_bytes())?;
            regfile.as_file().sync_all()?;
            let status = Command::new(&self.tool)
                .arg("--merge")
                .arg("--prefix")
                .arg(PREFIX)
                .arg(stage.path())
                .arg(regfile.path())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .context("failed to start registry merge")?;
            if !status.success() {
                bail!("registry merge failed (tool output withheld)");
            }
            let merged = checked_hive(stage.path())?;
            merged.sync_all()?;
            let snapshot = self.read(stage.path())?;
            for change in changes {
                let actual = snapshot
                    .values
                    .get(&change.key.to_lowercase())
                    .and_then(|values| values.get(&change.name.to_lowercase()));
                if !actual.is_some_and(|value| same_value(value, &change.value)) {
                    bail!("registry merge verification failed");
                }
            }
            let verified = checked_hive(stage.path())?;
            verified.set_permissions(metadata.permissions())?;
            verified.sync_all()?;
            let current = checked_hive(hive)?.metadata()?;
            if current.dev() != metadata.dev()
                || current.ino() != metadata.ino()
                || current.len() != metadata.len()
                || current.mtime() != metadata.mtime()
                || current.mtime_nsec() != metadata.mtime_nsec()
                || current.ctime() != metadata.ctime()
                || current.ctime_nsec() != metadata.ctime_nsec()
            {
                bail!("SYSTEM hive changed during staging");
            }
            Ok(())
        })();
        let cleanup = regfile
            .close()
            .context("remove private registry merge file");
        if let Err(error) = result {
            let stage_cleanup = stage.close().context("remove private hive staging file");
            cleanup?;
            stage_cleanup?;
            return Err(error);
        }
        if let Err(error) = cleanup {
            stage.close().context("remove private hive staging file")?;
            return Err(error);
        }
        match stage.persist(hive) {
            Ok(file) => file.sync_all()?,
            Err(error) => {
                let tempfile::PersistError { error, file } = error;
                file.close()
                    .context("remove unpersisted hive staging file")?;
                return Err(error).context("replace SYSTEM hive");
            }
        }
        File::open(parent)?
            .sync_all()
            .context("sync SYSTEM hive directory")
    }
}

fn checked_hive(path: &Path) -> Result<File> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        bail!("SYSTEM hive must be a regular non-symlink file");
    }
    // Linux O_NOFOLLOW closes the final-component symlink race without new dependencies.
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(0x20000)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() < 4096 || metadata.len() > MAX_HIVE {
        bail!("invalid SYSTEM hive size or file type");
    }
    let mut header = [0u8; 512];
    file.read_exact(&mut header)?;
    if &header[..4] != b"regf" {
        bail!("SYSTEM hive has no REGF header");
    }
    if header[4..8] != header[8..12] {
        bail!("dirty SYSTEM hive rejected; recover it in Windows first");
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}

fn valid_key(key: &str) -> Result<()> {
    if key
        .chars()
        .any(|c| c.is_control() || matches!(c, '[' | ']' | '"'))
        || (!key.is_empty() && key.split('\\').any(str::is_empty))
    {
        bail!("unsafe registry key");
    }
    Ok(())
}

fn valid_name(name: &str) -> Result<()> {
    if name.chars().any(char::is_control) {
        bail!("unsafe registry value name");
    }
    Ok(())
}

fn same_value(left: &RegistryArmValue, right: &RegistryArmValue) -> bool {
    match (left, right) {
        (RegistryArmValue::Dword(a), RegistryArmValue::Dword(b)) => a == b,
        (RegistryArmValue::String(a), RegistryArmValue::String(b))
        | (RegistryArmValue::ExpandString(a), RegistryArmValue::ExpandString(b)) => a == b,
        (RegistryArmValue::MultiString(a), RegistryArmValue::MultiString(b)) => a == b,
        _ => false,
    }
}

fn quoted(input: &str) -> Result<(String, &str)> {
    let mut chars = input.char_indices();
    if chars.next() != Some((0, '"')) {
        bail!("expected quoted registry string");
    }
    let mut result = String::new();
    while let Some((offset, c)) = chars.next() {
        match c {
            '"' => return Ok((result, &input[offset + 1..])),
            '\\' => match chars.next() {
                Some((_, escaped @ ('\\' | '"'))) => result.push(escaped),
                _ => bail!("invalid registry escape"),
            },
            c if c.is_control() => bail!("control character in quoted registry string"),
            _ => result.push(c),
        }
    }
    bail!("unterminated registry string")
}

fn parse_value(input: &str) -> Result<Option<RegistryArmValue>> {
    if input.starts_with('"') {
        let (value, rest) = quoted(input)?;
        if !rest.trim().is_empty() {
            bail!("trailing registry string syntax");
        }
        return Ok(Some(RegistryArmValue::String(value)));
    }
    let lower = input.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("dword:") {
        if hex.is_empty() || hex.len() > 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("invalid registry DWORD");
        }
        return Ok(Some(RegistryArmValue::Dword(u32::from_str_radix(hex, 16)?)));
    }
    let (tag, raw) = lower
        .split_once(':')
        .context("malformed registry value syntax")?;
    let kind = if tag == "hex" {
        3
    } else {
        let number = tag
            .strip_prefix("hex(")
            .and_then(|s| s.strip_suffix(')'))
            .context("malformed registry hex type")?;
        u32::from_str_radix(number, 16).context("invalid registry hex type")?
    };
    let mut bytes = Vec::new();
    if !raw.is_empty() {
        for byte in raw.split(',') {
            let byte = byte.trim();
            if byte.len() != 2 || !byte.bytes().all(|b| b.is_ascii_hexdigit()) {
                bail!("invalid registry hex byte");
            }
            bytes.push(u8::from_str_radix(byte, 16)?);
        }
    }
    if !matches!(kind, 1 | 2 | 7) {
        return Ok(None);
    }
    if bytes.len() % 2 != 0 {
        bail!("odd UTF-16 registry byte count");
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let mut text = String::from_utf16(&units).context("invalid registry UTF-16")?;
    if kind == 7 {
        if !text.ends_with("\0\0") {
            bail!("unterminated registry multi-string");
        }
        text.truncate(text.len() - 2);
        if !text.is_empty() && text.split('\0').any(str::is_empty) {
            bail!("empty registry multi-string element");
        }
        let values = if text.is_empty() {
            Vec::new()
        } else {
            text.split('\0').map(str::to_owned).collect()
        };
        Ok(Some(RegistryArmValue::MultiString(values)))
    } else {
        if !text.ends_with('\0') {
            bail!("unterminated registry string");
        }
        text.pop();
        if text.contains('\0') {
            bail!("embedded NUL in registry string");
        }
        Ok(Some(if kind == 1 {
            RegistryArmValue::String(text)
        } else {
            RegistryArmValue::ExpandString(text)
        }))
    }
}

fn parse_export(input: &str) -> Result<RegistryHiveSnapshot> {
    if input.len() as u64 > MAX_EXPORT {
        bail!("registry export exceeds size limit");
    }
    let mut snapshot = RegistryHiveSnapshot::default();
    let mut key = None;
    let mut seen = BTreeMap::<(String, String), String>::new();
    let mut pending = String::new();
    for physical in input.trim_start_matches('\u{feff}').lines() {
        let line = physical.trim();
        if line.ends_with('\\') {
            pending.push_str(line.strip_suffix('\\').context("invalid continuation")?);
            continue;
        }
        pending.push_str(line);
        let logical = std::mem::take(&mut pending);
        let line = logical.trim();
        if line.is_empty()
            || line.starts_with(';')
            || line == "Windows Registry Editor Version 5.00"
            || line == "REGEDIT4"
        {
            continue;
        }
        if line.starts_with('[') {
            let section = line
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .context("malformed registry section")?;
            let relative = section
                .strip_prefix('\\')
                .context("registry export section is not hive-relative")?;
            valid_key(relative)?;
            let canonical = relative.to_lowercase();
            snapshot.values.entry(canonical.clone()).or_default();
            key = Some(canonical);
            continue;
        }
        let key = key.as_ref().context("registry value without section")?;
        let (name, rest) = if let Some(rest) = line.strip_prefix('@') {
            (String::new(), rest)
        } else {
            quoted(line)?
        };
        valid_name(&name)?;
        let raw = rest
            .trim_start()
            .strip_prefix('=')
            .context("missing registry value assignment")?
            .trim();
        let name = name.to_lowercase();
        if let Some(previous) = seen.insert((key.clone(), name.clone()), raw.to_owned()) {
            if previous != raw {
                bail!("conflicting duplicate registry value");
            }
        }
        if let Some(value) = parse_value(raw)? {
            snapshot
                .values
                .get_mut(key)
                .context("missing registry section")?
                .insert(name, value);
        }
    }
    if !pending.is_empty() {
        bail!("unfinished registry continuation");
    }
    if snapshot.values.is_empty() {
        bail!("registry export contains no sections");
    }
    Ok(snapshot)
}

fn encode_changes(changes: &[RegistryArmChange]) -> Result<String> {
    let mut output = String::from("Windows Registry Editor Version 5.00\n\n");
    let mut seen = BTreeSet::new();
    for change in changes {
        valid_key(&change.key)?;
        valid_name(&change.name)?;
        if !seen.insert((change.key.to_lowercase(), change.name.to_lowercase())) {
            bail!("duplicate requested registry change");
        }
        output.push_str(&format!(
            "[{PREFIX}{}{}]\n",
            if change.key.is_empty() { "" } else { "\\" },
            change.key
        ));
        let name = change.name.replace('\\', "\\\\").replace('"', "\\\"");
        output.push_str(&format!("\"{name}\"="));
        match &change.value {
            RegistryArmValue::Dword(value) => output.push_str(&format!("dword:{value:08x}")),
            value => {
                let (kind, text) = match value {
                    RegistryArmValue::String(text) => (1, format!("{text}\0")),
                    RegistryArmValue::ExpandString(text) => (2, format!("{text}\0")),
                    RegistryArmValue::MultiString(values) => {
                        if values.iter().any(|s| s.is_empty() || s.contains('\0')) {
                            bail!("invalid registry multi-string element");
                        }
                        (7, format!("{}\0\0", values.join("\0")))
                    }
                    RegistryArmValue::Dword(_) => unreachable!("DWORD encoded separately"),
                };
                if matches!(value, RegistryArmValue::String(s) | RegistryArmValue::ExpandString(s) if s.contains('\0'))
                {
                    bail!("embedded NUL in registry string");
                }
                output.push_str(&format!("hex({kind}):"));
                let bytes: Vec<String> = text
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                output.push_str(&bytes.join(","));
            }
        }
        output.push_str("\n\n");
        if output.len() as u64 > MAX_EXPORT {
            bail!("registry changes exceed size limit");
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn hive(path: &Path) -> Vec<u8> {
        let mut bytes = vec![0; 4096];
        bytes[..4].copy_from_slice(b"regf");
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        fs::write(path, &bytes).unwrap();
        bytes
    }

    fn change(value: RegistryArmValue) -> RegistryArmChange {
        RegistryArmChange {
            key: "Services\\Example".into(),
            name: "Va\\\"lue".into(),
            value,
            reason: "synthetic fixture".into(),
        }
    }

    fn tool(directory: &Path, body: &str) -> PathBuf {
        let path = directory.join("fake-hivexregedit");
        fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[test]
    fn unicode_and_all_supported_types_round_trip() {
        for value in [
            RegistryArmValue::String("नमस्ते 😀 \\\"\nline".into()),
            RegistryArmValue::String(String::new()),
            RegistryArmValue::ExpandString("%SystemRoot%\\日本語".into()),
            RegistryArmValue::Dword(u32::MAX),
            RegistryArmValue::MultiString(vec!["αβ".into(), "😀\\driver".into()]),
            RegistryArmValue::MultiString(Vec::new()),
        ] {
            let encoded = encode_changes(&[change(value.clone())]).unwrap();
            let relative = encoded.replace(PREFIX, "");
            let parsed = parse_export(&relative).unwrap();
            assert!(same_value(
                &parsed.values["services\\example"]["va\\\"lue"],
                &value
            ));
        }
    }

    #[test]
    fn quoted_escapes_default_continuations_and_canonical_lookup() {
        let snapshot = parse_export(concat!(
            "Windows Registry Editor Version 5.00\r\n",
            "[\\Services\\Example]\r\n",
            "@=\"default\"\r\n",
            "\"Path\"=\"C:\\\\Windows\\\"quoted\\\"\"\r\n",
            "\"Start\"=dword:0000000A\r\n",
            "\"Name\"=hex(1):61,00,\\\r\n  00,00\r\n",
            "\"Binary\"=hex:ff,00\r\n",
            "[\\Empty]\r\n"
        ))
        .unwrap();
        assert_eq!(
            snapshot.string("SERVICES\\EXAMPLE", ""),
            Some("default".into())
        );
        assert_eq!(
            snapshot.string("services\\example", "PATH"),
            Some("C:\\Windows\"quoted\"".into())
        );
        assert_eq!(snapshot.dword("Services\\Example", "Start"), Some(10));
        assert_eq!(
            snapshot.string("services\\example", "name"),
            Some("a".into())
        );
        assert!(snapshot.key_exists("EMPTY"));
        assert!(!snapshot.values["services\\example"].contains_key("binary"));
    }

    #[test]
    fn rejects_malformed_and_conflicting_supported_syntax() {
        for input in [
            "[\\A]\n\"x\"=dword:100000000",
            "[\\A]\n\"x\"=dword:no",
            "[\\A]\n\"x\"=hex(1):00",
            "[\\A]\n\"x\"=hex(1):61,00",
            "[\\A]\n\"x\"=hex(2):00,d8,00,00",
            "[\\A]\n\"x\"=hex(7):61,00,00,00",
            "[\\A]\n\"x\"=hex(1):zz,00",
            "[\\A]\n\"x\"=hex(1)00,00",
            "[\\A]\n\"x\"=\"unterminated",
            "[\\A]\n\"x\"=\"bad\\q\"",
            "[\\A]\n\"x\"=hex(1):00,\\",
            "[\\A]\n\"x\"=dword:00000001\n[\\a]\n\"X\"=dword:00000002",
            "[\\A]\n\"x\"=hex:01\n[\\a]\n\"X\"=hex:02",
            "[-HKEY_LOCAL_MACHINE\\SYSTEM]",
            "[\\A]injected\n",
            "\"x\"=dword:00000001",
        ] {
            assert!(
                parse_export(input).is_err(),
                "accepted malformed synthetic fixture"
            );
        }
    }

    #[test]
    fn rejects_unsafe_change_injection() {
        for key in ["A\n[B]", "\\Absolute", "A\\\\B", "A]", "A\0B"] {
            let mut value = change(RegistryArmValue::Dword(1));
            value.key = key.into();
            assert!(encode_changes(&[value]).is_err());
        }
        let mut value = change(RegistryArmValue::Dword(1));
        value.name = "name\n\"injected\"".into();
        assert!(encode_changes(&[value]).is_err());
        assert!(encode_changes(&[change(RegistryArmValue::String("a\0b".into()))]).is_err());
    }

    #[test]
    fn rejects_bad_headers_dirty_hives_symlinks_and_oversize() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("SYSTEM");
        let clean = hive(&path);
        assert!(checked_hive(&path).is_ok());
        let mut dirty = clean.clone();
        dirty[8] = 2;
        fs::write(&path, dirty).unwrap();
        let adapter = LinuxRegistryHive::with_tool("/nonexistent-tool");
        assert!(adapter
            .read(&path)
            .unwrap_err()
            .to_string()
            .contains("dirty"));
        assert!(adapter.apply(&path, &[]).is_err());
        fs::write(&path, b"regf").unwrap();
        assert!(checked_hive(&path).is_err());
        fs::write(&path, vec![0; 4096]).unwrap();
        assert!(checked_hive(&path).is_err());
        fs::write(&path, clean).unwrap();
        let link = directory.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(checked_hive(&link).is_err());
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_HIVE + 1)
            .unwrap();
        assert!(checked_hive(&path).is_err());
        assert!(checked_hive(directory.path()).is_err());
    }

    #[test]
    fn failed_merge_or_verification_never_changes_original() {
        for body in [
            "printf BROKEN > \"$4\"\nexit 1",
            "if [ \"$1\" = --merge ]; then printf BROKEN > \"$4\"; exit 0; fi\nexit 1",
            "if [ \"$1\" = --merge ]; then printf '\\002' | dd of=\"$4\" bs=1 seek=8 conv=notrunc 2>/dev/null; exit 0; fi\nexit 1",
            "if [ \"$1\" = --merge ]; then exit 0; fi\nprintf '[\\\\Services\\\\Example]\\n\"Other\"=dword:00000001\\n'",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("SYSTEM");
            let original = hive(&path);
            let adapter = LinuxRegistryHive::with_tool(tool(directory.path(), body));
            assert!(adapter.apply(&path, &[change(RegistryArmValue::Dword(1))]).is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
        }
    }

    #[test]
    fn verified_merge_replaces_atomically_and_preserves_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("SYSTEM");
        hive(&path);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let old_inode = fs::metadata(&path).unwrap().ino();
        let body = concat!(
            "if [ \"$1\" = --merge ]; then\n",
            "  printf X | dd of=\"$4\" bs=1 seek=100 conv=notrunc 2>/dev/null\n",
            "  exit 0\nfi\n",
            "printf '[\\\\Services\\\\Example]\\n\"Start\"=dword:00000001\\n'\n"
        );
        let adapter = LinuxRegistryHive::with_tool(tool(directory.path(), body));
        let mut requested = change(RegistryArmValue::Dword(1));
        requested.name = "Start".into();
        adapter.apply(&path, &[requested]).unwrap();
        assert_eq!(fs::read(&path).unwrap()[100], b'X');
        let metadata = fs::metadata(&path).unwrap();
        assert_ne!(metadata.ino(), old_inode);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o640);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}
