//! Deterministic network-driver selection for PXE clients.
//!
//! Exact hardware-ID matches are preferred over compatible IDs. Architecture is
//! a hard compatibility filter when both the client and package declare it.

use super::NetworkDriverPackage;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};

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
/// Only catalog-backed packages whose hardware IDs and architecture match a
/// Boot NIC are eligible; Windows/DISM verifies package signatures at install.
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

/// Return signed catalog packages for one Boot NIC's alternative hardware and
/// compatible IDs, plus any packages explicitly selected by an administrator.
pub fn select_managed_image_driver_ids(
    packages: &[NetworkDriverPackage],
    boot_nic_ids: &[String],
    architectures: &[String],
    explicit_ids: &[String],
) -> Result<Vec<String>, String> {
    let known_architectures = architectures
        .iter()
        .map(|architecture| normalize_architecture(architecture))
        .filter(|architecture| !architecture.is_empty())
        .collect::<BTreeSet<_>>();
    let architecture = (known_architectures.len() == 1)
        .then(|| known_architectures.first().cloned())
        .flatten();
    let target = ImageDriverTarget {
        architecture,
        boot_nic_ids: boot_nic_ids.to_vec(),
        ..Default::default()
    };
    let mut ids = select_compatible_for_image(packages, &target)
        .into_iter()
        .map(|selected| selected.driver_id)
        .collect::<BTreeSet<_>>();
    if known_architectures.len() != 1 {
        ids.retain(|id| {
            packages
                .iter()
                .any(|package| package.id == *id && package.architectures.is_empty())
        });
    }
    // WinPE reports alternative IDs for the same adapter, not separate devices.
    if !boot_nic_ids.is_empty() && ids.is_empty() {
        return Err(format!(
            "no compatible signed imported driver covers Boot NIC hardware ID(s): {}",
            boot_nic_ids.join(", ")
        ));
    }
    ids.extend(explicit_ids.iter().cloned());
    if ids.is_empty() {
        return Err("no imported network-driver package matches this client inventory; import a compatible package or select one explicitly".to_string());
    }
    Ok(ids.into_iter().collect())
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
                .chain(package.pnp_device_id.iter())
                .any(|id| {
                    input
                        .pnp_device_ids
                        .iter()
                        .any(|observed| pnp_id_matches(id, observed))
                });
            if !input.pnp_device_ids.is_empty() && exact_ids {
                score += 9_000;
                identity_match = true;
                reasons.push("exact PNP hardware ID match".to_string());
            }

            let compatible_match = package.compatible_ids.iter().any(|id| {
                input
                    .pnp_device_ids
                    .iter()
                    .any(|observed| pnp_id_matches(id, observed))
            });
            if !input.pnp_device_ids.is_empty() && compatible_match {
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

/// Match INF HardwareIds/CompatibleIds against either those IDs or a device
/// InstanceId whose final path component is the instance-specific suffix.
pub fn pnp_id_matches(package_id: &str, observed_id: &str) -> bool {
    let package_id = normalize_pnp(package_id);
    let observed_id = normalize_pnp(observed_id);
    package_id == observed_id
        || observed_id
            .rsplit_once('\\')
            .is_some_and(|(hardware_id, _)| package_id == hardware_id)
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

    #[test]
    fn inf_hardware_id_matches_device_instance_id_with_suffix() {
        let package = signed_package("intel", "PCI\\VEN_8086&DEV_15F3", "x64");
        let input = NetworkDriverSelectorInput {
            architecture: Some("x64".to_string()),
            pnp_device_ids: vec!["PCI\\VEN_8086&DEV_15F3\\3&11583659&0&FE".to_string()],
            ..Default::default()
        };

        let selected = select_drivers(&[package], &input);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].driver_id, "intel");
    }

    #[test]
    fn image_selection_does_not_compare_driverver_to_windows_version() {
        let mut nic = signed_package("intel", "PCI\\VEN_8086&DEV_15F3", "x64");
        nic.version = Some("08/01/2026,12.3.4.5".to_string());
        let target = ImageDriverTarget {
            os_version: Some("Windows 11 24H2".to_string()),
            architecture: Some("x64".to_string()),
            boot_nic_ids: vec!["PCI\\VEN_8086&DEV_15F3".to_string()],
        };

        let selected = select_compatible_for_image(&[nic], &target);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].driver_id, "intel");
    }

    #[test]
    fn managed_image_selection_unions_all_boot_nics_and_explicit_packages() {
        let packages = vec![
            signed_package("first-nic", "PCI\\VEN_8086&DEV_15F3", "x64"),
            signed_package("second-nic", "PCI\\VEN_10EC&DEV_8168", "x64"),
            signed_package("manual", "PCI\\VEN_ABCD&DEV_0001", "x64"),
        ];
        let selected = select_managed_image_driver_ids(
            &packages,
            &[
                "PCI\\VEN_8086&DEV_15F3".to_string(),
                "PCI\\VEN_10EC&DEV_8168".to_string(),
            ],
            &["x64".to_string()],
            &["manual".to_string(), "first-nic".to_string()],
        )
        .unwrap();
        assert_eq!(selected, vec!["first-nic", "manual", "second-nic"]);
        assert!(select_managed_image_driver_ids(&packages, &[], &[], &[]).is_err());
        assert!(select_managed_image_driver_ids(
            &packages,
            &["PCI\\VEN_8086&DEV_15F3".to_string()],
            &[],
            &[],
        )
        .is_err());
        assert!(select_managed_image_driver_ids(
            &packages,
            &["PCI\\VEN_8086&DEV_15F3".to_string()],
            &["x64".to_string(), "x86".to_string()],
            &[],
        )
        .is_err());
    }
}
