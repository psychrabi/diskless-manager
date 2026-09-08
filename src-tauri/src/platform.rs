//! Cross-distribution platform handling (Debian-like, RedHat-like, Arch-like).
//!
//! diskless-manager historically targeted Debian/Ubuntu. This module detects the
//! running Linux distribution and exposes the package manager, service names,
//! and configuration paths that differ between the families so the rest of the
//! crate stays distribution-agnostic.

use std::path::Path;
use std::process::Command;

/// The package/service platform family of the running Linux distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distro {
    /// Debian, Ubuntu and derivatives: `apt`, `dpkg-query`, `isc-dhcp-server`,
    /// `tftpd-hpa`, `apache2`, netplan, ...
    Debian,
    /// Fedora, RHEL, CentOS, Rocky, Alma and derivatives: `dnf`, `rpm`,
    /// `dhcpd`, `tftpd`, `httpd`, NetworkManager/nmcli, ...
    RedHat,
    /// Arch Linux and derivatives: `pacman`, `dhcpd4`, `tftpd`, `httpd`,
    /// NetworkManager/nmcli, ...
    Arch,
}

impl Distro {
    pub fn is_debian(self) -> bool {
        self == Distro::Debian
    }

    pub fn is_redhat(self) -> bool {
        self == Distro::RedHat
    }

    /// Command + arguments used to install a package with elevated privileges.
    /// The caller is expected to prepend `sudo`.
    pub fn pkg_install<'a>(&self, package: &'a str) -> Vec<&'a str> {
        match self {
            Distro::Debian => vec!["apt-get", "install", "-y", package],
            Distro::RedHat => vec!["dnf", "install", "-y", package],
            Distro::Arch => vec!["pacman", "-S", "--noconfirm", package],
        }
    }

    /// Command + arguments used to query the installed version of a package.
    pub fn pkg_version<'a>(&self, package: &'a str) -> Vec<&'a str> {
        match self {
            Distro::Debian => vec!["--showformat=${Version}\n", "--show", package],
            Distro::RedHat => vec!["-q", "--queryformat=%{VERSION}", package],
            // pacman -Q prints "<name> <version>"; only the version is kept.
            Distro::Arch => vec!["-Q", package],
        }
    }

    // -------------------------------------------------------------------
    // systemd service names
    // -------------------------------------------------------------------

    pub fn dhcp_service(&self) -> &'static str {
        match self {
            Distro::Debian => "isc-dhcp-server",
            Distro::RedHat => "dhcpd",
            Distro::Arch => "dhcpd4",
        }
    }

    pub fn tftp_service(&self) -> &'static str {
        match self {
            Distro::Debian => "tftpd-hpa",
            // Fedora/RHEL's tftp-server package ships `tftp.service` (activated
            // over a tftp.socket) — there is no `tftpd.service` unit.
            Distro::RedHat => "tftp",
            Distro::Arch => "tftpd",
        }
    }

    pub fn http_service(&self) -> &'static str {
        match self {
            Distro::Debian => "apache2",
            Distro::RedHat => "httpd",
            Distro::Arch => "httpd",
        }
    }

    pub fn nfs_service(&self) -> &'static str {
        match self {
            Distro::Debian => "nfs-kernel-server",
            Distro::RedHat => "nfs-server",
            Distro::Arch => "nfs-server",
        }
    }

    pub fn iscsi_service(&self) -> &'static str {
        match self {
            Distro::Debian => "rtslib-fb-targetctl",
            Distro::RedHat => "target",
            Distro::Arch => "target",
        }
    }

    /// Samba offers smbd/nmbd on Debian but smb/nmb on Fedora/RHEL and Arch.
    pub fn samba_services(&self) -> &'static [&'static str] {
        match self {
            Distro::Debian => &["smbd", "nmbd"],
            Distro::RedHat | Distro::Arch => &["smb", "nmb"],
        }
    }

    // -------------------------------------------------------------------
    // configuration paths
    // -------------------------------------------------------------------

    pub fn dhcp_defaults_path(&self) -> &'static str {
        match self {
            Distro::Debian => "/etc/default/isc-dhcp-server",
            Distro::RedHat => "/etc/sysconfig/dhcpd",
            // Arch's dhcpd4.service has no defaults file; a systemd drop-in
            // overrides ExecStart to point at the manager-owned config and
            // pass the serving interfaces on the command line.
            Distro::Arch => "/etc/systemd/system/dhcpd4.service.d/diskless-manager.conf",
        }
    }

    pub fn tftp_defaults_path(&self) -> &'static str {
        match self {
            Distro::Debian => "/etc/default/tftpd-hpa",
            Distro::RedHat => "/etc/sysconfig/tftpd",
            // Arch's tftpd.service sources /etc/conf.d/tftpd via EnvironmentFile.
            Distro::Arch => "/etc/conf.d/tftpd",
        }
    }

    pub fn http_config_path(&self) -> &'static str {
        match self {
            Distro::Debian => "/etc/apache2/sites-available/diskless-server.conf",
            Distro::RedHat => "/etc/httpd/conf.d/diskless-server.conf",
            // Arch's httpd.conf ships with
            // "IncludeOptional conf/conf.d/*.conf".
            Distro::Arch => "/etc/httpd/conf/conf.d/diskless-server.conf",
        }
    }

    /// Apache/HTTPD log directory. Debian exports `$APACHE_LOG_DIR`,
    /// Fedora's httpd logs directly into `/var/log/httpd`.
    pub fn http_log_dir(&self) -> &'static str {
        match self {
            Distro::Debian => "${APACHE_LOG_DIR}",
            Distro::RedHat | Distro::Arch => "/var/log/httpd",
        }
    }

    // -------------------------------------------------------------------
    // privileged access (sudoers)
    // -------------------------------------------------------------------

    /// Absolute command paths that the diskless-manager sudoers file must grant
    /// passwordless access to.
    pub fn privileged_commands(&self) -> Vec<&'static str> {
        let mut commands = vec![
            "/usr/bin/systemctl",
            "/usr/sbin/zfs",
            "/usr/sbin/zpool",
            "/usr/bin/targetcli",
            "/usr/bin/tee",
            "/usr/bin/cat",
            "/usr/bin/mkdir",
            "/usr/bin/sync",
            "/usr/sbin/exportfs",
            "/usr/bin/journalctl",
            "/usr/bin/rm",
            "/usr/bin/mv",
            "/usr/bin/cp",
            "/usr/sbin/dhcpd",
            "/usr/sbin/restorecon",
            "/usr/sbin/semanage",
        ];

        match self {
            Distro::Debian => commands.append(&mut vec![
                "/usr/bin/apt-get",
                "/usr/sbin/a2ensite",
                "/usr/sbin/a2enmod",
                "/usr/sbin/netplan",
            ]),
            Distro::RedHat => commands.append(&mut vec!["/usr/bin/dnf", "/usr/bin/nmcli"]),
            Distro::Arch => commands.append(&mut vec!["/usr/bin/pacman", "/usr/bin/nmcli"]),
        }

        commands
    }
}

