use crate::error::AppError;
use std::process::Command;

// Helper: Launch VNC viewer
pub fn launch_vnc_viewer(client_ip: &str) -> Result<(), AppError> {
    // Try generic vncviewer
    let vnc_command = ["vncviewer", client_ip];

    Command::new(vnc_command[0])
        .arg(vnc_command[1])
        .spawn()
        .map_err(|e| AppError::Command(format!("Failed to launch vncviewer: {}", e)))?;

    Ok(())
}

// Helper: Launch xfreerdp
pub fn launch_remote_desktop(client_ip: &str, username: &str) -> Result<(), AppError> {
    let rdp_command = [
        "xfreerdp3",
        &format!("/v:{}", client_ip),
        &format!("/u:{}", username),
        "/p:1",
        "/cert:ignore",
        "/w:1920",
        "/h:1080",
        "/dynamic-resolution",
        "/gdi:hw",
        "/network:lan",
        "/bpp:32",
        "/sec:nla",
        "/timeout:20000",
    ];

    Command::new(rdp_command[0])
        .args(&rdp_command[1..])
        .spawn()
        .map_err(|e| AppError::Command(format!("Failed to launch xfreerdp: {}", e)))?;

    Ok(())
}
