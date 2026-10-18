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
const SCRIPTS_CSE_PAIR: &str =
    "[{42B5FAAE-6536-11D2-AE5A-0000F87571E3}{40B6664F-4972-11D1-A7CA-0000F87571E3}]";

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
    let bytes = if gpt.is_file() {
        fs::read(&gpt).context("failed to read Group Policy configuration")?
    } else {
        Vec::new()
    };
    let (text, encoding) = decode_gpt_ini(&bytes)?;
    let merged = merge_gpt_ini(&text)?;
    fs::create_dir_all(gpt.parent().unwrap_or_else(|| Path::new(".")))?;
    fs::write(&gpt, encode_gpt_ini(&merged, encoding))
        .context("failed to update Group Policy configuration")?;
    Ok(())
}

#[derive(Clone, Copy)]
enum GptIniEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
}

fn decode_gpt_ini(bytes: &[u8]) -> Result<(String, GptIniEncoding)> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        if (bytes.len() - 2) % 2 != 0 {
            bail!("Group Policy gpt.ini has an incomplete UTF-16 code unit");
        }
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return Ok((
            String::from_utf16(&units).context("Group Policy gpt.ini is invalid UTF-16")?,
            GptIniEncoding::Utf16Le,
        ));
    }
    let (text, encoding) = if let Some(bytes) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        (bytes, GptIniEncoding::Utf8Bom)
    } else {
        (bytes, GptIniEncoding::Utf8)
    };
    Ok((
        String::from_utf8(text.to_vec()).context("Group Policy gpt.ini is not valid UTF-8")?,
        encoding,
    ))
}

fn encode_gpt_ini(text: &str, encoding: GptIniEncoding) -> Vec<u8> {
    match encoding {
        GptIniEncoding::Utf8 => text.as_bytes().to_vec(),
        GptIniEncoding::Utf8Bom => [b"\xEF\xBB\xBF".as_slice(), text.as_bytes()].concat(),
        GptIniEncoding::Utf16Le => {
            let mut bytes = vec![0xFF, 0xFE];
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            bytes
        }
    }
}

fn merge_gpt_ini(text: &str) -> Result<String> {
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    let Some(start) = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case("[General]"))
    else {
        lines.splice(0..0, ["[General]".to_string()]);
        return merge_gpt_ini(&lines.join("\r\n"));
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| line.trim_start().starts_with('['))
        .map(|(index, _)| index)
        .unwrap_or(lines.len());
    let mut saw_version = false;
    let mut saw_extensions = false;
    for line in &mut lines[start + 1..end] {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("Version") {
            let version = value
                .trim()
                .parse::<u64>()
                .context("Group Policy Version is not a non-negative integer")?
                .saturating_add(1);
            *line = format!("{}={version}", key.trim());
            saw_version = true;
        } else if key.trim().eq_ignore_ascii_case("gPCMachineExtensionNames") {
            *line = format!("{}={}", key.trim(), merge_extension_groups(value));
            saw_extensions = true;
        }
    }
    let mut additions = Vec::new();
    if !saw_version {
        additions.push("Version=1".to_string());
    }
    if !saw_extensions {
        additions.push(format!("gPCMachineExtensionNames={SCRIPTS_CSE_PAIR}"));
    }
    lines.splice(end..end, additions);
    Ok(lines.join("\r\n") + "\r\n")
}

fn merge_extension_groups(value: &str) -> String {
    let mut groups = Vec::new();
    let mut remainder = value;
    while let Some(start) = remainder.find('[') {
        let after_start = &remainder[start..];
        let Some(end) = after_start.find(']') else {
            break;
        };
        groups.push(after_start[..=end].to_string());
        remainder = &after_start[end + 1..];
    }
    if groups.is_empty() && !value.trim().is_empty() {
        groups.push(value.trim().to_string());
    }
    if !value
        .to_ascii_uppercase()
        .contains("{42B5FAAE-6536-11D2-AE5A-0000F87571E3}")
    {
        groups.push(SCRIPTS_CSE_PAIR.to_string());
    }
    groups.sort_by_key(|group| group.to_ascii_uppercase());
    groups.join("")
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
        let gpt =
            fs::read_to_string(root.path().join("Windows/System32/GroupPolicy/gpt.ini")).unwrap();
        assert!(gpt.contains("gPCMachineExtensionNames=[{42B5FAAE-6536-11D2-AE5A-0000F87571E3}{40B6664F-4972-11D1-A7CA-0000F87571E3}]"));
        assert!(gpt.contains("Version=1"));
    }

    #[test]
    fn registers_scripts_cse_while_preserving_existing_extensions_and_bumping_version() {
        let root = tempfile::tempdir().unwrap();
        let drivers = tempfile::tempdir().unwrap();
        fs::write(drivers.path().join("test.inf"), "[Version]").unwrap();
        let group_policy = root.path().join("Windows/System32/GroupPolicy");
        fs::create_dir_all(&group_policy).unwrap();
        fs::write(
            group_policy.join("gpt.ini"),
            "[General]\r\nVersion=12\r\ngPCMachineExtensionNames=[{AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA}{BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB}]\r\n",
        )
        .unwrap();

        stage(root.path(), drivers.path(), true).unwrap();

        let gpt = fs::read_to_string(group_policy.join("gpt.ini")).unwrap();
        assert!(gpt.contains("Version=13"), "{gpt}");
        assert!(
            gpt.contains(
                "{AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA}{BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB}"
            ),
            "{gpt}"
        );
        assert!(
            gpt.contains(
                "{42B5FAAE-6536-11D2-AE5A-0000F87571E3}{40B6664F-4972-11D1-A7CA-0000F87571E3}"
            ),
            "{gpt}"
        );
    }

    #[test]
    fn preserves_utf16le_group_policy_encoding() {
        let root = tempfile::tempdir().unwrap();
        let drivers = tempfile::tempdir().unwrap();
        fs::write(drivers.path().join("test.inf"), "[Version]").unwrap();
        let group_policy = root.path().join("Windows/System32/GroupPolicy");
        fs::create_dir_all(&group_policy).unwrap();
        let text = "[General]\r\nVersion=4\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(group_policy.join("gpt.ini"), bytes).unwrap();

        stage(root.path(), drivers.path(), true).unwrap();

        let bytes = fs::read(group_policy.join("gpt.ini")).unwrap();
        assert!(bytes.starts_with(&[0xFF, 0xFE]));
        let text = String::from_utf16(
            &bytes[2..]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert!(text.contains("Version=5"), "{text}");
        assert!(text.contains("42B5FAAE"), "{text}");
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
