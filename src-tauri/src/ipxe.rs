use std::path::Path;

pub const NVMEOF_FIRMWARE: &str = "snponly-nvmeof.efi";
pub const NVMEOF_CAPABILITY_FLAG: &str = "diskless-nvmeof";

/// Port the dedicated client-enrollment HTTP listener binds to.
///
/// The enrollment listener is deliberately separate from the management API
/// so iPXE clients only ever reach a single, unauthenticated endpoint instead
/// of the full management surface.
pub const ENROLL_PORT: u16 = 4237;

/// Returns a filesystem-safe identifier suitable for a per-client iPXE filename.
#[must_use]
pub fn client_script_slug(client_name: &str) -> String {
    let mut slug = String::with_capacity(client_name.len());
    for byte in client_name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            slug.push((byte as char).to_ascii_lowercase());
        } else {
            slug.push('_');
        }
    }

    if slug.is_empty() {
        "client".to_owned()
    } else {
        slug
    }
}

#[must_use]
pub fn client_mac_slug(mac: &str) -> String {
    let slug: String = mac
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .map(|character| character.to_ascii_lowercase())
        .collect();

    if slug.len() == 12 {
        slug
    } else {
        "unknown".to_owned()
    }
}

#[must_use]
pub fn client_script_path(client_name: &str) -> String {
    format!("{}.ipxe", client_script_slug(client_name))
}

#[must_use]
pub fn client_mac_script_path(mac: &str) -> String {
    format!("clients/{}.ipxe", client_mac_slug(mac))
}

#[must_use]
pub fn render_client_script(client_name: &str, target_iqn: &str, http_port: u16) -> String {
    render_client_script_with_mode(client_name, target_iqn, http_port, true, None)
}

/// iPXE response served to a client that is not yet registered.
///
/// Reuses the session variables set by `autoexec.ipxe` (`boot-url`,
/// `client-mac`), which are preserved across a `chain` into this script.
#[must_use]
pub fn render_enrollment_pending() -> String {
    r##"#!ipxe
echo ##########################################################
echo # Diskless Manager - client enrollment                 #
echo #                                                        #
echo # Client ${client-mac} has been registered.             #
echo # No master image has been assigned yet.                #
echo # Ask an administrator to provision this machine.       #
echo ##########################################################
sleep 30
reboot
"##
    .to_string()
}

/// iPXE response served to an already-provisioned client whose per-client
/// menu lives on the boot server. Re-enters the normal dispatch path.
#[must_use]
pub fn render_enrollment_redirect() -> String {
    r##"#!ipxe
echo Diskless Manager: client ${client-mac} is provisioned.
echo Re-entering the normal boot menu...
chain ${boot-url}/clients/${client-mac}.ipxe || shell
"##
    .to_string()
}

/// iPXE response served when the enrollment request itself could not be
/// understood. Retries are deferred so clients poll rather than hammer the
/// server on a broken configuration.
#[must_use]
pub fn render_enrollment_invalid_mac() -> String {
    r##"#!ipxe
echo Diskless Manager: enrollment failed (unrecognized client MAC).
echo Check the boot server network configuration.
sleep 30
reboot
"##
    .to_string()
}

/// iPXE response served to a client whose record is disabled.
///
/// A disabled client must never boot, even if it is fully provisioned:
/// this is the binding gate that e.g. decommissioned or quarantined
/// machines hit. Polls slowly so re-enabling takes effect without hurry.
#[must_use]
pub fn render_enrollment_disabled() -> String {
    r##"#!ipxe
echo ##########################################################
echo # Diskless Manager - client disabled                   #
echo #                                                        #
echo # This machine is disabled by an administrator.         #
echo # Booting is not permitted.                             #
echo ##########################################################
sleep 60
reboot
"##
    .to_string()
}

/// iPXE response served to an unknown client while the registration
/// window is closed.
///
/// Deliberately reveals nothing about when enrollment opens: unknown
/// machines outside an admin-opened window cannot distinguish "closed"
/// from "nonexistent" beyond this message.
#[must_use]
pub fn render_enrollment_closed() -> String {
    r##"#!ipxe
echo ##########################################################
echo # Diskless Manager - enrollment closed                  #
echo #                                                        #
echo # This machine is not registered.                       #
echo # Ask an administrator to open enrollment and retry.    #
echo ##########################################################
sleep 60
reboot
"##
    .to_string()
}

