//! Windows INF parsing used by network-driver harvesting and validation.
//!
//! The parser is intentionally platform-independent. It does not execute an INF;
//! it extracts the metadata Diskless Manager needs to select a complete driver
//! package safely before Windows/DISM performs the authoritative installation.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const NETWORK_CLASS_GUID: &str = "4d36e972-e325-11ce-bfc1-08002be10318";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WindowsInfMetadata {
    pub path: String,
    pub class: Option<String>,
    pub class_guid: Option<String>,
    pub provider: Option<String>,
    pub version: Option<String>,
    pub signature: Option<String>,
    pub catalog_files: Vec<String>,
    pub architectures: Vec<String>,
    pub hardware_ids: Vec<String>,
    pub compatible_ids: Vec<String>,
    pub service_names: Vec<String>,
    #[serde(default)]
    pub associated_service_names: Vec<String>,
    pub service_binaries: Vec<String>,
    pub is_network_class: bool,
}

/// A bounded subset of kernel service declarations for offline boot reflection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfBootService {
    pub name: String,
    pub binary: String,
    pub group: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub error_control: u32,
    pub boot_flags: u32,
    pub dependencies: Vec<String>,
    pub parameters: Vec<(String, String, InfServiceValue)>,
    pub event_log: Vec<(String, InfServiceValue)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfServiceValue {
    String(String),
    ExpandString(String),
    Dword(u32),
    MultiString(Vec<String>),
}