/// Detect the distribution family from well-known files. Files are preferred
/// over parsed version strings because they are stable across releases.
pub fn detect() -> Distro {
    if Path::new("/etc/debian_version").exists() {
        Distro::Debian
    } else if Path::new("/etc/redhat-release").exists() {
        Distro::RedHat
    } else if Path::new("/etc/arch-release").exists() || os_release_id_is_arch() {
        Distro::Arch
    } else {
        // Fall back to Debian for unknown distributions to preserve the
        // historical Debian-first behaviour.
        Distro::Debian
    }
}

/// Some Arch derivatives ship an empty `/etc/arch-release` but always set
/// `ID=arch` (or a derivative ID) in `/etc/os-release`. Re-read the file only
/// as a last resort so detection keeps working when the marker file is missing.
fn os_release_id_is_arch() -> bool {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .map(|content| {
            content.lines().any(|line| {
                let Some(id) = line.strip_prefix("ID=") else {
                    return false;
                };
                let id = id.trim().trim_matches('"');
                id == "arch" || id.starts_with("arch") || id == "endeavouros" || id == "cachyos"
            })
        })
        .unwrap_or(false)
}

/// Wake-on-LAN utility binary name. Fedora/RHEL ship the `wol` package (formerly
/// `wakeonlan`), while Debian and Arch still ship `wakeonlan`.
pub fn wol_binary() -> &'static str {
    match detect() {
        Distro::RedHat => "wol",
        Distro::Debian | Distro::Arch => "wakeonlan",
    }
}