#[must_use]
pub fn render_client_script_with_mode(
    client_name: &str,
    target_iqn: &str,
    http_port: u16,
    ready: bool,
    chap: Option<&crate::infrastructure::iscsi::ChapCredentials>,
) -> String {
    let slug = client_script_slug(client_name);
    let port_suffix = if http_port == 80 {
        String::new()
    } else {
        format!(":{http_port}")
    };
    // One-way CHAP for the iSCSI attach. iPXE has no --username flag on
    // sanboot/sanhook: authentication rides the `username`/`password`
    // settings, which sanboot, sanhook, and the iBFT handoff all honor.
    // The settings are emitted once up top; every attach line below stays
    // byte-identical so unauthenticated menus never change.
    let chap_settings = match chap {
        Some(credentials) => format!(
            "set username {}\nset password {}\n",
            credentials.username, credentials.password
        ),
        None => String::new(),
    };

    if !ready {
        return format!(
            r##"#!ipxe
dhcp
# Client is registered but does not have a verified Windows installation.
# Attach its iSCSI disk and boot WinPE for installation.
set client {slug}
set target-iqn {target_iqn}
set boot-url http://${{next-server}}{port_suffix}
set keep-san 1
{chap_settings}
isset ${{root-path}} || goto no_target
echo Provisioning {slug}: attaching ${{root-path}}
sanhook ${{root-path}} || goto failed

echo Loading Windows PE...
kernel ${{boot-url}}/boot/winpe/wimboot || goto failed
initrd ${{boot-url}}/boot/winpe/BCD BCD || goto failed
initrd ${{boot-url}}/boot/winpe/boot.sdi boot.sdi || goto failed
initrd ${{boot-url}}/boot/winpe/boot.wim boot.wim || goto failed
initrd ${{boot-url}}/boot/winpe/install.bat install.bat || goto failed
initrd ${{boot-url}}/boot/winpe/find-target-disk.txt find-target-disk.txt || goto failed
initrd ${{boot-url}}/boot/winpe/show-disks.txt show-disks.txt || goto failed
initrd ${{boot-url}}/boot/winpe/partition-windows.txt partition-windows.txt || goto failed
boot || goto failed

:no_target
echo No iSCSI root-path was supplied by DHCP.
echo The client cannot be provisioned until a storage target exists.
shell

:failed
echo WinPE provisioning boot failed.
shell
"##,
        );
    }

    let root = format!("/dev/disk/by-path/ip-${{next-server}}:3260-iscsi-{target_iqn}-lun-0-part2");
    let initiator_iqn = format!("iqn.2026-01.client:client.{slug}");
    let nvme_nqn = format!("nqn.2026-09.local.diskless:client.{slug}");

    format!(
        r##"#!ipxe
dhcp
# Client-specific iPXE script generated by diskless-manager.
set client {slug}
set target-iqn {target_iqn}
set initiator-iqn {initiator_iqn}
set nvme-nqn {nvme_nqn}
set boot-url http://${{next-server}}{port_suffix}
set keep-san 1
{chap_settings}
:start
menu Diskless Boot Menu (Client: ${{client}} - Server IP: ${{next-server}})
item --key b boot_ubuntu    Diskless boot Ubuntu 25.10
item --key a boot_anduinos  Diskless boot Anduin OS 25.04
item --key d boot_debian    Diskless boot Debian 13.2
item --key w boot_windows   Diskless boot Windows 11 (iSCSI)
item --key n boot_windows_nvme Windows Server NVMe/TCP (Experimental)
item --key i boot_pe       Boot from WinPE (WIM)
item --gap --             ------------------------- Advanced options ------------------------------
item shell               Drop to iPXE shell
item reboot              Reboot computer
choose --timeout 5000 --default boot_windows selected || goto cancel
goto ${{selected}}

:cancel
goto shell

:shell
shell
goto start

:failed
echo Booting failed, dropping to shell
shell

:reboot
reboot

:boot_windows
set keep-san 1
set net0/gateway 0.0.0.0
sanboot ${{root-path}} || goto failed

:boot_windows_nvme
# Stock iPXE has no NVMe/TCP SAN driver. Chain the isolated Kurrent-based
# firmware once; its embedded script sets {nvme_flag}=1 and returns through
# autoexec.ipxe to this client menu. On the second pass, perform native NVMe/TCP.
isset ${{{nvme_flag}}} || goto load_nvmeof_firmware
set keep-san 1
set net0/gateway 0.0.0.0
set nvme-root nvme://${{next-server}}:4420/${{nvme-nqn}}
echo Experimental NVMe/TCP boot: ${{nvme-root}}
sanboot ${{nvme-root}} || goto failed

:load_nvmeof_firmware
echo Loading experimental NVMe/TCP-capable iPXE firmware...
chain ${{boot-url}}/{nvme_firmware} || goto nvmeof_firmware_missing
# The embedded firmware normally replaces this iPXE instance. If chain ever
# returns, go back to the menu instead of accidentally running stock sanboot.
goto start

:nvmeof_firmware_missing
echo Could not load ${{boot-url}}/{nvme_firmware}
echo Build/install it with: bash scripts/build-nvmeof-ipxe.sh --install
goto failed

:boot_pe
set keep-san 1
set net0/gateway 0.0.0.0
sanhook ${{root-path}} || goto failed
kernel ${{boot-url}}/boot/winpe/wimboot || goto failed
initrd ${{boot-url}}/boot/winpe/BCD BCD || goto failed
initrd ${{boot-url}}/boot/winpe/boot.sdi boot.sdi || goto failed
initrd ${{boot-url}}/boot/winpe/boot.wim boot.wim || goto failed
initrd ${{boot-url}}/boot/winpe/install.bat install.bat || goto failed
initrd ${{boot-url}}/boot/winpe/find-target-disk.txt find-target-disk.txt || goto failed
initrd ${{boot-url}}/boot/winpe/show-disks.txt show-disks.txt || goto failed
initrd ${{boot-url}}/boot/winpe/partition-windows.txt partition-windows.txt || goto failed
boot || goto failed

:boot_debian
set net0/gateway 0.0.0.0
sanhook ${{root-path}} || goto failed
kernel ${{boot-url}}/debian/vmlinuz ip=dhcp iscsi_initiator=${{initiator-iqn}} iscsi_target_name=${{target-iqn}} iscsi_target_ip=${{next-server}} root={root} rw rd.iscsi.waitnet=0 rd.iscsi.ibft=1
initrd ${{boot-url}}/debian/initrd.img
boot || goto failed

:boot_ubuntu
set net0/gateway 0.0.0.0
sanhook ${{root-path}} || goto failed
kernel ${{boot-url}}/ubuntu/vmlinuz ip=dhcp iscsi_initiator=${{initiator-iqn}} iscsi_target_name=${{target-iqn}} iscsi_target_ip=${{next-server}} root={root} rw rd.iscsi.waitnet=0 rd.iscsi.ibft=1
initrd ${{boot-url}}/ubuntu/initrd.img
boot || goto failed

:boot_anduinos
set net0/gateway 0.0.0.0
sanhook ${{root-path}} || goto failed
kernel ${{boot-url}}/anduinos/vmlinuz ip=dhcp iscsi_initiator=${{initiator-iqn}} iscsi_target_name=${{target-iqn}} iscsi_target_ip=${{next-server}} root={root} rw rd.iscsi.waitnet=0 rd.iscsi.ibft=1
initrd ${{boot-url}}/anduinos/initrd.img
boot || goto failed
"##,
        nvme_flag = NVMEOF_CAPABILITY_FLAG,
        nvme_firmware = NVMEOF_FIRMWARE,
    )
}