pub fn boot_service_profiles(path: &Path) -> Result<Vec<InfBootService>> {
    let text = decode_inf_bytes(&fs::read(path)?);
    let sections = parse_sections(&text);
    let strings = collect_strings(&sections);
    let mut profiles = BTreeMap::new();
    for section in sections
        .values()
        .filter(|s| s.name.to_ascii_lowercase().ends_with(".services"))
    {
        let associated = section
            .lines
            .iter()
            .filter_map(|line| split_assignment(line))
            .filter(|(key, _)| key.eq_ignore_ascii_case("AddService"))
            .map(|(_, value)| {
                split_csv(value)
                    .iter()
                    .map(|v| expand_tokens(v, &strings))
                    .collect::<Vec<_>>()
            })
            .any(|fields| {
                fields.first().is_some_and(|v| !v.is_empty())
                    && fields
                        .get(1)
                        .and_then(|v| parse_inf_number(v))
                        .is_some_and(|v| v & 2 != 0)
            });
        if !associated {
            continue;
        }
        for line in &section.lines {
            let (key, value) = split_assignment(line).context("invalid service directive")?;
            if !key.eq_ignore_ascii_case("AddService") {
                bail!("unsupported service requirement {key}; install this NIC in the image first");
            }
            let fields: Vec<_> = split_csv(value)
                .iter()
                .map(|v| expand_tokens(v, &strings))
                .collect();
            if !(3..=4).contains(&fields.len())
                || fields[0].is_empty()
                || parse_inf_number(&fields[1]) != Some(2)
            {
                bail!("unsupported AddService requirements; install this NIC in the image first");
            }
            let name = fields
                .first()
                .context("missing associated service name")?
                .clone();
            let install = sections
                .get(
                    &fields
                        .get(2)
                        .context("missing service installation section")?
                        .to_ascii_lowercase(),
                )
                .context("service installation section not found")?;
            let mut values = BTreeMap::new();
            let mut reg_sections = Vec::new();
            for line in &install.lines {
                let (key, value) =
                    split_assignment(line).context("invalid service installation directive")?;
                let key = key.to_ascii_lowercase();
                let value = expand_tokens(value, &strings);
                if key == "addreg" {
                    reg_sections.extend(split_csv(&value));
                } else if let Some(old) = values.insert(key.clone(), value.clone()) {
                    if old != value {
                        bail!("conflicting service directive {key} for {name}");
                    }
                }
            }
            let number = |key: &str| -> Result<u32> {
                values
                    .get(key)
                    .and_then(|v| parse_inf_number(v))
                    .context(format!("invalid service {key}"))
            };
            if number("servicetype")? != 1
                || number("starttype")? > 3
                || number("errorcontrol")? > 3
            {
                bail!("unsupported kernel service declaration for {name}");
            }
            for key in values.keys() {
                if ![
                    "servicetype",
                    "starttype",
                    "errorcontrol",
                    "servicebinary",
                    "loadordergroup",
                    "dependencies",
                    "displayname",
                    "description",
                    "bootflags",
                    "addreg",
                ]
                .contains(&key.as_str())
                {
                    bail!("unsupported service directive {key} for {name}; install this NIC in the image first");
                }
            }
            let binary = values
                .get("servicebinary")
                .context("service binary is missing")?
                .clone();
            let mut boot_flags = values
                .get("bootflags")
                .map(|v| parse_inf_number(v).context("invalid BootFlags"))
                .transpose()?
                .unwrap_or(0);
            let mut parameters = Vec::new();
            {
                for name in reg_sections {
                    let reg = sections
                        .get(&name.to_ascii_lowercase())
                        .context("service AddReg section not found")?;
                    for line in &reg.lines {
                        let fields: Vec<_> = split_csv(line)
                            .iter()
                            .map(|v| expand_tokens(v, &strings))
                            .collect();
                        if fields.len() < 4 || !fields[0].eq_ignore_ascii_case("HKR") {
                            bail!("only service-relative AddReg entries are supported");
                        }
                        let key = fields[1].clone();
                        let name = fields[2].clone();
                        if key.split('\\').any(|v| v == ".." || v == ".")
                            || key.starts_with('\\')
                            || key.contains('/')
                            || key.contains('%')
                            || name.contains('\0')
                        {
                            bail!("invalid service-relative registry path");
                        }
                        if name.is_empty()
                            || (key.is_empty()
                                && [
                                    "type",
                                    "imagepath",
                                    "start",
                                    "errorcontrol",
                                    "group",
                                    "dependonservice",
                                ]
                                .contains(&name.to_ascii_lowercase().as_str()))
                        {
                            bail!("unsupported service AddReg value");
                        }
                        let flags = if fields[3].is_empty() {
                            0
                        } else {
                            parse_inf_number(&fields[3]).context("unsupported AddReg flags")?
                        };
                        let value = match flags {
                            0 if fields.len() <= 5 => InfServiceValue::String(fields.get(4).cloned().unwrap_or_default()),
                            0x10001 if fields.len() == 5 => InfServiceValue::Dword(fields.get(4).and_then(|v| parse_inf_number(v)).context("invalid service DWORD")?),
                            0x10000 => InfServiceValue::MultiString(fields[4..].to_vec()),
                            _ => bail!("unsupported service AddReg flags; install this NIC in the image first"),
                        };
                        if key.is_empty() && name.eq_ignore_ascii_case("BootFlags") {
                            let InfServiceValue::Dword(flags) = value else {
                                bail!("BootFlags must be a DWORD")
                            };
                            boot_flags = flags;
                        } else {
                            parameters.push((key, name, value));
                        }
                    }
                }
            }
            if values.get("dependencies").is_some_and(|v| {
                split_csv(v)
                    .iter()
                    .any(|d| d.starts_with('+') || d.contains('%'))
            }) {
                bail!("unsupported service dependencies; install this NIC in the image first");
            }
            let profile = InfBootService {
                name: name.clone(),
                binary,
                group: values.get("loadordergroup").cloned(),
                display_name: values.get("displayname").cloned(),
                description: values.get("description").cloned(),
                error_control: number("errorcontrol")?,
                boot_flags,
                dependencies: values
                    .get("dependencies")
                    .map(|v| split_csv(v))
                    .unwrap_or_default(),
                parameters,
                event_log: fields
                    .get(3)
                    .filter(|s| !s.is_empty())
                    .map(|name| event_log_values(name, &sections, &strings))
                    .transpose()?
                    .unwrap_or_default(),
            };
            if let Some(old) = profiles.insert(name.to_ascii_lowercase(), profile.clone()) {
                if old != profile {
                    bail!("architecture/OS service declarations differ; refusing ambiguous offline reflection for {name}");
                }
            }
        }
    }
    Ok(profiles.into_values().collect())
}

