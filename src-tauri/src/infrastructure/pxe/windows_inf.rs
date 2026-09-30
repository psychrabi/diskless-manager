//! Windows INF parsing used by driver validation, harvesting and selection.
//!
//! The parser is intentionally conservative: it extracts package identity and
//! matching metadata, but it does not try to emulate SetupAPI. Driver
//! installation remains the responsibility of DISM/Windows.

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const NETWORK_CLASS_GUID: &str = "4d36e972-e325-11ce-bfc1-08002be10318";

#[derive(Debug, Clone, Serialize)]
pub struct WindowsInfMetadata {
    pub class: Option<String>,
    pub class_guid: Option<String>,
    pub provider: Option<String>,
    pub driver_date: Option<String>,
    pub driver_version: Option<String>,
    pub catalog_files: Vec<String>,
    pub architectures: Vec<String>,
    pub hardware_ids: Vec<String>,
    pub compatible_ids: Vec<String>,
    pub service_names: Vec<String>,
    pub service_binaries: Vec<String>,
    pub is_network_class: bool,
}

#[derive(Debug, Clone, Default)]
struct InfDocument {
    sections: BTreeMap<String, Vec<(String, String)>>,
    strings: BTreeMap<String, String>,
}

pub fn read_inf_text(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read INF {}", path.display()))?;
    Ok(decode_inf_bytes(&bytes))
}

pub fn parse_inf_file(path: &Path) -> Result<WindowsInfMetadata> {
    let text = read_inf_text(path)?;
    Ok(parse_inf_text(&text))
}

pub fn parse_inf_text(text: &str) -> WindowsInfMetadata {
    let document = InfDocument::parse(text);

    let version = document.section("version");
    let class = version
        .and_then(|items| first_value(items, "class"))
        .map(|value| document.expand(&value));
    let class_guid = version
        .and_then(|items| first_value(items, "classguid"))
        .map(|value| document.expand(&value));
    let provider = version
        .and_then(|items| first_value(items, "provider"))
        .map(|value| document.expand(&value));

    let (driver_date, driver_version) = version
        .and_then(|items| first_value(items, "driverver"))
        .map(|value| {
            let expanded = document.expand(&value);
            let mut parts = expanded.splitn(2, ',');
            (
                parts.next().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string),
                parts.next().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string),
            )
        })
        .unwrap_or((None, None));

    let mut catalog_files = BTreeSet::new();
    if let Some(items) = version {
        for (key, value) in items {
            if key.eq_ignore_ascii_case("catalogfile")
                || key.to_ascii_lowercase().starts_with("catalogfile.")
            {
                let expanded = document.expand(value);
                if !expanded.is_empty() {
                    catalog_files.insert(expanded);
                }
            }
        }
    }

    let mut architectures = BTreeSet::new();
    let mut model_sections = BTreeSet::new();

    if let Some(manufacturers) = document.section("manufacturer") {
        for (_, value) in manufacturers {
            let expanded = document.expand(value);
            let fields = split_csv(&expanded);
            if let Some(base) = fields.first() {
                if !base.is_empty() {
                    model_sections.insert(base.to_ascii_lowercase());
                }
            }
            for decoration in fields.iter().skip(1) {
                if let Some(arch) = architecture_from_decoration(decoration) {
                    architectures.insert(arch.to_string());
                }
                if let Some(base) = fields.first() {
                    model_sections
                        .insert(format!("{}.{}", base, decoration).to_ascii_lowercase());
                }
            }
        }
    }

    // Some packages omit [Manufacturer] decorations but still use decorated model
    // sections. Discover those sections as a fallback.
    for section_name in document.sections.keys() {
        if let Some(arch) = architecture_from_decoration(section_name) {
            architectures.insert(arch.to_string());
        }
    }

    let mut hardware_ids = BTreeSet::new();
    let mut compatible_ids = BTreeSet::new();

    let model_candidates: Vec<String> = if model_sections.is_empty() {
        document
            .sections
            .keys()
            .filter(|name| {
                name.contains(".nt")
                    && !name.ends_with(".services")
                    && !name.contains(".coinstallers")
                    && !name.contains(".hw")
            })
            .cloned()
            .collect()
    } else {
        model_sections.into_iter().collect()
    };

    for section_name in model_candidates {
        let Some(items) = document.section(&section_name) else {
            continue;
        };
        for (_, value) in items {
            let expanded = document.expand(value);
            let fields = split_csv(&expanded);
            // Model syntax: InstallSection, HardwareId[, CompatibleId...]
            if fields.len() < 2 {
                continue;
            }

            if looks_like_device_id(&fields[1]) {
                hardware_ids.insert(normalize_device_id(&fields[1]));
            }
            for field in fields.iter().skip(2) {
                if looks_like_device_id(field) {
                    compatible_ids.insert(normalize_device_id(field));
                }
            }
        }
    }

    // Fallback for unusual INFs: inspect RHS fields while still requiring a
    // recognizable PnP namespace. This avoids the old behavior of treating the
    // complete "InstallSection, PCI\\VEN_..." RHS as the hardware ID.
    if hardware_ids.is_empty() {
        for items in document.sections.values() {
            for (_, value) in items {
                let expanded = document.expand(value);
                let fields = split_csv(&expanded);
                for field in fields.iter().skip(1) {
                    if looks_like_device_id(field) {
                        hardware_ids.insert(normalize_device_id(field));
                    }
                }
            }
        }
    }

    let mut service_names = BTreeSet::new();
    let mut service_binaries = BTreeSet::new();
    for items in document.sections.values() {
        for (key, value) in items {
            if key.eq_ignore_ascii_case("addservice") {
                let expanded = document.expand(value);
                if let Some(service) = split_csv(&expanded).first() {
                    if !service.is_empty() {
                        service_names.insert(service.to_string());
                    }
                }
            } else if key.eq_ignore_ascii_case("servicebinary") {
                let expanded = document.expand(value);
                if !expanded.is_empty() {
                    service_binaries.insert(expanded);
                }
            }
        }
    }

    let is_network_class = class
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("net"))
        || class_guid.as_deref().is_some_and(|value| {
            value
                .trim_matches(|c| c == '{' || c == '}')
                .eq_ignore_ascii_case(NETWORK_CLASS_GUID)
        });

    WindowsInfMetadata {
        class,
        class_guid,
        provider,
        driver_date,
        driver_version,
        catalog_files: catalog_files.into_iter().collect(),
        architectures: architectures.into_iter().collect(),
        hardware_ids: hardware_ids.into_iter().collect(),
        compatible_ids: compatible_ids.into_iter().collect(),
        service_names: service_names.into_iter().collect(),
        service_binaries: service_binaries.into_iter().collect(),
        is_network_class,
    }
}

