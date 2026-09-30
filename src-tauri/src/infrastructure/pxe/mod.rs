pub mod driver_injection;
pub mod driver_manifest;
pub mod driver_selection;
pub mod driver_validation;
mod ipxe;
pub mod nvmeof;
pub mod windows_boot_arming;
pub mod windows_driver_injection;
pub mod windows_inf;

pub use driver_injection::{
    DriverInjectionStatus, NetworkDriverInjectionPlugin, NetworkDriverPackage,
};
pub use driver_manifest::{DriverManifest, DriverManifestEntry};
pub use driver_selection::{select_drivers, NetworkDriverSelectorInput, SelectedNetworkDriver};
pub use driver_validation::{inspect_inf, validate_package, DriverInfInspection, DriverPackageValidation};
pub use ipxe::*;
pub use nvmeof::*;
pub use windows_boot_arming::{
    WindowsBootArmPlan, WindowsBootArmReport, WindowsBootArmer, WindowsBootArmingOptions,
};
pub use windows_driver_injection::{
    WindowsDriverInjectionRequest, WindowsDriverInjectionResult, WindowsDriverInjector,
};
pub use windows_inf::{normalize_device_id, parse_inf_file, parse_inf_text, read_inf_text, WindowsInfMetadata};
