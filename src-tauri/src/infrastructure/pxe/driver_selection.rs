//! Deterministic network-driver selection for PXE clients.
//!
//! Exact hardware-ID matches are preferred over compatible IDs. Architecture is
//! a hard compatibility filter when both the client and package declare it.

use super::NetworkDriverPackage;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NetworkDriverSelectorInput {
    #[serde(default)]
    pub mac_address: Option<String>,
    #[serde(default)]
    pub architecture: Option<String>,
    #[serde(default)]
    pub pnp_device_ids: Vec<String>,
    #[serde(default)]
    pub service_names: Vec<String>,
    #[serde(default)]
    pub driver_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectedNetworkDriver {
    pub driver_id: String,
    pub score: u32,
    pub reasons: Vec<String>,
}

/// Target image/client description for automatic Windows revision builds.
///
/// Only signed catalog packages whose hardware IDs match a Boot NIC are
/// eligible; OS/architecture mismatches and unsigned packages are omitted so a
/// damaged or incompatible package never reaches a published revision.
#[derive(Debug, Clone, Default)]
pub struct ImageDriverTarget {
    pub os_version: Option<String>,
    pub architecture: Option<String>,
    pub boot_nic_ids: Vec<String>,
}

/// Normalize caller-supplied PnP hardware IDs: trimmed, `/` treated as `\\`,
/// bounded length and charset. Malformed IDs are rejected, never stored.
pub fn normalize_hardware_ids(ids: &[String]) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for id in ids {
        let clean = id.trim().trim_matches('"').replace('/', "\\");
        if clean.is_empty() || clean.len() > 128 {
            return Err(format!("invalid hardware ID: '{id}'"));
        }
        if !clean.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '\\' | '&' | '_' | '-' | '.' | '+' | '(' | ')' | '{' | '}' | ','
                )
        }) {
            return Err(format!("invalid hardware ID: '{id}'"));
        }
        if !normalized.contains(&clean) {
            normalized.push(clean);
        }
    }
    Ok(normalized)
}

/// Select the compatible signed driver set for a Windows image revision.
///
/// Deterministic: exact hardware-ID matches first, then compatible IDs, with
/// stable package-ID tie breaking. Each package is injected at most once.
pub fn select_compatible_for_image(
    packages: &[NetworkDriverPackage],
    target: &ImageDriverTarget,
) -> Vec<SelectedNetworkDriver> {
    let eligible: Vec<NetworkDriverPackage> = packages
        .iter()
        .filter(|package| {
            // Signed Windows packages must carry catalog files; unsigned or
            // damaged packages are never eligible for a published revision.
            if package.catalog_files.is_empty() {
                return false;
            }
            if let Some(os) = target.os_version.as_deref() {
                let os = os.to_ascii_lowercase();
                if let Some(version) = package.version.as_deref() {
                    let version = version.to_ascii_lowercase();
                    // A package declaring a different Windows version is
                    // incompatible; packages without version info pass through
                    // and are gated by DISM signature verification at install.
                    if !version.is_empty()
                        && !os.is_empty()
                        && !version.contains(&os)
                        && !os.contains(&version)
                    {
                        return false;
                    }
                }
            }
            true
        })
        .cloned()
        .collect();
    let input = NetworkDriverSelectorInput {
        architecture: target.architecture.clone(),
        pnp_device_ids: target.boot_nic_ids.clone(),
        ..Default::default()
    };
    let mut selected = select_drivers(&eligible, &input);
    // Deduplicate by package ID while preserving deterministic score order.
    let mut seen = HashSet::new();
    selected.retain(|entry| seen.insert(entry.driver_id.clone()));
    selected
}