/// Generic PXE entry point used for diagnostic WinPE booting.
#[must_use]
pub fn render_autoexec_ipxe(http_port: u16) -> String {
    let port_suffix = if http_port == 80 {
        String::new()
    } else {
        format!(":{http_port}")
    };

    format!(
        r##"#!ipxe
# Generated by diskless-manager.
# Diagnostic WinPE boot: do NOT attach the iSCSI disk.

dhcp
set boot-url http://${{next-server}}{port_suffix}

echo ================================================
echo Diskless Manager - WinPE Diagnostic Boot
echo iSCSI SAN attachment: DISABLED
echo Loading Windows PE...
echo ================================================

kernel ${{boot-url}}/boot/winpe/wimboot || goto failed
initrd ${{boot-url}}/boot/winpe/BCD BCD || goto failed
initrd ${{boot-url}}/boot/winpe/boot.sdi boot.sdi || goto failed
initrd ${{boot-url}}/boot/winpe/boot.wim boot.wim || goto failed
initrd ${{boot-url}}/boot/winpe/install.bat install.bat || goto failed
initrd ${{boot-url}}/boot/winpe/find-target-disk.txt find-target-disk.txt || goto failed
initrd ${{boot-url}}/boot/winpe/show-disks.txt show-disks.txt || goto failed
initrd ${{boot-url}}/boot/winpe/partition-windows.txt partition-windows.txt || goto failed
boot || goto failed

:failed
echo ================================================
echo WinPE boot failed.
echo ================================================
shell
"##,
    )
}

