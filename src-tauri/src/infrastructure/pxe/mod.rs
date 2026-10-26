pub mod driver_injection;
pub mod driver_manifest;
pub mod driver_selection;
pub mod driver_validation;
pub mod installation;
mod ipxe;
pub mod nic_inventory;
pub mod nvmeof;
pub mod windows_boot_arming;
pub mod windows_driver_injection;
pub mod windows_image_preparation;
pub mod windows_inf;
pub mod windows_iscsi_servicing;
pub mod windows_nexboot_adaptation;
pub mod windows_remote_servicing;

pub use driver_injection::{
    DriverInjectionStatus, NetworkDriverInjectionPlugin, NetworkDriverPackage,
};
pub use driver_manifest::{DriverManifest, DriverManifestEntry};
pub use driver_selection::{
    normalize_hardware_ids, pnp_id_matches, select_compatible_for_image, select_drivers,
    select_managed_image_driver_ids, ImageDriverTarget, NetworkDriverSelectorInput,
    SelectedNetworkDriver,
};
pub use driver_validation::{validate_package, DriverInfInspection, DriverPackageValidation};
pub use ipxe::*;
pub use nic_inventory::{
    parse_inventory_payload, FirmwareInterface, NicInventoryPayload, ValidatedNicInventory,
};
pub use nvmeof::*;
pub use windows_boot_arming::{
    build_plan as build_windows_boot_arm_plan, hive_is_dirty, RegistryArmChange, RegistryArmValue,
    WindowsBootArmConfig, WindowsBootArmPlan, WindowsBootArmResult, WindowsBootArmer,
    WindowsBootInventory,
};
pub use windows_driver_injection::{
    WindowsDriverInjectionRequest, WindowsDriverInjectionResult, WindowsDriverInjector,
};
pub use windows_image_preparation::{
    servicing_available as windows_servicing_available, WindowsDriverInstallStrategy,
    WindowsImagePreparationRequest, WindowsImagePreparationResult, WindowsImagePreparer,
};
pub use windows_inf::{inspect_inf_file, parse_inf, WindowsInfMetadata};

pub use windows_remote_servicing::{
    RemoteWindowsCapabilitiesRequest, RemoteWindowsCatalogPreparationResult, RemoteWindowsServicer,
    RemoteWindowsServicingCapabilities, RemoteWindowsServicingRequest, StagedDriverPackage,
};

pub mod windows_volume_safety;
