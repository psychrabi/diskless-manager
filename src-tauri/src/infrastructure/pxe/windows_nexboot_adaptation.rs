//! NexBoot-compatible boot-time Windows NIC adaptation.
//!
//! This intentionally preserves NexBoot's inventory-first, install-on-next-boot
//! behavior rather than using DISM. The target image receives the driver bundle
//! and a one-shot machine startup script; Windows executes pnputil and CDDB
//! setup as SYSTEM on the next boot.

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

const ADAPT_SCRIPT: &str = include_str!("../../../../tools/windows-servicer/assets/auto-adapt.ps1");
const ADAPT_CMD: &str = include_str!("../../../../tools/windows-servicer/assets/ndadapt.cmd");

pub fn stage(image_root: &Path, driver_root: &Path, recursive: bool) -> Result<usize> {
    if !image_root.is_dir() {
        bail!("NexBoot adaptation requires a mounted Windows volume");
    }
    if !driver_root.is_dir() {
        bail!("NexBoot adaptation driver root does not exist");
    }

    let work = image_root.join("ndadapt");
    fs::create_dir_all(&work).context("failed to create NexBoot adaptation directory")?;
    let bundle = work.join("bundle.zip");
    let count = write_driver_bundle(&bundle, driver_root, recursive)?;
    fs::write(work.join("adapt.ps1"), ADAPT_SCRIPT)
        .context("failed to stage NexBoot adaptation script")?;
    fs::write(work.join("ndadapt.cmd"), ADAPT_CMD)
        .context("failed to stage NexBoot startup command")?;
    install_startup_registration(image_root)?;
    Ok(count)
}

fn write_driver_bundle(output: &Path, root: &Path, recursive: bool) -> Result<usize> {
    let file = fs::File::create(output).context("failed to create NexBoot driver bundle")?;
    let mut archive = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut count = 0usize;
    add_directory(&mut archive, options, root, root, recursive, &mut count)?;
    archive
        .finish()
        .context("failed to finalize NexBoot driver bundle")?;
    if count == 0 {
        bail!("NexBoot adaptation driver root contains no files");
    }
    Ok(count)
}

fn add_directory<W: Write + std::io::Seek>(
    archive: &mut zip::ZipWriter<W>,
    options: SimpleFileOptions,
    root: &Path,
    current: &Path,
    recursive: bool,
    count: &mut usize,
) -> Result<()> {
    for entry in
        fs::read_dir(current).with_context(|| format!("failed to read {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .context("driver path escaped driver root")?;
        if path.is_dir() {
            if recursive {
                add_directory(archive, options, root, &path, recursive, count)?;
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let name = relative.to_string_lossy().replace('\\', "/");
        if name.is_empty() || name.starts_with('/') || name.contains("../") {
            bail!("unsafe driver bundle path: {name}");
        }
        archive.start_file(name, options)?;
        let bytes =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        archive.write_all(&bytes)?;
        *count += 1;
    }
    Ok(())
}

fn install_startup_registration(image_root: &Path) -> Result<()> {
    let scripts = image_root.join("Windows/System32/GroupPolicy/Machine/Scripts");
    let startup = scripts.join("Startup");
    fs::create_dir_all(&startup).context("failed to create Group Policy startup directory")?;
    fs::write(startup.join("adapt.ps1"), ADAPT_SCRIPT)
        .context("failed to install Group Policy adaptation script")?;
    fs::write(startup.join("ndadapt.cmd"), ADAPT_CMD)
        .context("failed to install Group Policy startup command")?;
    let ini_path = scripts.join("scripts.ini");
    let existing = if ini_path.is_file() {
        fs::read(&ini_path).context("failed to read existing Group Policy scripts")?
    } else {
        Vec::new()
    };
    let utf16 = existing.starts_with(&[0xFF, 0xFE]);
    let text = if utf16 {
        let units = existing[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        String::from_utf16(&units).context("existing Group Policy scripts are invalid UTF-16")?
    } else {
        String::from_utf8_lossy(&existing).into_owned()
    };
    let merged = merge_startup_ini(&text);
    let bytes = if utf16 {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in merged.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes
    } else {
        merged.into_bytes()
    };
    fs::write(&ini_path, bytes).context("failed to merge Group Policy startup command")?;

    let gpt = image_root.join("Windows/System32/GroupPolicy/gpt.ini");
    let mut text = if gpt.is_file() {
        fs::read_to_string(&gpt).context("failed to read Group Policy version")?
    } else {
        "[General]\r\nVersion=0\r\n".to_string()
    };
    let mut changed = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Version=") {
            let version = value.trim().parse::<u64>().unwrap_or(0).saturating_add(1);
            lines.push(format!("Version={version}"));
            changed = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !changed {
        lines.push("Version=1".to_string());
    }
    text = lines.join("\r\n") + "\r\n";
    fs::create_dir_all(gpt.parent().unwrap_or_else(|| Path::new(".")))?;
    fs::write(gpt, text).context("failed to update Group Policy version")?;
    Ok(())
}

fn merge_startup_ini(text: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let Some(start) = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case("[Startup]"))
    else {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend([
            "[Startup]".into(),
            "0CmdLine=ndadapt.cmd".into(),
            "0Parameters=".into(),
        ]);
        return lines.join("\r\n") + "\r\n";
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| line.trim_start().starts_with('['))
        .map(|(index, _)| index)
        .unwrap_or(lines.len());
    let mut remove_numbers = std::collections::BTreeSet::new();
    for line in &lines[start + 1..end] {
        if let Some((left, value)) = line.split_once('=') {
            let digits = left
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            if left[digits.len()..].eq_ignore_ascii_case("CmdLine")
                && value.trim().eq_ignore_ascii_case("ndadapt.cmd")
            {
                remove_numbers.insert(digits);
            }
        }
    }
    let mut items = Vec::new();
    for line in &lines[start + 1..end] {
        let digits = line
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        if digits.is_empty() || !remove_numbers.contains(&digits) {
            items.push(line.clone());
        }
    }
    let next = items
        .iter()
        .filter_map(|line| {
            let digits = line
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            (!digits.is_empty())
                .then(|| digits.parse::<u64>().ok())
                .flatten()
        })
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    items.extend([
        format!("{next}CmdLine=ndadapt.cmd"),
        format!("{next}Parameters="),
    ]);
    lines.splice(start + 1..end, items);
    lines.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stages_bundle_and_one_shot_startup_files() {
        let root = tempfile::tempdir().unwrap();
        let drivers = tempfile::tempdir().unwrap();
        fs::create_dir_all(drivers.path().join("nested")).unwrap();
        fs::write(drivers.path().join("nested/test.inf"), "[Version]").unwrap();
        assert_eq!(stage(root.path(), drivers.path(), true).unwrap(), 1);
        assert!(root.path().join("ndadapt/bundle.zip").is_file());
        assert!(root.path().join("ndadapt/adapt.ps1").is_file());
        assert!(root
            .path()
            .join("Windows/System32/GroupPolicy/Machine/Scripts/scripts.ini")
            .is_file());
    }

    #[test]
    fn merges_startup_without_removing_other_entries_or_shutdown() {
        let ini = "[Startup]\r\n0CmdLine=ndmount.cmd\r\n0Parameters=\r\n[Shutdown]\r\n0CmdLine=cleanup.cmd\r\n";
        let merged = merge_startup_ini(ini);
        assert!(merged.contains("0CmdLine=ndmount.cmd"));
        assert!(merged.contains("[Shutdown]"));
        assert!(merged.contains("1CmdLine=ndadapt.cmd"));
    }
}
