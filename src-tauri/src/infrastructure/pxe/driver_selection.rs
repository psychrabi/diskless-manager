//! Deterministic network-driver selection for PXE clients.

use super::NetworkDriverPackage;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NetworkDriverSelectorInput {
    #[serde(default)]
    pub mac_address: Option<String>,
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

pub fn select_drivers(
    packages: &[NetworkDriverPackage],
    input: &NetworkDriverSelectorInput,
) -> Vec<SelectedNetworkDriver> {
    let mac = normalize_mac(input.mac_address.as_deref());
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
            let mut score = 0;
            let mut reasons = Vec::new();

            if explicit.contains(package.id.as_str()) {
                score += 1000;
                reasons.push("explicit driver selection".to_string());
            }

            if let Some(package_mac) = package.mac_address.as_deref() {
                if !mac.is_empty() && normalize_mac(Some(package_mac)) == mac {
                    score += 500;
                    reasons.push("MAC address match".to_string());
                }
            }

            if let Some(package_pnp) = package.pnp_device_id.as_deref() {
                if !pnp_ids.is_empty() && pnp_ids.contains(&normalize_pnp(package_pnp)) {
                    score += 900;
                    reasons.push("PNP device ID match".to_string());
                }
            }

            if let Some(service) = package.service_name.as_deref() {
                if services.contains(&service.to_ascii_lowercase()) {
                    score += 400;
                    reasons.push("driver service match".to_string());
                }
            }

            if score > 0 {
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

    selected.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.driver_id.cmp(&b.driver_id)));
    selected
}

fn normalize_mac(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .flat_map(|character| character.to_ascii_lowercase().to_string().chars().collect::<Vec<_>>())
        .collect()
}

fn normalize_pnp(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn package(id: &str, pnp: Option<&str>, mac: Option<&str>, service: Option<&str>) -> NetworkDriverPackage {
        NetworkDriverPackage {
            id: id.to_string(),
            name: id.to_string(),
            service_name: service.map(str::to_string),
            driver_name: None,
            pnp_device_id: pnp.map(str::to_string),
            guid: None,
            mac_address: mac.map(str::to_string),
            inf_files: vec!["driver.inf".to_string()],
            imported_at: Utc::now(),
        }
    }

    #[test]
    fn pnp_match_wins_over_service_match() {
        let packages = vec![
            package("generic", None, None, Some("e2fexpress")),
            package("intel", Some("PCI\\VEN_8086&DEV_15F3"), None, None),
        ];
        let input = NetworkDriverSelectorInput {
            pnp_device_ids: vec!["PCI\\VEN_8086&DEV_15F3".to_string()],
            service_names: vec!["e2fexpress".to_string()],
            ..Default::default()
        };

        let selected = select_drivers(&packages, &input);
        assert_eq!(selected[0].driver_id, "intel");
    }

    #[test]
    fn explicit_selection_is_supported() {
        let packages = vec![package("one", None, None, None), package("two", None, None, None)];
        let input = NetworkDriverSelectorInput {
            driver_ids: vec!["two".to_string()],
            ..Default::default()
        };
        let selected = select_drivers(&packages, &input);
        assert_eq!(selected[0].driver_id, "two");
        assert_eq!(selected[0].score, 1000);
    }
}