fn event_log_values(
    name: &str,
    sections: &BTreeMap<String, InfSection>,
    strings: &BTreeMap<String, String>,
) -> Result<Vec<(String, InfServiceValue)>> {
    let section = sections
        .get(&name.to_ascii_lowercase())
        .context("event-log section not found")?;
    let mut values = Vec::new();
    for line in &section.lines {
        let (key, value) = split_assignment(line).context("invalid event-log directive")?;
        if !key.eq_ignore_ascii_case("AddReg") {
            bail!("unsupported event-log requirement {key}");
        }
        for name in split_csv(&expand_tokens(value, strings)) {
            let reg = sections
                .get(&name.to_ascii_lowercase())
                .context("event-log AddReg section not found")?;
            for line in &reg.lines {
                let fields: Vec<_> = split_csv(line)
                    .iter()
                    .map(|v| expand_tokens(v, strings))
                    .collect();
                if fields.len() != 5
                    || !fields[0].eq_ignore_ascii_case("HKR")
                    || !fields[1].is_empty()
                {
                    bail!("only event-source-relative values are supported");
                }
                let flags = parse_inf_number(&fields[3]);
                let value = if fields[2].eq_ignore_ascii_case("EventMessageFile")
                    && flags == Some(0x20000)
                {
                    let path = fields[4].replace("%%", "%");
                    if !path.split(';').all(|p| {
                        p.to_ascii_lowercase()
                            .starts_with("%systemroot%\\system32\\")
                            && !p[13..].contains(['%', ':', '/'])
                            && !p.split('\\').any(|v| v == ".." || v == ".")
                    }) {
                        bail!("event message files must be inside Windows System32");
                    }
                    InfServiceValue::ExpandString(path)
                } else if fields[2].eq_ignore_ascii_case("TypesSupported") && flags == Some(0x10001)
                {
                    InfServiceValue::Dword(
                        parse_inf_number(&fields[4]).context("invalid event TypesSupported")?,
                    )
                } else {
                    bail!("unsupported event-log registry requirement");
                };
                values.push((fields[2].clone(), value));
            }
        }
    }
    Ok(values)
}

#[derive(Debug, Clone)]
struct InfSection {
    name: String,
    lines: Vec<String>,
}

pub fn inspect_inf_file(path: &Path) -> Result<WindowsInfMetadata> {
    let bytes = fs::read(path).with_context(|| format!("failed to read INF {}", path.display()))?;
    let text = decode_inf_bytes(&bytes);
    let mut metadata = parse_inf(&text);
    metadata.path = path.display().to_string();
    Ok(metadata)
}

pub fn decode_inf_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16(&bytes[2..], true);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16(&bytes[2..], false);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }

    match std::str::from_utf8(bytes) {
        Ok(value) => value.to_owned(),
        Err(_) => {
            // Most legacy INF syntax and hardware IDs are ASCII even when the
            // descriptive strings use an ANSI code page. Lossy conversion keeps
            // the structural tokens intact and avoids rejecting an otherwise
            // valid package merely because its description is not UTF-8.
            String::from_utf8_lossy(bytes).into_owned()
        }
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let words = bytes
        .chunks_exact(2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&words)
}

