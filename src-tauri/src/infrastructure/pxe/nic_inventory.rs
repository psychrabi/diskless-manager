//! Preboot NIC inventory collected in WinPE before Windows starts.
//!
//! A new client boots a small WinPE image that reports the Boot NIC's PnP
//! hardware IDs keyed by the observed MAC address. The record stays pending
//! until the client is assigned: MAC is only the lookup key, and matching
//! always uses hardware IDs, never the MAC alone.

use super::normalize_hardware_ids;
use serde::{Deserialize, Serialize};

/// Firmware boot interface reported from WinPE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FirmwareInterface {
    Uefi,
    Bios,
    Unknown,
}

impl std::fmt::Display for FirmwareInterface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uefi => write!(f, "uefi"),
            Self::Bios => write!(f, "bios"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for FirmwareInterface {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "uefi" => Ok(Self::Uefi),
            "bios" | "legacy" => Ok(Self::Bios),
            "" | "unknown" => Ok(Self::Unknown),
            _ => Err(anyhow::anyhow!("invalid firmware interface: {}", value)),
        }
    }
}

/// Raw inventory payload posted by the WinPE script.
#[derive(Debug, Clone, Deserialize)]
pub struct NicInventoryPayload {
    #[serde(default)]
    pub boot_nic_ids: Vec<String>,
    #[serde(default)]
    pub secondary_ids: Vec<String>,
    #[serde(default)]
    pub architecture: Option<String>,
    #[serde(default)]
    pub firmware: Option<String>,
}

/// Validated inventory ready for storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedNicInventory {
    pub boot_nic_ids: Vec<String>,
    pub secondary_ids: Vec<String>,
    pub architecture: String,
    pub firmware: FirmwareInterface,
}

/// Normalize and validate an inventory payload.
///
/// At least one Boot NIC hardware ID is required; secondary adapters are
/// recorded as non-boot and never satisfy the Boot NIC requirement. Bounds
/// keep a misbehaving reporter from bloating the table.
pub fn parse_inventory_payload(
    payload: &NicInventoryPayload,
) -> anyhow::Result<ValidatedNicInventory> {
    if payload.boot_nic_ids.len() > 16 {
        anyhow::bail!("at most 16 Boot NIC hardware IDs are accepted");
    }
    if payload.secondary_ids.len() > 64 {
        anyhow::bail!("at most 64 secondary hardware IDs are accepted");
    }
    let boot_nic_ids = normalize_hardware_ids(&payload.boot_nic_ids)
        .map_err(|message| anyhow::anyhow!("{message}"))?;
    if boot_nic_ids.is_empty() {
        anyhow::bail!("at least one Boot NIC hardware ID is required");
    }
    let secondary_ids = normalize_hardware_ids(&payload.secondary_ids)
        .map_err(|message| anyhow::anyhow!("{message}"))?;
    let secondary_ids = secondary_ids
        .into_iter()
        .filter(|id| !boot_nic_ids.contains(id))
        .collect();
    let architecture = payload
        .architecture
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    if architecture.len() > 32 {
        anyhow::bail!("architecture value is too long");
    }
    let firmware = payload
        .firmware
        .as_deref()
        .unwrap_or_default()
        .parse::<FirmwareInterface>()?;
    Ok(ValidatedNicInventory {
        boot_nic_ids,
        secondary_ids,
        architecture,
        firmware,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(boot: &[&str], secondary: &[&str]) -> NicInventoryPayload {
        NicInventoryPayload {
            boot_nic_ids: boot.iter().map(|value| value.to_string()).collect(),
            secondary_ids: secondary.iter().map(|value| value.to_string()).collect(),
            architecture: Some("AMD64".to_string()),
            firmware: Some("UEFI".to_string()),
        }
    }

    #[test]
    fn multiple_adapters_parse_with_boot_and_secondary_split() {
        let validated = parse_inventory_payload(&payload(
            &["PCI\\VEN_8086&DEV_15F3"],
            &["PCI\\VEN_10EC&DEV_8168", "PCI\\VEN_8086&DEV_15F3"],
        ))
        .unwrap();
        assert_eq!(validated.boot_nic_ids, vec!["PCI\\VEN_8086&DEV_15F3"]);
        // A secondary ID duplicating the Boot NIC is not stored twice.
        assert_eq!(validated.secondary_ids, vec!["PCI\\VEN_10EC&DEV_8168"]);
        assert_eq!(validated.firmware, FirmwareInterface::Uefi);
    }

    #[test]
    fn missing_boot_nic_is_rejected() {
        assert!(parse_inventory_payload(&payload(&[], &["PCI\\VEN_10EC&DEV_8168"])).is_err());
    }

    #[test]
    fn malformed_ids_are_rejected() {
        assert!(parse_inventory_payload(&payload(&[""], &[])).is_err());
        assert!(parse_inventory_payload(&payload(&["id with spaces;"], &[])).is_err());
        assert!(
            parse_inventory_payload(&payload(&["PCI\\VEN_8086&DEV_15F3"], &["bad;id"])).is_err()
        );
    }

    #[test]
    fn secondary_adapter_never_satisfies_the_boot_requirement() {
        let payload = NicInventoryPayload {
            boot_nic_ids: Vec::new(),
            secondary_ids: vec!["PCI\\VEN_10EC&DEV_8168".to_string()],
            architecture: None,
            firmware: None,
        };
        let error = parse_inventory_payload(&payload).unwrap_err();
        assert!(error.to_string().contains("Boot NIC"), "{error:#}");
    }

    #[test]
    fn oversized_payloads_are_rejected() {
        let many: Vec<String> = (0..17).map(|index| format!("ID{index}")).collect();
        assert!(parse_inventory_payload(&NicInventoryPayload {
            boot_nic_ids: many,
            secondary_ids: Vec::new(),
            architecture: None,
            firmware: None,
        })
        .is_err());
    }
}