/// Install a package using the distribution's package manager.
///
/// Records the operation in the application log (visible in the Logs page)
/// including the package manager command and a tail of the captured output so
/// administrators can audit dependency installation during setup.
pub async fn install_package(package: &str) -> Result<String, String> {
    let distro = detect();
    // OpenZFS is not shipped by the default Fedora/RHEL repositories; it needs
    // the official zfsonlinux repository and a matching kernel-devel for the
    // DKMS module build.
    if distro == Distro::RedHat && package == "zfs" {
        return install_redhat_zfs().await;
    }

    let args = distro.pkg_install(package);
    log::info!(
        "Dependency install: installing '{}' via sudo {}",
        package,
        args.join(" ")
    );

    let output = Command::new("sudo").args(&args).output().map_err(|e| {
        let message = format!("Failed to spawn {}: {}", args[0], e);
        log::error!("Dependency install '{}' failed to start: {}", package, e);
        message
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        log::info!("Dependency install: '{}' installed successfully", package);
        match package_version(distro, package) {
            Some(version) => log::info!(
                "Dependency install: '{}' is now version {}",
                package,
                version
            ),
            None => log::info!(
                "Dependency install: '{}' version could not be determined",
                package
            ),
        }
        log_install_output(package, &stdout, &stderr);
        Ok(format!("Package {} installed successfully", package))
    } else {
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        let message = format!("Failed to install {}: {}", package, detail);
        log::error!("{}", message);
        log_install_output(package, &stdout, &stderr);
        Err(message)
    }
}

/// Write the captured package-manager output into the application log, keeping
/// only the most recent lines so the log file stays readable.
fn log_install_output(package: &str, stdout: &str, stderr: &str) {
    const MAX_LINES: usize = 120;

    let mut combined = String::new();
    if !stdout.trim().is_empty() {
        combined.push_str(stdout);
    }
    if !stderr.trim().is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(stderr);
    }
    if combined.trim().is_empty() {
        log::info!("Dependency install: no output captured for '{}'", package);
        return;
    }

    let lines: Vec<&str> = combined.lines().collect();
    if lines.len() > MAX_LINES {
        log::warn!(
            "Dependency install: output for '{}' truncated ({} lines kept of {})",
            package,
            MAX_LINES,
            lines.len()
        );
    }
    for line in lines.iter().rev().take(MAX_LINES).rev() {
        log::info!("Dependency install {}: {}", package, line);
    }
}

/// Install OpenZFS on Fedora/RHEL. The stock repositories do not carry OpenZFS,
/// so this enables the official zfsonlinux repository and builds the kernel
/// module via DKMS against the running kernel (kernel-devel must match it).
async fn install_redhat_zfs() -> Result<String, String> {
    log::info!(
        "Dependency install: OpenZFS is not in the default Fedora/RHEL repositories; enabling the official zfsonlinux repository"
    );

    let kernel = kernel_release().ok_or_else(|| {
        "Dependency install: failed to determine the running kernel (uname -r)".to_string()
    })?;
    let kernel_devel = format!("kernel-devel-{kernel}");
    log::info!(
        "Dependency install: installing '{}' for the DKMS module build",
        kernel_devel
    );
    run_sudo_dnf("zfs", &["install", "-y", &kernel_devel])
        .await
        .map_err(|e| format!("Failed to install {}: {}", kernel_devel, e))?;

    let repo_rpm = format!(
        "https://zfsonlinux.org/fedora/zfs-release-3-1{}.noarch.rpm",
        rpm_dist_tag()
    );
    log::info!(
        "Dependency install: enabling the zfsonlinux repository ({})",
        repo_rpm
    );
    run_sudo_dnf("zfs", &["install", "-y", &repo_rpm])
        .await
        .map_err(|e| format!("Failed to enable the zfsonlinux repository: {}", e))?;

    log::info!("Dependency install: installing 'zfs' (OpenZFS)");
    run_sudo_dnf("zfs", &["install", "-y", "zfs"])
        .await
        .map_err(|e| format!("Failed to install zfs: {}", e))?;

    log::info!("Dependency install: 'zfs' installed successfully");
    Ok("Package zfs installed successfully".to_string())
}