pub fn parse_inf(content: &str) -> WindowsInfMetadata {
    let sections = parse_sections(content);
    let strings = collect_strings(&sections);

    let class =
        section_value(&sections, "version", "class").map(|value| expand_tokens(&value, &strings));
    let class_guid = section_value(&sections, "version", "classguid")
        .map(|value| expand_tokens(&value, &strings));
    let provider = section_value(&sections, "version", "provider")
        .map(|value| expand_tokens(&value, &strings));
    let version = section_value(&sections, "version", "driverver")
        .map(|value| expand_tokens(&value, &strings));
    let signature = section_value(&sections, "version", "signature")
        .map(|value| expand_tokens(&value, &strings));

    let mut catalog_files = BTreeSet::new();
    if let Some(version_section) = sections.get("version") {
        for line in &version_section.lines {
            if let Some((key, value)) = split_assignment(line) {
                if key.to_ascii_lowercase().starts_with("catalogfile") {
                    let value = expand_tokens(value, &strings);
                    if !value.is_empty() {
                        catalog_files.insert(value);
                    }
                }
            }
        }
    }

    let model_sections = discover_model_sections(&sections, &strings);
    let mut hardware_ids = BTreeSet::new();
    let mut compatible_ids = BTreeSet::new();

    for section_name in &model_sections {
        let Some(section) = sections.get(section_name) else {
            continue;
        };
        for line in &section.lines {
            let Some((_, rhs)) = split_assignment(line) else {
                continue;
            };
            let fields = split_csv(rhs)
                .into_iter()
                .map(|field| expand_tokens(&field, &strings))
                .collect::<Vec<_>>();
            if fields.len() < 2 {
                continue;
            }

            if looks_like_device_id(&fields[1]) {
                hardware_ids.insert(normalize_device_id(&fields[1]));
            }
            for compatible in fields.iter().skip(2) {
                if looks_like_device_id(compatible) {
                    compatible_ids.insert(normalize_device_id(compatible));
                }
            }
        }
    }

    // Some vendor INFs omit a conventional [Manufacturer] mapping. Fall back
    // to model-looking assignments, but still only extract device-ID fields
    // from the comma-separated right hand side.
    if hardware_ids.is_empty() {
        for section in sections.values() {
            for line in &section.lines {
                let Some((_, rhs)) = split_assignment(line) else {
                    continue;
                };
                let fields = split_csv(rhs);
                if fields.len() < 2 || !looks_like_device_id(&fields[1]) {
                    continue;
                }
                hardware_ids.insert(normalize_device_id(&expand_tokens(&fields[1], &strings)));
                for compatible in fields.iter().skip(2) {
                    let compatible = expand_tokens(compatible, &strings);
                    if looks_like_device_id(&compatible) {
                        compatible_ids.insert(normalize_device_id(&compatible));
                    }
                }
            }
        }
    }

    let mut service_names = BTreeSet::new();
    let mut associated_service_names = BTreeSet::new();
    let mut service_binaries = BTreeSet::new();
    for section in sections.values() {
        let service_section = section.name.to_ascii_lowercase().ends_with(".services");
        for line in &section.lines {
            let Some((key, value)) = split_assignment(line) else {
                continue;
            };
            let key_lower = key.to_ascii_lowercase();

            if service_section && key_lower == "addservice" {
                let fields = split_csv(value);
                if let Some(service) = fields.first() {
                    let service = expand_tokens(service, &strings);
                    if !service.is_empty() {
                        let flags = fields.get(1).map(|value| expand_tokens(value, &strings));
                        if flags
                            .as_deref()
                            .and_then(parse_inf_number)
                            .is_some_and(|flags| flags & 2 != 0)
                        {
                            associated_service_names.insert(service.clone());
                        }
                        service_names.insert(service);
                    }
                }
            }

            if key_lower == "servicebinary" {
                let binary = expand_tokens(value, &strings);
                if !binary.is_empty() {
                    service_binaries.insert(binary);
                }
            }
        }
    }

    let mut architectures = BTreeSet::new();
    for section in sections.values() {
        detect_architectures(&section.name, &mut architectures);
    }

    let is_network_class = class
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("Net"))
        || class_guid.as_deref().is_some_and(|value| {
            value
                .trim_matches(|ch| ch == '{' || ch == '}')
                .eq_ignore_ascii_case(NETWORK_CLASS_GUID)
        });

    WindowsInfMetadata {
        path: String::new(),
        class,
        class_guid,
        provider,
        version,
        signature,
        catalog_files: catalog_files.into_iter().collect(),
        architectures: architectures.into_iter().collect(),
        hardware_ids: hardware_ids.into_iter().collect(),
        compatible_ids: compatible_ids.into_iter().collect(),
        service_names: service_names.into_iter().collect(),
        associated_service_names: associated_service_names.into_iter().collect(),
        service_binaries: service_binaries.into_iter().collect(),
        is_network_class,
    }
}

