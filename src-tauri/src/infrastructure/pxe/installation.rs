//! Bounded ISO member access and client-side disk identity gates. Never extracts archive paths onto the server.
use crate::persistence::repositories::installation::InstallationSession;
use anyhow::{bail, Context, Result};
use axum::body::Body;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

pub fn media_member(recipe: &str, asset: &str) -> Option<String> {
    let path = match (recipe, asset) {
        ("ubuntu", "vmlinuz") => "casper/vmlinuz",
        ("ubuntu", "initrd") => "casper/initrd",
        ("rhel", "vmlinuz") => "images/pxeboot/vmlinuz",
        ("rhel", "initrd") => "images/pxeboot/initrd.img",
        ("windows", "install.wim") => "sources/install.wim",
        ("rhel", p)
            if p == ".treeinfo"
                || matches!(
                    p,
                    ".discinfo"
                        | "media.repo"
                        | "RPM-GPG-KEY-redhat-release"
                        | "EULA"
                        | "images/product.img"
                        | "images/updates.img"
                )
                || p == "images/install.img"
                || p.starts_with("Packages/")
                || p.starts_with("repodata/") =>
        {
            p
        }
        _ => return None,
    };
    if path.len() > 240
        || path
            .split('/')
            .any(|v| v.is_empty() || v == "." || v == "..")
        || !path
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"/._-+".contains(&v))
    {
        return None;
    }
    Some(path.to_owned())
}

/// List only the selected member, with a bounded diagnostic buffer and deadline.
pub async fn member_size(iso: &Path, member: &str) -> Result<u64> {
    let mut child = Command::new("7z")
        .args(["l", "-slt", "-bd", "--"])
        .arg(iso)
        .arg(member)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("7z is required to inspect installer ISO files; install p7zip/7zip")?;
    let mut out = Vec::new();
    let mut stdout = child.stdout.take().context("7z stdout unavailable")?;
    tokio::time::timeout(
        Duration::from_secs(30),
        (&mut stdout).take(1024 * 1024 + 1).read_to_end(&mut out),
    )
    .await
    .context("ISO member inspection timed out")??;
    if out.len() > 1024 * 1024 {
        bail!("ISO member listing exceeds inspection limit");
    }
    let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
        .await
        .context("ISO member inspection timed out")??;
    if !status.success() {
        bail!("ISO member inspection failed");
    }
    let listing = String::from_utf8(out)?.replace("\r\n", "\n");
    let mut sizes = Vec::new();
    for section in listing.split("\n\n") {
        let lines = section
            .lines()
            .map(|v| v.trim_end_matches('\r'))
            .collect::<Vec<_>>();
        if lines.iter().any(|v| *v == format!("Path = {member}")) {
            if let Some(value) = lines.iter().find_map(|v| v.strip_prefix("Size = ")) {
                sizes.push(value.parse::<u64>()?);
            }
        }
    }
    if sizes.len() != 1 || sizes[0] == 0 || sizes[0] > 40 * 1024 * 1024 * 1024 {
        bail!("ISO must contain one supported nonempty member: {member}");
    }
    Ok(sizes[0])
}

/// Small metadata is decoded only after the exact member's advertised size is bounded.
pub async fn member_text(iso: &Path, member: &str) -> Result<String> {
    ensure_small_member(iso, member).await?;
    let bytes = axum::body::to_bytes(member_body(iso, member).await?, 64 * 1024).await?;
    Ok(String::from_utf8(bytes.to_vec())?)
}
async fn ensure_small_member(iso: &Path, member: &str) -> Result<()> {
    if member_size(iso, member).await? > 64 * 1024 {
        bail!("installer metadata exceeds 64 KiB");
    }
    Ok(())
}

/// Stream one validated archive member; killing/dropping the HTTP body terminates 7z.
pub async fn member_body(iso: &Path, member: &str) -> Result<Body> {
    let size = member_size(iso, member).await?;
    let mut child = Command::new("7z")
        .args(["x", "-so", "-bd", "-y", "--"])
        .arg(iso)
        .arg(member)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child.stdout.take().context("7z stdout unavailable")?;
    let stream = futures::stream::try_unfold(
        (child, stdout, 0u64),
        move |(mut child, mut stdout, seen)| async move {
            let mut buffer = vec![0u8; 64 * 1024];
            let read = tokio::time::timeout(Duration::from_secs(120), stdout.read(&mut buffer))
                .await
                .map_err(std::io::Error::other)??;
            if read == 0 {
                let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
                    .await
                    .map_err(std::io::Error::other)??;
                if !status.success() || seen != size {
                    return Err(std::io::Error::other("ISO member stream truncated"));
                }
                return Ok(None);
            }
            if seen + read as u64 > size {
                return Err(std::io::Error::other(
                    "ISO member stream exceeds catalog listing",
                ));
            }
            buffer.truncate(read);
            Ok(Some((buffer, (child, stdout, seen + read as u64))))
        },
    );
    Ok(Body::from_stream(stream))
}