/// Release of the running kernel, e.g. `6.16.3-100.fc44.x86_64`.
fn kernel_release() -> Option<String> {
    let output = Command::new("uname").arg("-r").output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// The RPM distribution tag, e.g. `.fc44` for Fedora 44 (empty when absent).
fn rpm_dist_tag() -> String {
    Command::new("rpm")
        .args(["-E", "%{dist}"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Run `sudo dnf <args>`, capture the output into the application log, and fail
/// with the first stderr line when dnf exits non-zero.
async fn run_sudo_dnf(package: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new("sudo")
        .arg("dnf")
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run dnf: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        log_install_output(package, &stdout, &stderr);
        Ok(())
    } else {
        log_install_output(package, &stdout, &stderr);
        Err(stderr.trim().to_string())
    }
}

/// Query the installed version of a package, if any.
pub fn package_version(distro: Distro, package: &str) -> Option<String> {
    let (cmd, args) = match distro {
        Distro::Debian => ("dpkg-query", distro.pkg_version(package)),
        Distro::RedHat => ("rpm", distro.pkg_version(package)),
        Distro::Arch => ("pacman", distro.pkg_version(package)),
    };

    let output = Command::new(cmd).args(&args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let output = if stdout.is_empty() { stderr } else { stdout };

    let line = output.lines().next()?;
    // pacman -Q prints "<name> <version>" - keep only the version token. The
    // dpkg/rpm query formats already emit a bare version.
    Some(
        line.split_whitespace()
            .next_back()
            .unwrap_or(line)
            .to_string(),
    )
}

/// Whether the given package is installed, queried through the distribution's
/// package manager. This is more reliable than scanning PATH for a binary:
/// many tools (dhcpd, in.tftpd, exportfs, httpd, smbd, iftop, zfs) live in
/// `/usr/sbin`, which is not on a regular user's PATH, and binary names can
/// differ across distributions (e.g. `wakeonlan` vs `wol`).
pub fn is_package_installed(distro: Distro, package: &str) -> bool {
    let (cmd, args): (&str, Vec<&str>) = match distro {
        Distro::Debian => ("dpkg-query", vec!["-W", "-f=${db:Status-Abbrev}", package]),
        Distro::RedHat => ("rpm", vec!["-q", package]),
        Distro::Arch => ("pacman", vec!["-Q", package]),
    };

    Command::new(cmd)
        .args(&args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Write and apply a static IP configuration for the given interface using the
/// distribution's network stack (netplan on Debian, NetworkManager elsewhere).
pub async fn apply_static_network_config(
    interface: &str,
    ip: &str,
    prefix: u32,
    gateway: &str,
    dns: &[String],
) -> Result<(), String> {
    match detect() {
        Distro::Debian => apply_netplan(interface, ip, prefix, gateway, dns).await,
        Distro::RedHat | Distro::Arch => apply_nmcli(interface, ip, prefix, gateway, dns).await,
    }
}

async fn apply_netplan(
    interface: &str,
    ip: &str,
    prefix: u32,
    gateway: &str,
    dns: &[String],
) -> Result<(), String> {
    let dns_str = if dns.is_empty() {
        // Preserve the historical default when no DNS servers were configured.
        "8.8.8.8, 8.8.4.4".to_string()
    } else {
        dns.join(", ")
    };

    let content = format!(
        r#"network:
  version: 2
  renderer: networkd
  ethernets:
    {}:
      dhcp4: no
      addresses:
        - {}/{}
      routes:
        - to: default
          via: {}
      nameservers:
        addresses: [{}]
"#,
        interface, ip, prefix, gateway, dns_str
    );

    let path = "/etc/netplan/99-diskless-manager.yaml";
    crate::services::write_with_sudo_tee(path, &content)
        .await
        .map_err(|e| format!("Failed to write netplan config: {}", e))?;

    crate::services::run_sudo_command(["netplan", "apply"])
        .await
        .map_err(|e| format!("Failed to apply netplan: {}", e))
}

async fn apply_nmcli(
    interface: &str,
    ip: &str,
    prefix: u32,
    gateway: &str,
    dns: &[String],
) -> Result<(), String> {
    const CONNECTION: &str = "diskless-manager";

    // Recreate the manager-owned connection so the whole flow is idempotent.
    let _ = Command::new("sudo")
        .args(["-n", "nmcli", "connection", "delete", CONNECTION])
        .status();

    let add_args = [
        "nmcli".to_string(),
        "connection".to_string(),
        "add".to_string(),
        "type".to_string(),
        "ethernet".to_string(),
        "ifname".to_string(),
        interface.to_string(),
        "con-name".to_string(),
        CONNECTION.to_string(),
        "ipv4.addresses".to_string(),
        format!("{}/{}", ip, prefix),
        "ipv4.gateway".to_string(),
        gateway.to_string(),
        "ipv4.method".to_string(),
        "manual".to_string(),
        "connection.autoconnect".to_string(),
        "yes".to_string(),
    ];
    crate::services::run_sudo_command(&add_args)
        .await
        .map_err(|e| format!("Failed to create NetworkManager connection: {}", e))?;

    if !dns.is_empty() {
        let dns_str = dns.join(" ");
        let modify_args = [
            "nmcli".to_string(),
            "connection".to_string(),
            "modify".to_string(),
            CONNECTION.to_string(),
            "ipv4.dns".to_string(),
            dns_str,
        ];
        crate::services::run_sudo_command(&modify_args)
            .await
            .map_err(|e| format!("Failed to set DNS servers: {}", e))?;
    }

    crate::services::run_sudo_command(["nmcli", "connection", "up", CONNECTION])
        .await
        .map_err(|e| format!("Failed to activate NetworkManager connection: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debian_uses_apt_and_debian_service_names() {
        let d = Distro::Debian;
        assert_eq!(
            d.pkg_install("samba"),
            ["apt-get", "install", "-y", "samba"]
        );
        assert_eq!(d.dhcp_service(), "isc-dhcp-server");
        assert_eq!(d.tftp_service(), "tftpd-hpa");
        assert_eq!(d.http_service(), "apache2");
        assert_eq!(d.nfs_service(), "nfs-kernel-server");
        assert_eq!(d.iscsi_service(), "rtslib-fb-targetctl");
        assert_eq!(d.samba_services(), ["smbd", "nmbd"]);
        assert_eq!(d.dhcp_defaults_path(), "/etc/default/isc-dhcp-server");
        assert_eq!(d.tftp_defaults_path(), "/etc/default/tftpd-hpa");
        assert_eq!(
            d.http_config_path(),
            "/etc/apache2/sites-available/diskless-server.conf"
        );
    }

    #[test]
    fn redhat_uses_dnf_and_redhat_service_names() {
        let d = Distro::RedHat;
        assert_eq!(d.pkg_install("samba"), ["dnf", "install", "-y", "samba"]);
        assert_eq!(d.dhcp_service(), "dhcpd");
        assert_eq!(d.tftp_service(), "tftp");
        assert_eq!(d.http_service(), "httpd");
        assert_eq!(d.nfs_service(), "nfs-server");
        assert_eq!(d.iscsi_service(), "target");
        assert_eq!(d.samba_services(), ["smb", "nmb"]);
        assert_eq!(d.dhcp_defaults_path(), "/etc/sysconfig/dhcpd");
        assert_eq!(d.tftp_defaults_path(), "/etc/sysconfig/tftpd");
        assert_eq!(
            d.http_config_path(),
            "/etc/httpd/conf.d/diskless-server.conf"
        );
    }

    #[test]
    fn arch_uses_pacman_and_arch_service_names() {
        let d = Distro::Arch;
        assert_eq!(
            d.pkg_install("samba"),
            ["pacman", "-S", "--noconfirm", "samba"]
        );
        assert_eq!(d.dhcp_service(), "dhcpd4");
        assert_eq!(d.tftp_service(), "tftpd");
        assert_eq!(d.http_service(), "httpd");
        assert_eq!(d.nfs_service(), "nfs-server");
        assert_eq!(d.iscsi_service(), "target");
        assert_eq!(d.samba_services(), ["smb", "nmb"]);
        assert_eq!(
            d.dhcp_defaults_path(),
            "/etc/systemd/system/dhcpd4.service.d/diskless-manager.conf"
        );
        assert_eq!(d.tftp_defaults_path(), "/etc/conf.d/tftpd");
        assert_eq!(
            d.http_config_path(),
            "/etc/httpd/conf/conf.d/diskless-server.conf"
        );
    }

    #[test]
    fn both_families_quote_dhcpd_in_sudoers() {
        assert!(Distro::Debian
            .privileged_commands()
            .contains(&"/usr/sbin/dhcpd"));
        assert!(Distro::RedHat
            .privileged_commands()
            .contains(&"/usr/sbin/dhcpd"));
        assert!(Distro::Debian
            .privileged_commands()
            .contains(&"/usr/bin/apt-get"));
        assert!(Distro::RedHat
            .privileged_commands()
            .contains(&"/usr/bin/dnf"));
    }

    #[test]
    fn arch_grants_pacman_and_nmcli_and_targetcli() {
        let commands = Distro::Arch.privileged_commands();
        assert!(commands.contains(&"/usr/bin/pacman"));
        assert!(commands.contains(&"/usr/bin/nmcli"));
        assert!(commands.contains(&"/usr/sbin/dhcpd"));
    }

    #[test]
    fn version_queries_target_the_right_tool() {
        assert_eq!(
            Distro::Debian.pkg_version("samba"),
            ["--showformat=${Version}\n", "--show", "samba"]
        );
        assert_eq!(
            Distro::RedHat.pkg_version("samba"),
            ["-q", "--queryformat=%{VERSION}", "samba"]
        );
        assert_eq!(Distro::Arch.pkg_version("samba"), ["-Q", "samba"]);
    }
}