fn parse_inf_number(value: &str) -> Option<u32> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn parse_sections(content: &str) -> BTreeMap<String, InfSection> {
    let mut sections = BTreeMap::new();
    let mut current: Option<String> = None;

    for line in logical_lines(content) {
        let trimmed = line.trim();
        if trimmed.len() >= 2 && trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed[1..trimmed.len() - 1].trim().to_string();
            let key = name.to_ascii_lowercase();
            sections.entry(key.clone()).or_insert_with(|| InfSection {
                name,
                lines: Vec::new(),
            });
            current = Some(key);
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        if let Some(section) = current.as_ref().and_then(|name| sections.get_mut(name)) {
            section.lines.push(trimmed.to_string());
        }
    }

    sections
}

fn logical_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut pending = String::new();

    for raw in content.lines() {
        let without_comment = strip_comment(raw);
        let trimmed = without_comment.trim();
        if trimmed.is_empty() {
            continue;
        }

        let continued = trimmed.ends_with('\\');
        let piece = if continued {
            trimmed.trim_end_matches('\\').trim_end()
        } else {
            trimmed
        };

        if !pending.is_empty() && !piece.is_empty() {
            pending.push(' ');
        }
        pending.push_str(piece);

        if !continued {
            lines.push(std::mem::take(&mut pending));
        }
    }

    if !pending.is_empty() {
        lines.push(pending);
    }

    lines
}

fn strip_comment(line: &str) -> String {
    let mut quoted = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ';' if !quoted => return line[..index].to_string(),
            _ => {}
        }
    }
    line.to_string()
}

fn collect_strings(sections: &BTreeMap<String, InfSection>) -> BTreeMap<String, String> {
    let mut strings = BTreeMap::new();
    for (name, section) in sections {
        if name != "strings" && !name.starts_with("strings.") {
            continue;
        }
        for line in &section.lines {
            if let Some((key, value)) = split_assignment(line) {
                strings.insert(
                    key.trim().to_ascii_lowercase(),
                    trim_inf_value(value).to_string(),
                );
            }
        }
    }
    strings
}

fn discover_model_sections(
    sections: &BTreeMap<String, InfSection>,
    strings: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let mut models = BTreeSet::new();
    let Some(manufacturer) = sections.get("manufacturer") else {
        return models;
    };

    for line in &manufacturer.lines {
        let Some((_, rhs)) = split_assignment(line) else {
            continue;
        };
        let fields = split_csv(rhs)
            .into_iter()
            .map(|field| expand_tokens(&field, strings))
            .collect::<Vec<_>>();
        let Some(base) = fields.first() else {
            continue;
        };

        let base_key = base.to_ascii_lowercase();
        if sections.contains_key(&base_key) {
            models.insert(base_key);
        }

        for decoration in fields.iter().skip(1) {
            let decorated = format!("{}.{}", base, decoration).to_ascii_lowercase();
            if sections.contains_key(&decorated) {
                models.insert(decorated);
            }
        }

        // Vendor INFs sometimes declare architecture-specific model sections
        // without listing every decoration on the Manufacturer line.
        let prefix = format!("{}.", base).to_ascii_lowercase();
        for name in sections.keys() {
            if name.starts_with(&prefix)
                && (name.contains("ntamd64") || name.contains("ntx86") || name.contains("ntarm64"))
            {
                models.insert(name.clone());
            }
        }
    }

    models
}

fn detect_architectures(section_name: &str, output: &mut BTreeSet<String>) {
    let lower = section_name.to_ascii_lowercase();
    if lower.contains("ntamd64") {
        output.insert("x64".to_string());
    }
    if lower.contains("ntx86") {
        output.insert("x86".to_string());
    }
    if lower.contains("ntarm64") {
        output.insert("arm64".to_string());
    }
}

fn section_value(
    sections: &BTreeMap<String, InfSection>,
    section: &str,
    key: &str,
) -> Option<String> {
    sections
        .get(&section.to_ascii_lowercase())?
        .lines
        .iter()
        .find_map(|line| {
            let (candidate, value) = split_assignment(line)?;
            candidate
                .eq_ignore_ascii_case(key)
                .then(|| trim_inf_value(value).to_string())
        })
}