#[must_use]
pub fn is_managed_script_path(root: &Path, relative_path: &str) -> bool {
    let path = Path::new(relative_path);
    !path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        && root.join(path).starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_deterministic_and_safe() {
        assert_eq!(client_script_slug("PC 001/West"), "pc_001_west");
        assert_eq!(client_script_path("PC 001/West"), "pc_001_west.ipxe");
    }

    #[test]
    fn deny_scripts_reboot_loop_without_booting() {
        for script in [render_enrollment_disabled(), render_enrollment_closed()] {
            assert!(script.starts_with("#!ipxe"), "must be an iPXE script");
            assert!(script.contains("reboot"), "deny scripts must loop, never boot");
            assert!(!script.contains("sanboot"), "deny scripts must not boot");
            assert!(!script.contains("chain "), "deny scripts must not chain");
        }
    }

    #[test]
    fn mac_slug_is_stable_and_safe() {
        assert_eq!(client_mac_slug("00:11:22:33:44:55"), "001122334455");
        assert_eq!(
            client_mac_script_path("00:11:22:33:44:55"),
            "clients/001122334455.ipxe"
        );
        assert_eq!(client_mac_slug("invalid"), "unknown");
    }

    #[test]
    fn script_uses_persisted_target_and_not_hard_coded_pc001() {
        let script = render_client_script("PC002", "iqn.2024-01.com.diskless:client.pc002", 4433);
        assert!(script.contains("set target-iqn iqn.2024-01.com.diskless:client.pc002"));
        assert!(script.contains("iscsi_target_name=${target-iqn}"));
        assert!(script.contains("http://${next-server}:4433"));
        assert!(!script.contains("client.pc001"));
    }

    #[test]
    fn unprovisioned_script_boots_winpe() {
        let script = render_client_script_with_mode(
            "PC001",
            "iqn.2024-01.com.diskless:client.pc001",
            4433,
            false,
            None,
        );
        assert!(script.contains("sanhook ${root-path}"));
        assert!(script.contains("kernel ${boot-url}/boot/winpe/wimboot"));
        assert!(!script.contains("boot_windows_nvme"));
    }

    #[test]
    fn ready_script_sanboots() {
        let script = render_client_script_with_mode(
            "PC001",
            "iqn.2024-01.com.diskless:client.pc001",
            4433,
            true,
            None,
        );
        assert!(script.contains("sanboot ${root-path}"));
    }

    #[test]
    fn ready_script_chains_nvme_firmware_before_nvme_sanboot() {
        let script = render_client_script_with_mode(
            "PC001",
            "iqn.2024-01.com.diskless:client.pc001",
            4433,
            true,
            None,
        );

        assert!(script.contains("Windows Server NVMe/TCP (Experimental)"));
        assert!(script.contains("set nvme-nqn nqn.2026-09.local.diskless:client.pc001"));
        assert!(script.contains("isset ${diskless-nvmeof} || goto load_nvmeof_firmware"));
        assert!(script.contains("chain ${boot-url}/snponly-nvmeof.efi"));
        assert!(script.contains("nvme://${next-server}:4420/${nvme-nqn}"));
        assert!(script.contains("sanboot ${nvme-root}"));
        assert!(script.contains("--default boot_windows"));
    }

    #[test]
    fn chap_credentials_ride_ipxe_settings_not_sanboot_flags() {
        use crate::infrastructure::iscsi::ChapCredentials;

        let chap = ChapCredentials {
            username: "chap-pc001".to_string(),
            password: "AbcDef123456".to_string(),
        };
        let script = render_client_script_with_mode(
            "PC001",
            "iqn.2024-01.com.diskless:client.pc001",
            4433,
            true,
            Some(&chap),
        );
        // iPXE has no --username flag: authentication rides the
        // username/password settings honored by sanboot, sanhook and iBFT.
        assert!(script.contains("set username chap-pc001"));
        assert!(script.contains("set password AbcDef123456"));
        assert!(!script.contains("--username"), "no such iPXE option");
        // Attach lines themselves stay plain.
        assert!(script.contains("sanboot ${root-path}"));
    }

    #[test]
    fn port_80_is_not_rendered_explicitly() {        let script = render_client_script("PC001", "iqn.example:pc001", 80);
        assert!(script.contains("set boot-url http://${next-server}"));
        assert!(!script.contains("${next-server}:80"));
    }

    #[test]
    fn autoexec_boots_winpe_without_san() {
        let script = render_autoexec_ipxe(80);
        assert!(script.starts_with("#!ipxe"));
        assert!(script.contains("dhcp"));
        assert!(script.contains("/boot/winpe/wimboot"));
        assert!(!script.contains("sanhook ${root-path}"));
        assert!(!script.contains("sanboot"));
    }

    #[test]
    fn autoexec_supports_non_default_http_port() {
        let script = render_autoexec_ipxe(4433);
        assert!(script.contains("set boot-url http://${next-server}:4433"));
    }

    #[test]
    fn managed_path_rejects_escape() {
        let root = Path::new("/srv/tftp");
        assert!(is_managed_script_path(root, "pc001.ipxe"));
        assert!(!is_managed_script_path(root, "../pc001.ipxe"));
        assert!(!is_managed_script_path(root, "/etc/passwd"));
    }
}