pub fn select_drivers(
    packages: &[NetworkDriverPackage],
    input: &NetworkDriverSelectorInput,
) -> Vec<SelectedNetworkDriver> {
    let mac = normalize_mac(input.mac_address.as_deref());
    let architecture = input
        .architecture
        .as_deref()
        .map(normalize_architecture)
        .filter(|value| !value.is_empty());
    let pnp_ids = input
        .pnp_device_ids
        .iter()
        .map(|value| normalize_pnp(value))
        .collect::<HashSet<_>>();
    let services = input
        .service_names
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let explicit = input
        .driver_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    let mut selected = packages
        .iter()
        .filter_map(|package| {
            if let Some(requested_arch) = architecture.as_deref() {
                if !package.architectures.is_empty()
                    && !package
                        .architectures
                        .iter()
                        .map(|value| normalize_architecture(value))
                        .any(|value| value == requested_arch)
                {
                    return None;
                }
            }

            let mut score = 0;
            let mut reasons = Vec::new();
            let mut identity_match = false;

            if explicit.contains(package.id.as_str()) {
                score += 10_000;
                identity_match = true;
                reasons.push("explicit driver selection".to_string());
            }

            let exact_ids = package
                .hardware_ids
                .iter()
                .map(|value| normalize_pnp(value))
                .chain(
                    package
                        .pnp_device_id
                        .iter()
                        .map(|value| normalize_pnp(value)),
                )
                .collect::<HashSet<_>>();
            if !pnp_ids.is_empty() && exact_ids.iter().any(|id| pnp_ids.contains(id)) {
                score += 9_000;
                identity_match = true;
                reasons.push("exact PNP hardware ID match".to_string());
            }

            let compatible_ids = package
                .compatible_ids
                .iter()
                .map(|value| normalize_pnp(value))
                .collect::<HashSet<_>>();
            if !pnp_ids.is_empty() && compatible_ids.iter().any(|id| pnp_ids.contains(id)) {
                score += 6_000;
                identity_match = true;
                reasons.push("compatible PNP ID match".to_string());
            }

            if let Some(package_mac) = package.mac_address.as_deref() {
                if !mac.is_empty() && normalize_mac(Some(package_mac)) == mac {
                    score += 2_000;
                    identity_match = true;
                    reasons.push("MAC address match".to_string());
                }
            }

            let package_services = package
                .service_names
                .iter()
                .map(|value| value.to_ascii_lowercase())
                .chain(
                    package
                        .service_name
                        .iter()
                        .map(|value| value.to_ascii_lowercase()),
                )
                .collect::<HashSet<_>>();
            if package_services
                .iter()
                .any(|service| services.contains(service))
            {
                score += 1_000;
                identity_match = true;
                reasons.push("driver service match".to_string());
            }

            if let Some(requested_arch) = architecture.as_deref() {
                if package
                    .architectures
                    .iter()
                    .map(|value| normalize_architecture(value))
                    .any(|value| value == requested_arch)
                {
                    score += 100;
                    reasons.push(format!("{} architecture match", requested_arch));
                }
            }

            if identity_match {
                Some(SelectedNetworkDriver {
                    driver_id: package.id.clone(),
                    score,
                    reasons,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    selected.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.driver_id.cmp(&b.driver_id))
    });
    selected
}

fn normalize_mac(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn normalize_pnp(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .to_ascii_lowercase()
        .replace('/', "\\")
}

fn normalize_architecture(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "amd64" | "x86_64" | "x64" => "x64".to_string(),
        "i386" | "i686" | "x86" => "x86".to_string(),
        "aarch64" | "arm64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn package(
        id: &str,
        pnp: Option<&str>,
        mac: Option<&str>,
        service: Option<&str>,
        architecture: Option<&str>,
    ) -> NetworkDriverPackage {
        NetworkDriverPackage {
            id: id.to_string(),
            name: id.to_string(),
            service_name: service.map(str::to_string),
            driver_name: None,
            pnp_device_id: pnp.map(str::to_string),
            guid: None,
            mac_address: mac.map(str::to_string),
            inf_files: vec!["driver.inf".to_string()],
            provider: None,
            version: None,
            architectures: architecture.into_iter().map(str::to_string).collect(),
            hardware_ids: pnp.into_iter().map(str::to_string).collect(),
            compatible_ids: Vec::new(),
            service_names: service.into_iter().map(str::to_string).collect(),
            catalog_files: Vec::new(),
            imported_at: Utc::now(),
        }
    }

    #[test]
    fn pnp_match_wins_over_service_match() {
        let packages = vec![
            package("generic", None, None, Some("e2fexpress"), Some("x64")),
            package(
                "intel",
                Some("PCI\\VEN_8086&DEV_15F3"),
                None,
                None,
                Some("x64"),
            ),
        ];
        let input = NetworkDriverSelectorInput {
            architecture: Some("amd64".to_string()),
            pnp_device_ids: vec!["PCI\\VEN_8086&DEV_15F3".to_string()],
            service_names: vec!["e2fexpress".to_string()],
            ..Default::default()
        };

        let selected = select_drivers(&packages, &input);
        assert_eq!(selected[0].driver_id, "intel");
    }

    #[test]
    fn explicit_selection_is_supported() {
        let packages = vec![
            package("one", None, None, None, None),
            package("two", None, None, None, None),
        ];
        let input = NetworkDriverSelectorInput {
            driver_ids: vec!["two".to_string()],
            ..Default::default()
        };
        let selected = select_drivers(&packages, &input);
        assert_eq!(selected[0].driver_id, "two");
        assert_eq!(selected[0].score, 10_000);
    }

    #[test]
    fn mismatched_architecture_is_rejected() {
        let packages = vec![package(
            "x86-only",
            Some("PCI\\VEN_1234&DEV_5678"),
            None,
            None,
            Some("x86"),
        )];
        let input = NetworkDriverSelectorInput {
            architecture: Some("x64".to_string()),
            pnp_device_ids: vec!["PCI\\VEN_1234&DEV_5678".to_string()],
            ..Default::default()
        };
        assert!(select_drivers(&packages, &input).is_empty());
    }

    #[test]
    fn architecture_match_does_not_select_an_unrelated_package() {
        let packages = vec![package(
            "unrelated-x64",
            Some("PCI\\VEN_1234&DEV_5678"),
            None,
            None,
            Some("x64"),
        )];
        let input = NetworkDriverSelectorInput {
            architecture: Some("x64".to_string()),
            pnp_device_ids: vec!["PCI\\VEN_ABCD&DEV_EF01".to_string()],
            ..Default::default()
        };

        assert!(select_drivers(&packages, &input).is_empty());
    }

    fn signed_package(id: &str, pnp: &str, arch: &str) -> NetworkDriverPackage {
        let mut pkg = package(id, Some(pnp), None, None, Some(arch));
        pkg.catalog_files = vec!["driver.cat".to_string()];
        pkg
    }

    #[test]
    fn image_selection_omits_unsigned_incompatible_and_dedupes() {
        let packages = vec![
            signed_package("intel", "PCI\\VEN_8086&DEV_15F3", "x64"),
            package(
                "unsigned",
                Some("PCI\\VEN_8086&DEV_15F3"),
                None,
                None,
                Some("x64"),
            ),
            signed_package("x86-only", "PCI\\VEN_8086&DEV_15F3", "x86"),
            signed_package("other", "PCI\\VEN_ABCD&DEV_EF01", "x64"),
        ];
        let target = ImageDriverTarget {
            architecture: Some("x64".to_string()),
            boot_nic_ids: vec!["PCI\\VEN_8086&DEV_15F3".to_string()],
            ..Default::default()
        };
        let selected = select_compatible_for_image(&packages, &target);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].driver_id, "intel");
    }
}