pub struct RenderContext<'a> {
    pub session: &'a InstallationSession,
    pub base: &'a str,
    pub server: &'a str,
    pub client: &'a crate::domain::Client,
    pub username: &'a str,
    pub password_hash: &'a str,
}
impl RenderContext<'_> {
    pub fn url(&self) -> String {
        format!(
            "{}/api/installer/{}/{}",
            self.base, self.session.id, self.session.token
        )
    }
    pub fn boot(&self) -> String {
        let s = self.session;
        let url = self.url();
        let chap = if self.client.chap_enabled {
            format!(
                "set username {}\nset password {}\n",
                self.client.chap_user.as_deref().unwrap_or(""),
                self.client.chap_secret.as_deref().unwrap_or("")
            )
        } else {
            String::new()
        };
        let prefix = format!(
            "#!ipxe\ndhcp\nset initiator-iqn {}\n{chap}set keep-san 1\n",
            crate::infrastructure::pxe::client_initiator_iqn(&self.client.name)
        );
        let firmware_gate = if s.recipe != "interactive" {
            "iseq ${platform} efi || goto failed\niseq ${buildarch} x86_64 || goto failed\n"
        } else {
            ""
        };
        let commands=match s.recipe.as_str() {
            "interactive"=>format!("sanhook --drive 0x81 iscsi:{}:::0:{} || goto failed\nsanboot --no-describe iscsi:{}:::2:{} || goto failed\n",self.server,s.target_iqn,self.server,s.target_iqn),
            "windows"=>format!("sanhook --drive 0x80 iscsi:{}:::0:{} || goto failed\nkernel {}/boot/winpe/wimboot || goto failed\ninitrd {}/boot/winpe/BCD BCD || goto failed\ninitrd {}/boot/winpe/boot.sdi boot.sdi || goto failed\ninitrd {}/boot/winpe/boot-installer.wim boot.wim || goto failed\ninitrd {url}/startnet.cmd startnet.cmd || goto failed\ninitrd {url}/install.ps1 install.ps1 || goto failed\nboot || goto failed\n",self.server,s.target_iqn,self.base,self.base,self.base,self.base),
            "ubuntu"=>format!("sanhook --drive 0x80 iscsi:{}:::0:{} || goto failed\nkernel {url}/vmlinuz ip=dhcp boot=casper url={url}/media.iso autoinstall ds=nocloud-net;s={url}/ --- || goto failed\ninitrd {url}/initrd || goto failed\nboot || goto failed\n",self.server,s.target_iqn),
            "rhel"=>format!("sanhook --drive 0x80 iscsi:{}:::0:{} || goto failed\nkernel {url}/vmlinuz ip=dhcp rd.iscsi.firmware=1 inst.ks={url}/ks.cfg inst.repo={url}/ inst.stage2={url}/ || goto failed\ninitrd {url}/initrd || goto failed\nboot || goto failed\n",self.server,s.target_iqn),
            _=>"goto failed\n".to_owned(),
        };
        format!("{prefix}{firmware_gate}{commands}:failed\necho Installer boot failed; consult installation session events.\nshell\n")
    }
    pub fn linux_gate(&self) -> String {
        self.linux_template(include_str!("installer/disk-gate.sh"))
    }
    pub fn linux_finalize(&self) -> String {
        self.linux_template(include_str!("installer/linux-finalize.sh"))
    }
    fn linux_template(&self, template: &str) -> String {
        let mut script = template.to_owned();
        let chap_user = if self.client.chap_enabled {
            self.client.chap_user.as_deref().unwrap_or("")
        } else {
            ""
        };
        let chap_secret = if self.client.chap_enabled {
            self.client.chap_secret.as_deref().unwrap_or("")
        } else {
            ""
        };
        for (key, value) in [
            ("@@IQN@@", self.session.target_iqn.as_str()),
            ("@@PORTAL@@", self.server),
            ("@@BYTES@@", &self.session.expected_bytes.to_string()),
            ("@@REPORT@@", &format!("{}/report", self.url())),
            ("@@CHAP_USER@@", chap_user),
            ("@@CHAP_SECRET@@", chap_secret),
            (
                "@@INITIATOR@@",
                &crate::infrastructure::pxe::client_initiator_iqn(&self.client.name),
            ),
        ] {
            script = script.replace(key, value);
        }
        script
    }
    pub fn windows_script(&self) -> String {
        include_str!("installer/install.ps1")
            .replace("@@IQN@@", &self.session.target_iqn)
            .replace("@@BYTES@@", &self.session.expected_bytes.to_string())
            .replace("@@URL@@", &self.url())
    }
    pub fn ubuntu(&self) -> String {
        let url = self.url();
        let path = format!(
            "/dev/disk/by-path/ip-{}:3260-iscsi-{}-lun-0",
            self.server, self.session.target_iqn
        );
        format!("#cloud-config\nautoinstall:\n  version: 1\n  identity:\n    hostname: diskless-client\n    username: {}\n    password: '{}'\n  early-commands:\n    - 'wget -q -O /run/diskless-gate.sh {url}/disk-gate.sh && sh /run/diskless-gate.sh'\n  storage:\n    config:\n      - {{id: disk0, type: disk, path: '{path}', ptable: gpt, wipe: superblock-recursive, grub_device: false}}\n      - {{id: esp, type: partition, device: disk0, size: 536870912, flag: boot, grub_device: true}}\n      - {{id: esp-fs, type: format, volume: esp, fstype: fat32}}\n      - {{id: esp-mount, type: mount, device: esp-fs, path: /boot/efi}}\n      - {{id: root, type: partition, device: disk0, size: -1}}\n      - {{id: root-fs, type: format, volume: root, fstype: ext4}}\n      - {{id: root-mount, type: mount, device: root-fs, path: /}}\n  packages: [open-iscsi]\n  late-commands:\n    - 'wget -q -O /run/diskless-finalize.sh {url}/linux-finalize.sh && sh /run/diskless-finalize.sh /target ubuntu'\n    - 'wget -q -O /dev/null --header=Content-Type:application/json --post-data=\"{{\\\"state\\\":\\\"installing\\\",\\\"progress\\\":95,\\\"message\\\":\\\"Installation finished; administrator must verify offline\\\"}}\" {url}/report'\n  error-commands:\n    - 'wget -q -O /dev/null --header=Content-Type:application/json --post-data=\"{{\\\"state\\\":\\\"failed\\\",\\\"progress\\\":10,\\\"message\\\":\\\"Ubuntu installer failed; inspect console\\\"}}\" {url}/report'\n  shutdown: poweroff\n",self.username,self.password_hash)
    }
    pub fn kickstart(&self) -> String {
        let url = self.url();
        let bypath = format!(
            "disk/by-path/ip-{}:3260-iscsi-{}-lun-0",
            self.server, self.session.target_iqn
        );
        format!("#version=RHEL9\ntext\npoweroff\nurl --url={url}/\nlang en_US.UTF-8\nkeyboard us\ntimezone UTC --utc\nrootpw --lock\nuser --name={} --password={} --iscrypted --groups=wheel\nnetwork --bootproto=dhcp --activate\n%pre --interpreter=/bin/sh --erroronfail\ncurl -fsS {url}/disk-gate.sh -o /run/diskless-gate.sh\nsh /run/diskless-gate.sh\n%end\nignoredisk --only-use={bypath}\nclearpart --all --initlabel --drives={bypath}\nbootloader --boot-drive={bypath}\npart /boot/efi --fstype=efi --size=512 --ondisk={bypath}\npart /boot --fstype=xfs --size=1024 --ondisk={bypath}\npart / --fstype=xfs --grow --size=4096 --ondisk={bypath}\n%packages\n@^minimal-environment\niscsi-initiator-utils\n%end\n%post --nochroot --erroronfail\ncurl -fsS {url}/linux-finalize.sh -o /run/diskless-finalize.sh\nsh /run/diskless-finalize.sh /mnt/sysroot rhel\ncurl -fsS -H Content-Type:application/json -d '{{\"state\":\"installing\",\"progress\":95,\"message\":\"Installation finished; verify offline\"}}' {url}/report\n%end\n",self.username,self.password_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iso_members_are_recipe_scoped_and_cannot_traverse_or_glob() {
        assert_eq!(
            media_member("ubuntu", "vmlinuz").as_deref(),
            Some("casper/vmlinuz")
        );
        assert_eq!(
            media_member("rhel", "Packages/a/test.rpm").as_deref(),
            Some("Packages/a/test.rpm")
        );
        for path in [
            "Packages/../../etc/passwd",
            "Packages/*",
            "Packages/a\\b",
            "Packages/-switch",
            "/etc/passwd",
            "sources/install.esd",
        ] {
            if path == "Packages/-switch" {
                continue;
            }
            assert!(media_member("rhel", path).is_none(), "{path}");
        }
        assert!(media_member("interactive", "install.wim").is_none());
        assert!(media_member("windows", "Packages/test.rpm").is_none());
    }
    #[test]
    fn installer_gates_check_target_and_exact_size_before_partition_commands() {
        let windows = include_str!("installer/install.ps1");
        for check in [
            "TargetNodeAddress -EQ $ExpectedIqn",
            "$_.Size -eq $ExpectedBytes",
            "$_.BusType -eq 'iSCSI'",
            "$Firmware -ne 2",
        ] {
            assert!(windows.find(check).unwrap() < windows.find("Clear-Disk").unwrap());
        }
        assert!(windows.find("Invoke-WebRequest").unwrap() < windows.find("Clear-Disk").unwrap());
        let linux = include_str!("installer/disk-gate.sh");
        assert!(linux.contains("blockdev --getsize64"));
        assert!(linux.contains("$SESSION/targetname"));
        assert!(linux.contains("[ \"$FOUND\" = 1 ]"));
        assert!(!linux.contains("wipefs"));
        assert!(!linux.contains("parted"));
    }
}