fn split_assignment(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim()))
}

fn split_csv(value: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for ch in value.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(trim_inf_value(&current).to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    fields.push(trim_inf_value(&current).to_string());
    fields
}

fn trim_inf_value(value: &str) -> &str {
    value.trim().trim_matches('"').trim()
}

fn expand_tokens(value: &str, strings: &BTreeMap<String, String>) -> String {
    let mut result = trim_inf_value(value).to_string();

    for _ in 0..4 {
        let Some(start) = result.find('%') else {
            break;
        };
        let Some(relative_end) = result[start + 1..].find('%') else {
            break;
        };
        let end = start + 1 + relative_end;
        let token = result[start + 1..end].to_ascii_lowercase();
        let Some(replacement) = strings.get(&token) else {
            break;
        };
        result.replace_range(start..=end, replacement);
    }

    trim_inf_value(&result).to_string()
}

fn looks_like_device_id(value: &str) -> bool {
    let lower = trim_inf_value(value).to_ascii_lowercase();
    ["pci\\", "usb\\", "vmbus\\", "acpi\\", "root\\"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

fn normalize_device_id(value: &str) -> String {
    trim_inf_value(value)
        .replace('/', "\\")
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[Version]
Signature="$WINDOWS NT$"
Class=Net
ClassGuid={4d36e972-e325-11ce-bfc1-08002be10318}
Provider=%Provider%
DriverVer=08/01/2026,12.3.4.5
CatalogFile.NTamd64=sample.cat

[Manufacturer]
%Provider%=Models,NTamd64

[Models.NTamd64]
%Device%=Install, PCI\VEN_8086&DEV_15F3, PCI\VEN_8086&CC_0200

[Install.NT.Services]
AddService = e2fexpress, 2, ServiceInstall

[ServiceInstall]
ServiceBinary = %12%\e2fexpress.sys

[Strings]
Provider="Example Vendor"
Device="Example Ethernet"
"#;

    #[test]
    fn parses_network_package_metadata() {
        let metadata = parse_inf(SAMPLE);
        assert!(metadata.is_network_class);
        assert_eq!(metadata.provider.as_deref(), Some("Example Vendor"));
        assert_eq!(metadata.version.as_deref(), Some("08/01/2026,12.3.4.5"));
        assert_eq!(metadata.architectures, vec!["x64"]);
        assert_eq!(metadata.hardware_ids, vec!["PCI\\VEN_8086&DEV_15F3"]);
        assert_eq!(metadata.compatible_ids, vec!["PCI\\VEN_8086&CC_0200"]);
        assert_eq!(metadata.service_names, vec!["e2fexpress"]);
        assert_eq!(metadata.catalog_files, vec!["sample.cat"]);
    }

    #[test]
    fn decodes_utf16le_inf() {
        let text = "[Version]\r\nClass=Net\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for word in text.encode_utf16() {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        assert!(decode_inf_bytes(&bytes).contains("Class=Net"));
    }

    #[test]
    fn distinguishes_function_driver_from_filter_services() {
        let metadata = parse_inf(&SAMPLE.replace(
            "AddService = e2fexpress, 2, ServiceInstall",
            "AddService = filter, 0, FilterInstall\nAddService = %NicService%, %Flags%, ServiceInstall",
        ).replace("Provider=\"Example Vendor\"", "Provider=\"Example Vendor\"\nNicService=\"e2fexpress\"\nFlags=0x00000012"));
        assert_eq!(metadata.associated_service_names, vec!["e2fexpress"]);
        assert_eq!(metadata.service_names, vec!["e2fexpress", "filter"]);
    }

    #[test]
    fn ignores_semicolon_inside_quotes() {
        assert_eq!(
            strip_comment(r#"Name="value;still-value" ; comment"#),
            r#"Name="value;still-value" "#
        );
    }
}

#[cfg(test)]
mod boot_profile_tests {
    use super::*;
    #[test]
    fn reflects_nic_event_log_and_bootflags_without_skipping_requirements() {
        let dir = tempfile::tempdir().unwrap();
        let inf = dir.path().join("nic.inf");
        let content = r#"[Device.Services]
AddService=Nic,2,Service,Events
[Service]
ServiceType=1
StartType=3
ErrorControl=1
ServiceBinary=%13%\nic.sys
AddReg=Flags
[Flags]
HKR,,BootFlags,0x10001,4
[Events]
AddReg=EventValues
[EventValues]
HKR,,EventMessageFile,0x20000,"%%SystemRoot%%\System32\netevent.dll"
HKR,,TypesSupported,0x10001,7
"#;
        fs::write(&inf, content).unwrap();
        let profile = boot_service_profiles(&inf).unwrap().remove(0);
        assert_eq!(profile.boot_flags, 4);
        assert_eq!(
            profile.event_log[0].1,
            InfServiceValue::ExpandString(r"%SystemRoot%\System32\netevent.dll".into())
        );
        for (from, to) in [
            ("Service,Events", "Service,Events,Security,Other"),
            ("HKR,,TypesSupported", "HKLM,,TypesSupported"),
            ("System32\\netevent.dll", "System32\\..\\outside.dll"),
            ("TypesSupported", "ArbitraryValue"),
        ] {
            fs::write(&inf, content.replace(from, to)).unwrap();
            assert!(boot_service_profiles(&inf).is_err(), "{to}");
        }
    }
    #[test]
    fn preserves_repeated_addreg_and_rejects_unhandled_service_requirements() {
        let dir = tempfile::tempdir().unwrap();
        let inf = dir.path().join("nic.inf");
        let content = "[Device.Services]\nAddService=Nic,2,Service\n[Service]\nServiceType=1\nStartType=3\nErrorControl=1\nServiceBinary=%13%\\nic.sys\nAddReg=First\nAddReg=Second\n[First]\nHKR,Parameters,First,0x10001,1\n[Second]\nHKR,Parameters,Second,0x10001,2\n";
        fs::write(&inf, content).unwrap();
        assert_eq!(boot_service_profiles(&inf).unwrap()[0].parameters.len(), 2);
        for replacement in [
            "AddService=Nic,2,Service\nInclude=other.inf\nNeeds=Other.Services",
            "AddService=Nic,2,Service\nAddService=Filter,0,FilterService",
            "AddService=Nic,0x802,Service",
            "AddService=Nic,2,Service,EventLog",
        ] {
            fs::write(
                &inf,
                content.replace("AddService=Nic,2,Service", replacement),
            )
            .unwrap();
            assert!(boot_service_profiles(&inf).is_err(), "{replacement}");
        }
        fs::write(
            &inf,
            content.replace("StartType=3", "StartType=3\nStartType=0"),
        )
        .unwrap();
        assert!(boot_service_profiles(&inf).is_err());
    }

    #[test]
    fn windows_profiles_preserve_parameters_and_reject_foreign_registry_writes() {
        let dir = tempfile::tempdir().unwrap();
        let inf = dir.path().join("nic.inf");
        let content = "[Device.NT.Services]\nAddService=netkvm,2,Service\n[Service]\nServiceType=1\nStartType=3\nErrorControl=1\nServiceBinary=%13%\\netkvm.sys\nLoadOrderGroup=NDIS\nAddReg=Params\n[Params]\nHKR,Parameters,DisableMSI,,0\nHKR,Parameters,Value,0x10001,5\n";
        fs::write(&inf, content).unwrap();
        let profiles = boot_service_profiles(&inf).unwrap();
        assert_eq!(profiles[0].binary, "%13%\\netkvm.sys");
        assert_eq!(profiles[0].parameters[1].2, InfServiceValue::Dword(5));
        fs::write(&inf, content.replace("HKR,Parameters", "HKLM,Parameters")).unwrap();
        assert!(boot_service_profiles(&inf).is_err());
        fs::write(&inf,format!("{content}\n[Other.Services]\nAddService=netkvm,2,Different\n[Different]\nServiceType=1\nStartType=3\nErrorControl=1\nServiceBinary=%12%\\other.sys\n")).unwrap();
        assert!(boot_service_profiles(&inf).is_err());
    }
}
