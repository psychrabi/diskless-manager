#[path = "../../dhcp.rs"]
mod implementation;
mod isc_dhcp;

pub use implementation::*;
pub use isc_dhcp::{BootReservation, BootReservationPublisher, IscDhcpPublisher};