fn decode_inf_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let words = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&words);
    }

    if bytes.starts_with(&[0xFE, 0xFF]) {
        let words = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16_lossy(&words);
    }

    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec()).unwrap_or_else(|error| {
        String::from_utf8_lossy(error.as_bytes()).into_owned()
    })
}

impl InfDocument {
    fn parse(text: &str) -> Self {
        let mut document = Self::default();
        let logical_lines = logical_lines(text);
        let mut current_section = String::new();

        for line in logical_lines {
            let line = strip_comment(&line).trim().to_string();
            if line.is_empty() {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                document
                    .sections
                    .entry(current_section.clone())
                    .or_default();
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let key = key.trim().to_string();
            let value = value.trim().to_string();
            document
                .sections
                .entry(current_section.clone())
                .or_default()
                .push((key.clone(), value.clone()));

            if current_section == "strings" || current_section.starts_with("strings.") {
                document
                    .strings
                    .insert(key.to_ascii_lowercase(), trim_quotes(&value).to_string());
            }
        }

        document
    }

    fn section(&self, name: &str) -> Option<&Vec<(String, String)>> {
        self.sections.get(&name.to_ascii_lowercase())
    }

    fn expand(&self, input: &str) -> String {
        let mut value = trim_quotes(input).to_string();

        // INF string substitution is %Token%. Resolve a few rounds to support
        // tokens that reference another token without risking cycles.
        for _ in 0..8 {
            let mut output = String::with_capacity(value.len());
            let mut rest = value.as_str();
            let mut changed = false;

            while let Some(start) = rest.find('%') {
                output.push_str(&rest[..start]);
                let token_start = start + 1;
                let Some(end_rel) = rest[token_start..].find('%') else {
                    output.push_str(&rest[start..]);
                    rest = "";
                    break;
                };
                let end = token_start + end_rel;
                let token = &rest[token_start..end];
                if let Some(replacement) = self.strings.get(&token.to_ascii_lowercase()) {
                    output.push_str(replacement);
                    changed = true;
                } else {
                    output.push_str(&rest[start..=end]);
                }
                rest = &rest[end + 1..];
            }
            output.push_str(rest);

            value = output;
            if !changed {
                break;
            }
        }

        trim_quotes(value.trim()).to_string()
    }
}

fn first_value(items: &[(String, String)], wanted: &str) -> Option<String> {
    items
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
        .map(|(_, value)| value.clone())
}

fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for physical in text.lines() {
        let trimmed = physical.trim_end();
        let continued = trimmed.ends_with('\\');
        let piece = if continued {
            &trimmed[..trimmed.len().saturating_sub(1)]
        } else {
            trimmed
        };

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(piece);

        if !continued {
            lines.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ';' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

fn trim_quotes(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn split_csv(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for ch in value.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                result.push(trim_quotes(current.trim()).to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    result.push(trim_quotes(current.trim()).to_string());
    result
}

fn architecture_from_decoration(value: &str) -> Option<&'static str> {
    let lower = value.to_ascii_lowercase();
    if lower.contains("ntamd64") {
        Some("x64")
    } else if lower.contains("ntarm64") {
        Some("arm64")
    } else if lower.contains("ntx86") {
        Some("x86")
    } else {
        None
    }
}

fn looks_like_device_id(value: &str) -> bool {
    let normalized = value.trim().to_ascii_uppercase().replace('/', "\\");
    normalized.starts_with("PCI\\")
        || normalized.starts_with("USB\\")
        || normalized.starts_with("VMBUS\\")
        || normalized.starts_with("ROOT\\")
        || normalized.starts_with("ACPI\\")
}

pub fn normalize_device_id(value: &str) -> String {
    trim_quotes(value)
        .trim()
        .replace('/', "\\")
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_models_services_and_strings() {
        let inf = r#"
[Version]
Signature="$Windows NT$"
Class=Net
ClassGuid={4d36e972-e325-11ce-bfc1-08002be10318}
Provider=%Provider%
DriverVer=09/01/2026,12.3.4.5
CatalogFile.NTamd64=netdemo.cat

[Manufacturer]
%Provider%=Models,NTamd64.10.0

[Models.NTamd64.10.0]
%Device%=Install, PCI\VEN_8086&DEV_15F3, PCI\VEN_8086&CC_0200

[Install.NT.Services]
AddService=e2fexpress,2,ServiceInstall

[ServiceInstall]
ServiceBinary=%12%\e2fexpress.sys

[Strings]
Provider="Example Networks"
Device="Example Ethernet"
"#;

        let parsed = parse_inf_text(inf);
        assert!(parsed.is_network_class);
        assert_eq!(parsed.provider.as_deref(), Some("Example Networks"));
        assert_eq!(parsed.driver_version.as_deref(), Some("12.3.4.5"));
        assert_eq!(parsed.architectures, vec!["x64"]);
        assert_eq!(parsed.hardware_ids, vec!["PCI\\VEN_8086&DEV_15F3"]);
        assert_eq!(parsed.compatible_ids, vec!["PCI\\VEN_8086&CC_0200"]);
        assert_eq!(parsed.service_names, vec!["e2fexpress"]);
        assert_eq!(parsed.catalog_files, vec!["netdemo.cat"]);
    }

    #[test]
    fn decodes_utf16le_inf() {
        let source = "[Version]\r\nClass=Net\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for word in source.encode_utf16() {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        assert_eq!(decode_inf_bytes(&bytes), source);
    }

    #[test]
    fn hardware_id_is_not_the_whole_model_rhs() {
        let parsed = parse_inf_text(
            "[Version]\nClass=Net\n[Manufacturer]\nM=M,NTamd64\n[M.NTamd64]\nD=Install, PCI\\VEN_10EC&DEV_8168\n",
        );
        assert_eq!(parsed.hardware_ids, vec!["PCI\\VEN_10EC&DEV_8168"]);
    }
}
