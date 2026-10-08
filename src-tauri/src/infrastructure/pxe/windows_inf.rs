//! Windows INF parsing used by network-driver harvesting and validation.
//!
//! The parser is intentionally platform-independent. It does not execute an INF;
//! it extracts the metadata Diskless Manager needs to select a complete driver
//! package safely before Windows/DISM performs the authoritative installation.

use anyhow::{Context, Result};
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
    pub service_binaries: Vec<String>,
    pub is_network_class: bool,
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
    let mut service_binaries = BTreeSet::new();
    for section in sections.values() {
        let service_section = section.name.to_ascii_lowercase().ends_with(".services");
        for line in &section.lines {
            let Some((key, value)) = split_assignment(line) else {
                continue;
            };
            let key_lower = key.to_ascii_lowercase();

            if service_section && key_lower == "addservice" {
                if let Some(service) = split_csv(value).first() {
                    let service = expand_tokens(service, &strings);
                    if !service.is_empty() {
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
        service_binaries: service_binaries.into_iter().collect(),
        is_network_class,
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
    fn ignores_semicolon_inside_quotes() {
        assert_eq!(
            strip_comment(r#"Name="value;still-value" ; comment"#),
            r#"Name="value;still-value" "#
        );
    }
}
