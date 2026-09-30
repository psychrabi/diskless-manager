pub mod driver_injection;
pub mod driver_manifest;
pub mod driver_selection;
pub mod driver_validation;
mod ipxe;
pub mod nvmeof;
pub mod windows_boot_arming;
pub mod windows_driver_injection;
pub mod windows_inf;
pub mod windows_image_preparation;
pub mod windows_remote_servicing;

pub use driver_injection::{
    DriverInjectionStatus, NetworkDriverInjectionPlugin, NetworkDriverPackage,
};
pub use driver_manifest::{DriverManifest, DriverManifestEntry};
pub use driver_selection::{select_drivers, NetworkDriverSelectorInput, SelectedNetworkDriver};
pub use driver_validation::{validate_package, DriverInfInspection, DriverPackageValidation};
pub use ipxe::*;
pub use nvmeof::*;
pub use windows_boot_arming::{
    build_plan as build_windows_boot_arm_plan, hive_is_dirty, RegistryArmChange,
    RegistryArmValue, WindowsBootArmConfig, WindowsBootArmPlan, WindowsBootArmResult,
    WindowsBootArmer, WindowsBootInventory,
};
pub use windows_driver_injection::{
    WindowsDriverInjectionRequest, WindowsDriverInjectionResult, WindowsDriverInjector,
};
pub use windows_inf::{inspect_inf_file, parse_inf, WindowsInfMetadata};
pub use windows_image_preparation::{
    servicing_available as windows_servicing_available, WindowsImagePreparationRequest,
    WindowsImagePreparationResult, WindowsImagePreparer,
};

pub use windows_remote_servicing::{
    RemoteWindowsCapabilitiesRequest, RemoteWindowsServicer, RemoteWindowsServicingCapabilities,
    RemoteWindowsServicingRequest,
};
