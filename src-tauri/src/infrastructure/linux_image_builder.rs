use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use std::{
    ffi::OsStr,
    fs,
    io::{Read, Write},
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const EXECUTABLE: &str = "/usr/bin/diskless-manager";
const WORKER_ENV: &str = "DISKLESS_LINUX_IMAGE_PRIVATE_MOUNT";
const MINIMUM_SIZE_BYTES: u64 = 20 * 1024 * 1024 * 1024;

#[derive(Debug, Serialize)]
struct PreflightResult {
    distro: &'static str,
    minimum_size_gb: u64,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct LinuxImageAccount {
    username: String,
    password: String,
    sudo_user: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Distro {
    Ubuntu,
    Fedora,
    CachyOs,
}

impl Distro {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ubuntu => "ubuntu",
            Self::Fedora => "fedora",
            Self::CachyOs => "cachyos",
        }
    }

    fn from_payload_name(name: &str) -> Option<Self> {
        match name {
            "ubuntu" | "pop" => Some(Self::Ubuntu),
            "fedora" | "nobara" => Some(Self::Fedora),
            "cachyos" | "arch" => Some(Self::CachyOs),
            _ => None,
        }
    }
}

enum Request {
    Check {
        iso: PathBuf,
        size_gb: Option<u64>,
        volume: Option<String>,
    },
    Build {
        iso: PathBuf,
        volume: String,
    },
}

/// Entry point restricted by the application's sudoers rule. The worker runs
/// in a private mount namespace so build mounts never leak into the server.
pub fn run_internal_command(arguments: &[String]) -> Result<()> {
    if arguments.len() == 1 && arguments[0] == "--authorization-check" {
        return Ok(());
    }
    let request = parse_request(arguments)?;
    let account = if matches!(request, Request::Build { .. }) {
        Some(read_account_config()?)
    } else {
        None
    };
    let mut command = Command::new("unshare");
    command.args(["--mount", "--propagation", "private", "--", EXECUTABLE]);
    match &request {
        Request::Check {
            iso,
            size_gb,
            volume,
        } => {
            command
                .args(["internal-linux-image-worker", "--check", "--iso"])
                .arg(path_arg(iso)?);
            if let Some(volume) = volume {
                command.arg("--volume").arg(volume);
            }
            if let Some(size_gb) = size_gb {
                command.arg("--size-gb").arg(size_gb.to_string());
            }
        }
        Request::Build { iso, volume } => {
            command
                .args(["internal-linux-image-worker", "--build", "--iso"])
                .arg(path_arg(iso)?)
                .arg("--volume")
                .arg(volume);
            command.stdin(Stdio::piped());
        }
    }
    command.env(WORKER_ENV, "1");
    let mut child = command
        .spawn()
        .context("start private Linux image worker")?;
    if let Some(account) = account {
        let mut input = serde_json::to_vec(&account)?;
        let write_result = child
            .stdin
            .take()
            .context("private Linux image worker stdin unavailable")?
            .write_all(&input);
        input.fill(0);
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error).context("send Linux image account settings");
        }
    }
    let status = child
        .wait()
        .context("wait for private Linux image worker")?;
    ensure!(
        status.success(),
        "private Linux image worker failed ({status})"
    );
    Ok(())
}

/// Worker entry point. It is only invoked by `run_internal_command` after
/// `unshare` has created the private mount namespace.
pub fn run_worker_command(arguments: &[String]) -> Result<()> {
    ensure!(
        std::env::var(WORKER_ENV).as_deref() == Ok("1")
            && fs::metadata("/proc/self")
                .map(|metadata| metadata.uid() == 0)
                .unwrap_or(false),
        "Linux image worker must run as root in its private mount namespace"
    );
    let request = parse_request(arguments)?;
    let result = match request {
        Request::Check {
            iso,
            size_gb,
            volume,
        } => run_check(&iso, size_gb, volume.as_deref()),
        Request::Build { iso, volume } => run_build(&iso, &volume, read_account_config()?),
    };
    result
}

fn read_account_config() -> Result<LinuxImageAccount> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(16 * 1024)
        .read_to_end(&mut input)
        .context("read Linux image account settings")?;
    let account: LinuxImageAccount = serde_json::from_slice(&input)
        .context("Linux image account settings are missing or invalid")?;
    validate_account(&account)?;
    Ok(account)
}

fn validate_account(account: &LinuxImageAccount) -> Result<()> {
    let username = account.username.as_str();
    ensure!(
        (1..=32).contains(&username.len())
            && username
                .as_bytes()
                .first()
                .is_some_and(|character| character.is_ascii_lowercase() || *character == b'_')
            && username
                .bytes()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || matches!(character, b'_' | b'-')),
        "username must start with a lowercase letter or underscore and contain only lowercase letters, digits, underscores, or hyphens"
    );
    ensure!(
        username != "root",
        "root cannot be selected as the Linux image user"
    );
    ensure!(
        !account.password.is_empty()
            && account.password.len() <= 256
            && !account.password.chars().any(char::is_control),
        "password must contain 1 to 256 non-control characters"
    );
    Ok(())
}

fn parse_request(arguments: &[String]) -> Result<Request> {
    let mut check = false;
    let mut build = false;
    let mut iso = None;
    let mut volume = None;
    let mut size_gb = None;
    let mut index = 0;
    while index < arguments.len() {
        let option = arguments[index].as_str();
        match option {
            "--check" => check = true,
            "--build" => build = true,
            "--iso" | "--volume" | "--size-gb" => {
                index += 1;
                let value = arguments
                    .get(index)
                    .filter(|value| !value.starts_with('-'))
                    .with_context(|| format!("{option} requires a value"))?;
                match option {
                    "--iso" => iso = Some(PathBuf::from(value)),
                    "--volume" => volume = Some(value.clone()),
                    _ => size_gb = Some(value.parse::<u64>().context("invalid size in GiB")?),
                }
            }
            _ => bail!("unknown Linux image option: {option}"),
        }
        index += 1;
    }
    ensure!(check ^ build, "select exactly one of --check or --build");
    let iso = iso.context("--iso is required")?;
    if check {
        ensure!(
            size_gb.is_some() ^ volume.is_some(),
            "--check requires exactly one of --size-gb or --volume"
        );
        if let Some(size_gb) = size_gb {
            ensure!(size_gb >= 20, "target must be at least 20 GiB");
        }
        if let Some(volume) = &volume {
            validate_volume_name(volume)?;
        }
        Ok(Request::Check {
            iso,
            size_gb,
            volume,
        })
    } else {
        ensure!(size_gb.is_none(), "--size-gb is not valid with --build");
        let volume = volume.context("--volume is required with --build")?;
        validate_volume_name(&volume)?;
        Ok(Request::Build { iso, volume })
    }
}

fn validate_volume_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && !name.starts_with('-'),
        "invalid ZFS volume name"
    );
    ensure!(
        name.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || "-_.:".contains(character)
                })
        }),
        "invalid ZFS volume name"
    );
    ensure!(
        !name.contains('@'),
        "target must be a ZFS volume, not a snapshot"
    );
    Ok(())
}

fn path_arg(path: &Path) -> Result<&OsStr> {
    ensure!(path.is_absolute(), "ISO path must be absolute");
    Ok(path.as_os_str())
}

fn run_check(iso: &Path, size_gb: Option<u64>, volume: Option<&str>) -> Result<()> {
    let size_gb = if let Some(volume) = volume {
        let device = validate_target_volume(volume)?;
        let bytes = output_text("blockdev", [OsStr::new("--getsize64"), device.as_os_str()])?
            .trim()
            .parse::<u64>()
            .context("could not read target volume size")?;
        bytes / (1024 * 1024 * 1024)
    } else {
        size_gb.context("--size-gb is required")?
    };
    ensure!(size_gb >= 20, "target must be at least 20 GiB");
    let mut workspace = Workspace::new()?;
    let result = (|| {
        let distro = workspace.open_iso(iso)?;
        println!(
            "{}",
            serde_json::to_string(&PreflightResult {
                distro: distro.as_str(),
                minimum_size_gb: 20,
            })?
        );
        Ok(())
    })();
    workspace.finish(result)
}

fn run_build(iso: &Path, volume: &str, account: LinuxImageAccount) -> Result<()> {
    let device = validate_target_volume(volume)?;
    let size = output_text("blockdev", [OsStr::new("--getsize64"), device.as_os_str()])?
        .trim()
        .parse::<u64>()
        .context("could not read target volume size")?;
    ensure!(size >= MINIMUM_SIZE_BYTES, "target must be at least 20 GiB");

    let mut workspace = Workspace::new()?;
    let result = (|| {
        let distro = workspace.open_iso(iso)?;
        build_image(&mut workspace, distro, volume, &device, &account)?;
        println!(
            "{}",
            serde_json::json!({"status":"ready", "distro":distro.as_str()})
        );
        Ok(())
    })();
    workspace.finish(result)
}

fn validate_target_volume(volume: &str) -> Result<PathBuf> {
    let property = |name: &str| {
        output_text(
            "zfs",
            [
                OsStr::new("get"),
                OsStr::new("-H"),
                OsStr::new("-o"),
                OsStr::new("value"),
                OsStr::new(name),
                OsStr::new(volume),
            ],
        )
        .map(|value| value.trim().to_owned())
    };
    let volmode = property("volmode").context("cannot read ZFS volume mode")?;
    ensure!(
        volmode == "dev" || volmode == "default",
        "target is not an exposed ZFS volume"
    );
    let resolved = output_text(
        "readlink",
        [OsStr::new("-f"), OsStr::new(&format!("/dev/zvol/{volume}"))],
    )?;
    let device = PathBuf::from(resolved.trim());
    ensure!(
        fs::metadata(&device).is_ok_and(|metadata| metadata.file_type().is_block_device()),
        "target has no ZFS block device"
    );
    ensure!(property("type")? == "volume", "target must be a ZFS volume");
    ensure!(property("origin")? == "-", "target is a clone; refusing");

    let snapshots = output_text(
        "zfs",
        [
            OsStr::new("list"),
            OsStr::new("-H"),
            OsStr::new("-t"),
            OsStr::new("snapshot"),
            OsStr::new("-o"),
            OsStr::new("name"),
            OsStr::new("-r"),
            OsStr::new(volume),
        ],
    )?;
    ensure!(
        snapshots.trim().is_empty(),
        "target has snapshots; refusing"
    );
    let signatures = output_text(
        "wipefs",
        [
            OsStr::new("-n"),
            OsStr::new("--noheadings"),
            OsStr::new("--output"),
            OsStr::new("TYPE"),
            OsStr::new("--"),
            device.as_os_str(),
        ],
    )?;
    ensure!(
        signatures.trim().is_empty(),
        "target has existing signatures; refusing"
    );
    let mountpoints = output_text(
        "lsblk",
        [
            OsStr::new("-nr"),
            OsStr::new("-o"),
            OsStr::new("MOUNTPOINTS"),
            device.as_os_str(),
        ],
    )?;
    ensure!(
        mountpoints.trim().is_empty(),
        "target or a child is mounted; refusing"
    );
    let holders = Path::new("/sys/class/block")
        .join(device.file_name().unwrap_or_default())
        .join("holders");
    let has_holders = fs::read_dir(&holders)
        .with_context(|| format!("could not inspect target holders at {}", holders.display()))?
        .next()
        .is_some();
    ensure!(!has_holders, "target has active holders; refusing");
    ensure!(
        !is_exported_block_device(&device)?,
        "target is exported through iSCSI/NVMe-oF; refusing"
    );
    Ok(device)
}

fn is_exported_block_device(device: &Path) -> Result<bool> {
    for (root, filename, depth) in [
        (Path::new("/sys/kernel/config/target/core"), "udev_path", 2),
        (
            Path::new("/sys/kernel/config/nvmet/subsystems"),
            "device_path",
            3,
        ),
    ] {
        if let Some(found) = find_export(root, filename, depth, device)? {
            if found.trim() == device.to_string_lossy() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn find_export(root: &Path, filename: &str, depth: usize, device: &Path) -> Result<Option<String>> {
    if depth == 0 {
        let path = root.join(filename);
        return match fs::read_to_string(path) {
            Ok(value) if value.trim() == device.to_string_lossy() => Ok(Some(value)),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("could not inspect block device exports"),
        };
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("could not inspect block device exports"),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(found) = find_export(&entry.path(), filename, depth - 1, device)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

struct Workspace {
    root: PathBuf,
    iso_mount: PathBuf,
    payload_mount: PathBuf,
    root_mount: PathBuf,
    mounts: Vec<PathBuf>,
    distro: Option<Distro>,
    source: Option<PathBuf>,
}

impl Workspace {
    fn new() -> Result<Self> {
        let root =
            std::env::temp_dir().join(format!("diskless-linux-image-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).context("create temporary Linux image workspace")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        let iso_mount = root.join("iso");
        let payload_mount = root.join("payload");
        let root_mount = root.join("root");
        fs::create_dir(&iso_mount)?;
        fs::create_dir(&payload_mount)?;
        fs::create_dir(&root_mount)?;
        Ok(Self {
            root,
            iso_mount,
            payload_mount,
            root_mount,
            mounts: Vec::new(),
            distro: None,
            source: None,
        })
    }

    fn mount(&mut self, source: &OsStr, target: &Path, options: &[&str]) -> Result<()> {
        if options.is_empty() {
            run("mount", [source, target.as_os_str()]).context("mount Linux image filesystem")?;
        } else {
            let option_string = options.join(",");
            let args = [
                OsStr::new("-o"),
                OsStr::new(&option_string),
                source,
                target.as_os_str(),
            ];
            run("mount", args).context("mount Linux installer image")?;
        }
        self.mounts.push(target.to_path_buf());
        Ok(())
    }

    fn open_iso(&mut self, iso: &Path) -> Result<Distro> {
        let metadata = fs::symlink_metadata(iso).context("could not inspect Linux ISO")?;
        ensure!(metadata.file_type().is_file(), "ISO must be a regular file");
        self.mount(iso.as_os_str(), &self.iso_mount.clone(), &["loop", "ro"])?;
        let (distro, payload) = detect_payload(&self.iso_mount)?;
        self.mount(
            payload.as_os_str(),
            &self.payload_mount.clone(),
            &["loop", "ro"],
        )?;
        let mut source = self.payload_mount.clone();
        if let Some(nested_root) = regular_file_within(&source, Path::new("LiveOS/rootfs.img"))? {
            let mountpoint = self.root.join("fedora-root");
            fs::create_dir(&mountpoint)?;
            self.mount(nested_root.as_os_str(), &mountpoint, &["loop", "ro"])?;
            source = mountpoint;
        }
        let release_path = regular_file_within(&source, Path::new("etc/os-release"))?
            .context("could not find an installed root filesystem in the ISO payload")?;
        let release = fs::read_to_string(release_path)?;
        ensure!(
            payload_matches(distro, &release),
            "ISO payload OS does not match its live-media layout"
        );
        self.distro = Some(distro);
        self.source = Some(source);
        Ok(distro)
    }

    fn finish(self, operation: Result<()>) -> Result<()> {
        if self.distro == Some(Distro::CachyOs) && self.root_mount.join("usr/bin/gpgconf").is_file()
        {
            let _ = chroot(
                &self.root_mount,
                [
                    "gpgconf",
                    "--homedir",
                    "/etc/pacman.d/gnupg",
                    "--kill",
                    "all",
                ],
            );
        }
        let mut cleanup_error = None;
        for mount in self.mounts.iter().rev() {
            if let Err(error) = run("umount", [OsStr::new("-R"), mount.as_os_str()]) {
                cleanup_error.get_or_insert_with(|| {
                    anyhow::anyhow!("could not unmount {}: {error:#}", mount.display())
                });
            }
        }
        if cleanup_error.is_none() {
            if let Err(error) = fs::remove_dir_all(&self.root) {
                cleanup_error = Some(
                    anyhow::Error::new(error).context("remove temporary Linux image workspace"),
                );
            }
        }
        if cleanup_error.is_some() {
            eprintln!("Temporary workspace retained at {}", self.root.display());
        }
        match (operation, cleanup_error) {
            (Err(operation), Some(cleanup)) => {
                Err(operation.context(format!("cleanup also failed: {cleanup:#}")))
            }
            (Err(operation), None) => Err(operation),
            (Ok(()), Some(cleanup)) => Err(cleanup),
            (Ok(()), None) => Ok(()),
        }
    }
}

fn detect_payload(iso_root: &Path) -> Result<(Distro, PathBuf)> {
    if let Some(payload) = regular_file_within(iso_root, Path::new("casper/filesystem.squashfs"))? {
        return Ok((Distro::Ubuntu, payload));
    }
    if let Some(payload) = regular_file_within(iso_root, Path::new("casper/minimal.squashfs"))? {
        return Ok((Distro::Ubuntu, payload));
    }
    let mut pop_payloads = Vec::new();
    for entry in fs::read_dir(iso_root)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("casper_pop-os_")
        {
            if let Some(payload) = regular_file_within(
                iso_root,
                &entry
                    .path()
                    .strip_prefix(iso_root)?
                    .join("filesystem.squashfs"),
            )? {
                pop_payloads.push(payload);
            }
        }
    }
    if pop_payloads.len() == 1 {
        return Ok((Distro::Ubuntu, pop_payloads.remove(0)));
    }
    if let Some(payload) = regular_file_within(iso_root, Path::new("LiveOS/squashfs.img"))? {
        return Ok((Distro::Fedora, payload));
    }
    if let Some(payload) = regular_file_within(iso_root, Path::new("arch/x86_64/airootfs.sfs"))? {
        return Ok((Distro::CachyOs, payload));
    }
    bail!("unsupported ISO layout; expected Ubuntu/Kubuntu/Pop!_OS, Fedora/Nobara, or CachyOS live media")
}

fn regular_file_within(root: &Path, relative: &Path) -> Result<Option<PathBuf>> {
    ensure!(relative.is_relative(), "expected a relative ISO path");
    ensure!(
        relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_))),
        "invalid ISO payload path"
    );
    let canonical_root = fs::canonicalize(root)?;
    let candidate = root.join(relative);
    let canonical = match fs::canonicalize(&candidate) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspect ISO payload {}", relative.display()))
        }
    };
    ensure!(
        canonical.starts_with(&canonical_root),
        "ISO payload escapes its mounted filesystem"
    );
    ensure!(
        fs::symlink_metadata(&canonical)?.file_type().is_file(),
        "ISO payload is not a regular file"
    );
    Ok(Some(canonical))
}

fn payload_matches(distro: Distro, os_release: &str) -> bool {
    let id = os_release.lines().find_map(|line| {
        line.strip_prefix("ID=")
            .map(|value| value.trim_matches('"').to_ascii_lowercase())
    });
    id.as_deref().and_then(Distro::from_payload_name) == Some(distro)
        || match distro {
            Distro::Ubuntu => {
                os_release.to_ascii_lowercase().contains("ubuntu") || os_release.contains("Pop!_OS")
            }
            Distro::Fedora => {
                os_release.to_ascii_lowercase().contains("fedora") || os_release.contains("Nobara")
            }
            Distro::CachyOs => {
                os_release.to_ascii_lowercase().contains("cachyos") || id.as_deref() == Some("arch")
            }
        }
}

fn build_image(
    workspace: &mut Workspace,
    distro: Distro,
    volume: &str,
    device: &Path,
    account: &LinuxImageAccount,
) -> Result<()> {
    let efi = PathBuf::from(format!("{}p1", device.display()));
    let root = PathBuf::from(format!("{}p2", device.display()));
    run(
        "parted",
        [
            OsStr::new("-s"),
            OsStr::new("-a"),
            OsStr::new("optimal"),
            device.as_os_str(),
            OsStr::new("mklabel"),
            OsStr::new("gpt"),
        ],
    )?;
    run(
        "parted",
        [
            OsStr::new("-s"),
            OsStr::new("-a"),
            OsStr::new("optimal"),
            device.as_os_str(),
            OsStr::new("mkpart"),
            OsStr::new("ESP"),
            OsStr::new("fat32"),
            OsStr::new("32MiB"),
            OsStr::new("1056MiB"),
        ],
    )?;
    run(
        "parted",
        [
            OsStr::new("-s"),
            device.as_os_str(),
            OsStr::new("set"),
            OsStr::new("1"),
            OsStr::new("esp"),
            OsStr::new("on"),
        ],
    )?;
    run(
        "parted",
        [
            OsStr::new("-s"),
            OsStr::new("-a"),
            OsStr::new("optimal"),
            device.as_os_str(),
            OsStr::new("mkpart"),
            OsStr::new("root"),
            OsStr::new("ext4"),
            OsStr::new("1056MiB"),
            OsStr::new("100%"),
        ],
    )?;
    run("partprobe", [device.as_os_str()])?;
    run("udevadm", [OsStr::new("settle")])?;
    ensure!(
        is_block_device(&efi) && is_block_device(&root),
        "kernel did not expose expected ZVOL partition devices"
    );
    run(
        "mkfs.fat",
        [
            OsStr::new("-F"),
            OsStr::new("32"),
            OsStr::new("-n"),
            OsStr::new("DISK_EFI"),
            efi.as_os_str(),
        ],
    )?;
    run(
        "mkfs.ext4",
        [
            OsStr::new("-F"),
            OsStr::new("-L"),
            OsStr::new("DISK_ROOT"),
            root.as_os_str(),
        ],
    )?;
    workspace.mount(root.as_os_str(), &workspace.root_mount.clone(), &[])?;
    let efi_mount = workspace.root_mount.join("boot/efi");
    fs::create_dir_all(&efi_mount)?;
    workspace.mount(efi.as_os_str(), &efi_mount, &[])?;
    copy_rootfs(
        workspace
            .source
            .as_deref()
            .context("missing ISO root filesystem")?,
        &workspace.root_mount,
    )?;
    prepare_chroot(workspace)?;
    install_boot_packages(workspace, distro)?;
    configure_boot(workspace, distro, volume, &root, &efi, account)?;
    Ok(())
}

fn copy_rootfs(source: &Path, destination: &Path) -> Result<()> {
    run(
        "rsync",
        [
            OsStr::new("-aHAXx"),
            OsStr::new("--numeric-ids"),
            OsStr::new("--info=progress2"),
            OsStr::new("--exclude=/dev/*"),
            OsStr::new("--exclude=/proc/*"),
            OsStr::new("--exclude=/sys/*"),
            OsStr::new("--exclude=/run/*"),
            OsStr::new("--exclude=/tmp/*"),
            OsStr::new("--exclude=/boot/efi/***"),
            OsStr::new("--exclude=/etc/machine-id"),
            OsStr::new("--exclude=/var/lib/dbus/machine-id"),
            OsStr::new("--exclude=/etc/ssh/ssh_host_*"),
            OsStr::new("--"),
            OsStr::new(&format!("{}/", source.display())),
            OsStr::new(&format!("{}/", destination.display())),
        ],
    )
    .context("copy live root filesystem")
}

fn prepare_chroot(workspace: &mut Workspace) -> Result<()> {
    let root = workspace.root_mount.clone();
    for path in ["run", "dev", "proc", "sys"] {
        checked_directory(&root, Path::new(path), true)?;
    }
    run(
        "mount",
        [
            OsStr::new("-t"),
            OsStr::new("tmpfs"),
            OsStr::new("tmpfs"),
            root.join("run").as_os_str(),
        ],
    )?;
    workspace.mounts.push(root.join("run"));
    run(
        "mount",
        [
            OsStr::new("--rbind"),
            OsStr::new("/dev"),
            root.join("dev").as_os_str(),
        ],
    )?;
    workspace.mounts.push(root.join("dev"));
    run(
        "mount",
        [OsStr::new("--make-rslave"), root.join("dev").as_os_str()],
    )?;
    run(
        "mount",
        [
            OsStr::new("-t"),
            OsStr::new("proc"),
            OsStr::new("proc"),
            root.join("proc").as_os_str(),
        ],
    )?;
    workspace.mounts.push(root.join("proc"));
    run(
        "mount",
        [
            OsStr::new("-t"),
            OsStr::new("sysfs"),
            OsStr::new("-o"),
            OsStr::new("ro"),
            OsStr::new("sysfs"),
            root.join("sys").as_os_str(),
        ],
    )?;
    workspace.mounts.push(root.join("sys"));
    let host_resolv = Path::new("/etc/resolv.conf");
    if host_resolv.exists() {
        let content = fs::read(host_resolv).context("read host resolver configuration")?;
        remove_file_under(&root, Path::new("etc/resolv.conf"))?;
        write_file_under(&root, Path::new("etc/resolv.conf"), &content)?;
    }
    Ok(())
}

fn install_boot_packages(workspace: &Workspace, distro: Distro) -> Result<()> {
    let root = &workspace.root_mount;
    match distro {
        Distro::Fedora => chroot(
            root,
            [
                "dnf",
                "-y",
                "install",
                "dracut",
                "dracut-network",
                "iscsi-initiator-utils",
                "grub2-tools-extra",
                "grub2-efi-x64-modules",
                "NetworkManager",
                "openssh-server",
                "sudo",
                "kernel-core",
            ],
        ),
        Distro::Ubuntu => {
            disable_cdrom_apt_sources(root)?;
            chroot(root, ["apt-get", "update"])?;
            chroot_env(
                root,
                "DEBIAN_FRONTEND",
                "noninteractive",
                [
                    "apt-get",
                    "install",
                    "-y",
                    "dracut-core",
                    "dracut-network",
                    "open-iscsi",
                    "grub-efi-amd64-bin",
                    "grub-common",
                    "network-manager",
                    "openssh-server",
                    "sudo",
                    "linux-image-generic",
                ],
            )
        }
        Distro::CachyOs => {
            chroot(root, ["pacman-key", "--init"])?;
            chroot(root, ["pacman-key", "--populate"])?;
            chroot(
                root,
                [
                    "pacman",
                    "-Syu",
                    "--noconfirm",
                    "dracut",
                    "open-iscsi",
                    "grub",
                    "networkmanager",
                    "openssh",
                    "sudo",
                    "linux-cachyos-lts",
                ],
            )
        }
    }
}

fn disable_cdrom_apt_sources(root: &Path) -> Result<()> {
    let Some(sources_directory) =
        checked_directory(root, Path::new("etc/apt/sources.list.d"), false)?
    else {
        return Ok(());
    };
    for entry in fs::read_dir(sources_directory)? {
        let path = entry?.path();
        let relative = path.strip_prefix(root)?;
        let Some(safe_path) = checked_file(root, relative)? else {
            continue;
        };
        let extension = safe_path.extension().and_then(OsStr::to_str);
        let content = fs::read_to_string(&safe_path)?;
        let updated = match extension {
            Some("list") => disable_cdrom_legacy(&content),
            Some("sources") => disable_cdrom_deb822(&content),
            _ => continue,
        };
        write_file_under(root, relative, updated.as_bytes())?;
    }
    if let Some(legacy) = checked_file(root, Path::new("etc/apt/sources.list"))? {
        let content = fs::read_to_string(legacy)?;
        let updated = disable_cdrom_legacy(&content);
        write_file_under(root, Path::new("etc/apt/sources.list"), updated.as_bytes())?;
    }
    Ok(())
}

fn disable_cdrom_legacy(content: &str) -> String {
    content
        .split_inclusive('\n')
        .map(|line| {
            let source_type = line.split_whitespace().next();
            let lower = line.to_ascii_lowercase();
            if matches!(source_type, Some("deb" | "deb-src")) && contains_cdrom_uri(&lower) {
                format!("# {line}")
            } else {
                line.to_owned()
            }
        })
        .collect()
}

fn disable_cdrom_deb822(content: &str) -> String {
    content
        .split_inclusive("\n\n")
        .map(|stanza| {
            let is_cdrom = stanza.lines().any(|line| {
                line.to_ascii_lowercase().starts_with("uris:")
                    && contains_cdrom_uri(&line.to_ascii_lowercase())
            });
            if !is_cdrom {
                return stanza.to_owned();
            }
            let mut lines = stanza
                .trim_end_matches('\n')
                .lines()
                .filter(|line| !line.to_ascii_lowercase().starts_with("enabled:"))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            lines.push("Enabled: no".to_owned());
            let mut result = lines.join("\n");
            if stanza.ends_with("\n\n") {
                result.push_str("\n\n");
            } else if stanza.ends_with('\n') {
                result.push('\n');
            }
            result
        })
        .collect()
}

fn contains_cdrom_uri(value: &str) -> bool {
    if value.contains("cdrom:") {
        return true;
    }
    value.match_indices("file:").any(|(index, _)| {
        value[index + "file:".len()..]
            .trim_start_matches('/')
            .starts_with("cdrom")
    })
}

fn checked_directory(root: &Path, relative: &Path, create: bool) -> Result<Option<PathBuf>> {
    validate_relative_image_path(relative)?;
    let root_metadata = fs::symlink_metadata(root)?;
    ensure!(
        root_metadata.is_dir() && !root_metadata.file_type().is_symlink(),
        "image root is not a real directory"
    );
    let mut path = root.to_path_buf();
    for component in relative.components() {
        path.push(component.as_os_str());
        match fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "image path contains a non-directory or symlink: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                fs::create_dir(&path)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspect image directory {}", path.display()))
            }
        }
    }
    Ok(Some(path))
}

fn checked_file(root: &Path, relative: &Path) -> Result<Option<PathBuf>> {
    validate_relative_image_path(relative)?;
    let parent = relative.parent().unwrap_or_else(|| Path::new("."));
    let Some(parent) = checked_directory(root, parent, false)? else {
        return Ok(None);
    };
    let name = relative.file_name().context("image file name is missing")?;
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(Some(path)),
        Ok(_) => bail!("image file is not a regular file: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("inspect image file {}", path.display())),
    }
}

fn read_file_under(root: &Path, relative: &Path) -> Result<Vec<u8>> {
    let path = checked_file(root, relative)?
        .with_context(|| format!("image file is missing: {}", relative.display()))?;
    fs::read(path).with_context(|| format!("read image file {}", relative.display()))
}

fn write_file_under(root: &Path, relative: &Path, content: &[u8]) -> Result<()> {
    validate_relative_image_path(relative)?;
    let parent_relative = relative.parent().unwrap_or_else(|| Path::new("."));
    let parent = checked_directory(root, parent_relative, true)?
        .context("create image file parent directory")?;
    let destination = parent.join(relative.file_name().context("image file name is missing")?);
    if let Some(existing) = checked_file(root, relative)? {
        ensure!(
            existing == destination,
            "image file path changed unexpectedly"
        );
    } else if fs::symlink_metadata(&destination).is_ok() {
        bail!(
            "refusing to replace a non-regular image file: {}",
            destination.display()
        );
    }
    let temporary = parent.join(format!(".diskless-{}.tmp", uuid::Uuid::new_v4()));
    let write_result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content)?;
        fs::rename(&temporary, &destination)?;
        Ok::<_, anyhow::Error>(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result.with_context(|| format!("write image file {}", relative.display()))
}

fn configure_ssh_host_key_generation(root: &Path, ssh_service: &str) -> Result<()> {
    let keygen_unit = concat!(
        "[Unit]\n",
        "Description=Generate unique SSH host keys for this diskless client\n",
        "Before=ssh.service sshd.service sshd-keygen.service\n",
        "\n[Service]\n",
        "Type=oneshot\n",
        "ExecStart=/usr/bin/ssh-keygen -A\n",
    );
    write_file_under(
        root,
        Path::new("etc/systemd/system/diskless-manager-ssh-hostkeys.service"),
        keygen_unit.as_bytes(),
    )?;

    let drop_in_path = PathBuf::from(format!(
        "etc/systemd/system/{ssh_service}.d/10-diskless-manager-hostkeys.conf"
    ));
    let drop_in = concat!(
        "[Unit]\n",
        "Requires=diskless-manager-ssh-hostkeys.service\n",
        "After=diskless-manager-ssh-hostkeys.service\n",
    );
    write_file_under(root, &drop_in_path, drop_in.as_bytes())
}

fn remove_file_under(root: &Path, relative: &Path) -> Result<()> {
    validate_relative_image_path(relative)?;
    let parent_relative = relative.parent().unwrap_or_else(|| Path::new("."));
    let Some(parent) = checked_directory(root, parent_relative, false)? else {
        return Ok(());
    };
    let path = parent.join(relative.file_name().context("image file name is missing")?);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            bail!(
                "refusing to remove an image directory as a file: {}",
                path.display()
            )
        }
        Ok(_) => fs::remove_file(&path)
            .with_context(|| format!("remove image file {}", relative.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("inspect image file {}", path.display())),
    }
}

fn remove_dir_under(root: &Path, relative: &Path) -> Result<()> {
    validate_relative_image_path(relative)?;
    let parent_relative = relative.parent().unwrap_or_else(|| Path::new("."));
    let Some(parent) = checked_directory(root, parent_relative, false)? else {
        return Ok(());
    };
    let path = parent.join(
        relative
            .file_name()
            .context("image directory name is missing")?,
    );
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(&path)
                .with_context(|| format!("remove image directory {}", relative.display()))
        }
        Ok(_) => fs::remove_file(&path)
            .with_context(|| format!("remove image entry {}", relative.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("inspect image directory {}", path.display()))
        }
    }
}

fn resolve_file_under(root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_image_path(relative)?;
    let root = fs::canonicalize(root)?;
    let path = fs::canonicalize(root.join(relative))
        .with_context(|| format!("resolve image file {}", relative.display()))?;
    ensure!(path.starts_with(&root), "image path escapes the image root");
    ensure!(
        fs::metadata(&path)?.is_file(),
        "image path is not a regular file"
    );
    Ok(path)
}

fn resolve_directory_under(root: &Path, relative: &Path) -> Result<PathBuf> {
    validate_relative_image_path(relative)?;
    let root = fs::canonicalize(root)?;
    let path = fs::canonicalize(root.join(relative))
        .with_context(|| format!("resolve image directory {}", relative.display()))?;
    ensure!(path.starts_with(&root), "image path escapes the image root");
    ensure!(
        fs::metadata(&path)?.is_dir(),
        "image path is not a directory"
    );
    Ok(path)
}

fn validate_relative_image_path(relative: &Path) -> Result<()> {
    ensure!(relative.is_relative(), "expected a relative image path");
    ensure!(
        relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_))),
        "invalid image path"
    );
    Ok(())
}

fn configure_boot(
    workspace: &Workspace,
    distro: Distro,
    volume: &str,
    root: &Path,
    efi: &Path,
    account: &LinuxImageAccount,
) -> Result<()> {
    let mount = &workspace.root_mount;
    let root_uuid = output_text(
        "blkid",
        [
            OsStr::new("-s"),
            OsStr::new("UUID"),
            OsStr::new("-o"),
            OsStr::new("value"),
            root.as_os_str(),
        ],
    )?;
    let efi_uuid = output_text(
        "blkid",
        [
            OsStr::new("-s"),
            OsStr::new("UUID"),
            OsStr::new("-o"),
            OsStr::new("value"),
            efi.as_os_str(),
        ],
    )?;
    let root_uuid = root_uuid.trim();
    let efi_uuid = efi_uuid.trim();
    let fstab = format!("UUID={root_uuid} / ext4 defaults 0 1\nUUID={efi_uuid} /boot/efi vfat defaults,umask=0077 0 2\n");
    write_file_under(mount, Path::new("etc/fstab"), fstab.as_bytes())?;
    let grub_cfg = format!(
        "search --no-floppy --fs-uuid --set=root {efi_uuid}\nset timeout=3\nset default=0\nmenuentry 'Diskless Linux (iSCSI)' {{\n  search --no-floppy --fs-uuid --set=root {root_uuid}\n  linux /boot/diskless-vmlinuz root=UUID={root_uuid} ip=ibft rd.neednet=1 netroot=iscsi rd.iscsi.firmware=1 rw\n  initrd /boot/diskless-initramfs.img\n}}\n"
    );
    checked_directory(mount, Path::new("boot/grub"), true)?;
    write_file_under(mount, Path::new("boot/grub/grub.cfg"), grub_cfg.as_bytes())?;
    let grub_binary = if resolve_file_under(mount, Path::new("usr/bin/grub2-mkstandalone")).is_ok()
    {
        "/usr/bin/grub2-mkstandalone"
    } else if resolve_file_under(mount, Path::new("usr/bin/grub-mkstandalone")).is_ok() {
        "/usr/bin/grub-mkstandalone"
    } else {
        bail!("grub-mkstandalone is missing")
    };
    checked_directory(mount, Path::new("boot/efi/EFI/BOOT"), true)?;
    chroot(
        mount,
        [
            grub_binary,
            "-O",
            "x86_64-efi",
            "-o",
            "/boot/efi/EFI/BOOT/BOOTX64.EFI",
            "--modules=part_gpt fat ext2 search search_fs_uuid normal linux",
            "boot/grub/grub.cfg=/boot/grub/grub.cfg",
        ],
    )?;
    if distro == Distro::Fedora {
        checked_directory(mount, Path::new("boot/efi/EFI/fedora"), true)?;
        let binary = read_file_under(mount, Path::new("boot/efi/EFI/BOOT/BOOTX64.EFI"))?;
        write_file_under(mount, Path::new("boot/efi/EFI/fedora/grubx64.efi"), &binary)?;
    }
    chroot(mount, ["systemctl", "enable", "NetworkManager.service"])?;
    chroot(
        mount,
        [
            "systemctl",
            "mask",
            "--force",
            "systemd-networkd.service",
            "systemd-networkd.socket",
            "systemd-networkd-wait-online.service",
        ],
    )?;
    if checked_directory(mount, Path::new("etc/iscsi"), false)?.is_some() {
        for name in ["nodes", "send_targets"] {
            remove_dir_under(mount, &Path::new("etc/iscsi").join(name))?;
        }
    }
    chroot(
        mount,
        [
            "useradd",
            "--create-home",
            "--shell",
            "/bin/bash",
            &account.username,
        ],
    )?;
    let password_input = format!("{}:{}\n", account.username, account.password);
    chpasswd(mount, &password_input)?;
    if account.sudo_user {
        checked_directory(mount, Path::new("etc/sudoers.d"), true)?;
        let sudoers_path = Path::new("etc/sudoers.d").join(&account.username);
        let sudoers = format!("{} ALL=(ALL:ALL) ALL\n", account.username);
        write_file_under(mount, &sudoers_path, sudoers.as_bytes())?;
        run(
            "chmod",
            [OsStr::new("440"), mount.join(&sudoers_path).as_os_str()],
        )?;
        chroot(mount, ["visudo", "--check"])?;
    }
    build_initramfs(mount, distro)?;
    let ssh_service = if distro == Distro::Ubuntu {
        "ssh.service"
    } else {
        "sshd.service"
    };
    configure_ssh_host_key_generation(mount, ssh_service)?;
    chroot(mount, ["systemctl", "enable", ssh_service])?;
    run(
        "zfs",
        [
            OsStr::new("set"),
            OsStr::new("org.diskless:os=linux"),
            OsStr::new(volume),
        ],
    )?;
    for path in [
        Path::new("etc/machine-id"),
        Path::new("var/lib/dbus/machine-id"),
    ] {
        remove_file_under(mount, path)?;
    }
    if let Some(ssh_dir) = checked_directory(mount, Path::new("etc/ssh"), false)? {
        for entry in fs::read_dir(ssh_dir)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("ssh_host_"))
            {
                remove_file_under(mount, &Path::new("etc/ssh").join(entry.file_name()))?;
            }
        }
    }
    write_file_under(mount, Path::new("etc/machine-id"), b"")?;
    run("sync", std::iter::empty::<&OsStr>())?;
    Ok(())
}

fn build_initramfs(root: &Path, distro: Distro) -> Result<()> {
    let modules = resolve_directory_under(root, Path::new("usr/lib/modules"))
        .or_else(|_| resolve_directory_under(root, Path::new("lib/modules")))
        .context("installed kernel modules are missing")?;
    let mut versions = fs::read_dir(&modules)?
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir())
                .map(|_| entry.file_name().to_string_lossy().into_owned())
        })
        .filter(|version| distro != Distro::CachyOs || version.contains("cachyos-lts"))
        .collect::<Vec<_>>();
    versions.sort_by(|left, right| natural_version_cmp(left, right));
    let version = versions
        .pop()
        .context("no installed kernel modules found")?;
    let kernel = if distro == Distro::CachyOs {
        resolve_file_under(root, Path::new("boot/vmlinuz-linux-cachyos-lts"))?
    } else {
        resolve_file_under(root, Path::new(&format!("boot/vmlinuz-{version}")))?
    };
    write_file_under(root, Path::new("boot/diskless-vmlinuz"), &fs::read(kernel)?)?;
    let drivers_root = modules.join(&version).join("kernel/drivers/net/ethernet");
    let mut network_drivers = Vec::new();
    for entry in walk_files(&drivers_root)? {
        let name = entry
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();
        if name.contains(".ko") {
            network_drivers.push(name.split(".ko").next().unwrap_or(name).to_owned());
        }
    }
    let mut args = vec![
        "dracut".to_owned(),
        "--force".to_owned(),
        "--no-hostonly".to_owned(),
        "--no-hostonly-cmdline".to_owned(),
    ];
    if resolve_file_under(root, Path::new("usr/lib/open-iscsi/startup-checks.sh")).is_ok() {
        args.extend([
            "--install".to_owned(),
            "/usr/lib/open-iscsi/startup-checks.sh /etc/iscsi/iscsid.conf /etc/iscsi/initiatorname.iscsi /usr/sbin/iscsi-iname".to_owned(),
        ]);
    }
    args.extend([
        "--add".to_owned(),
        "network-manager network iscsi".to_owned(),
        "--omit".to_owned(),
        "systemd-networkd".to_owned(),
        "--add-drivers".to_owned(),
        format!("iscsi_tcp iscsi_ibft {}", network_drivers.join(" ")),
        "/boot/diskless-initramfs.img".to_owned(),
        version,
    ]);
    chroot_args(root, args)
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry.file_type()?.is_file() {
                paths.push(entry.path());
            }
        }
    }
    Ok(paths)
}

fn natural_version_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let left = left
        .split(|character: char| !character.is_ascii_digit())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect::<Vec<_>>();
    let right = right
        .split(|character: char| !character.is_ascii_digit())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect::<Vec<_>>();
    left.cmp(&right)
}

fn chpasswd(root: &Path, input: &str) -> Result<()> {
    let mut child = Command::new("chroot")
        .arg(root)
        .args(["chpasswd"])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("start chpasswd in image root")?;
    child
        .stdin
        .take()
        .context("chpasswd stdin unavailable")?
        .write_all(input.as_bytes())
        .context("send initial Linux image account password")?;
    let status = child.wait().context("wait for chpasswd")?;
    ensure!(status.success(), "chpasswd failed ({status})");
    Ok(())
}

fn chroot<const N: usize>(root: &Path, arguments: [&str; N]) -> Result<()> {
    chroot_args(root, arguments.into_iter().map(str::to_owned).collect())
}

fn chroot_env<const N: usize>(
    root: &Path,
    key: &str,
    value: &str,
    arguments: [&str; N],
) -> Result<()> {
    let mut command = Command::new("chroot");
    command
        .arg(root)
        .arg("env")
        .arg(format!("{key}={value}"))
        .args(arguments);
    run_command(command).context("run command in Linux image root")
}

fn chroot_args(root: &Path, arguments: Vec<String>) -> Result<()> {
    let mut command = Command::new("chroot");
    command.arg(root).args(arguments);
    run_command(command).context("run command in Linux image root")
}

fn is_block_device(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.file_type().is_block_device())
}

fn run<I, S>(program: &str, arguments: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(arguments);
    run_command(command)
}

fn output_text<I, S>(program: &str, arguments: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program)
        .args(arguments)
        .output()
        .with_context(|| format!("start {program}"))?;
    ensure_command_success(program, &output)?;
    String::from_utf8(output.stdout).with_context(|| format!("{program} returned non-UTF-8 output"))
}

fn run_command(mut command: Command) -> Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command
        .status()
        .with_context(|| format!("start {program}"))?;
    ensure!(status.success(), "{program} failed ({status})");
    Ok(())
}

fn ensure_command_success(program: &str, output: &Output) -> Result<()> {
    ensure!(
        output.status.success(),
        "{program} failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        detect_payload, disable_cdrom_deb822, disable_cdrom_legacy, natural_version_cmp,
        parse_request, payload_matches, validate_account, write_file_under, Distro,
        LinuxImageAccount, Request,
    };
    use std::{cmp::Ordering, fs, path::Path};

    #[test]
    fn preflight_only_accepts_supported_size_and_required_iso() {
        let valid = ["--check", "--iso", "/tmp/linux.iso", "--size-gb", "20"].map(str::to_owned);
        assert!(matches!(
            parse_request(&valid),
            Ok(Request::Check {
                size_gb: Some(20),
                volume: None,
                ..
            })
        ));
        assert!(parse_request(
            &["--check", "--iso", "/tmp/linux.iso", "--size-gb", "19"].map(str::to_owned)
        )
        .is_err());
        assert!(parse_request(&["--build", "--iso", "/tmp/linux.iso"].map(str::to_owned)).is_err());
    }

    #[test]
    fn target_names_reject_options_and_snapshots() {
        assert!(parse_request(
            &[
                "--build",
                "--iso",
                "/tmp/linux.iso",
                "--volume",
                "tank/images/test"
            ]
            .map(str::to_owned)
        )
        .is_ok());
        assert!(parse_request(
            &["--build", "--iso", "/tmp/linux.iso", "--volume", "-f"].map(str::to_owned)
        )
        .is_err());
        assert!(parse_request(
            &[
                "--build",
                "--iso",
                "/tmp/linux.iso",
                "--volume",
                "tank/image@test"
            ]
            .map(str::to_owned)
        )
        .is_err());
    }

    #[test]
    fn ubuntu_media_source_stanzas_are_disabled_without_touching_online_sources() {
        let content = "Types: deb\nURIs: file:///cdrom\nSuites: noble\nEnabled: yes\n\nTypes: deb\nURIs: https://archive.ubuntu.com/ubuntu\nSuites: noble\n";
        let result = disable_cdrom_deb822(content);
        assert!(
            result.contains("URIs: file:///cdrom\nSuites: noble\nEnabled: no"),
            "{result:?}"
        );
        assert!(result.contains("URIs: https://archive.ubuntu.com/ubuntu"));
        assert!(disable_cdrom_legacy(
            "deb cdrom:[Ubuntu] noble main\ndeb https://archive.ubuntu.com noble main\n"
        )
        .starts_with("# deb cdrom:[Ubuntu] noble main\n"));
        assert_eq!(natural_version_cmp("6.10.0", "6.9.0"), Ordering::Greater);
    }

    #[test]
    fn live_iso_layouts_are_selected_and_verified_by_payload_os() {
        let root =
            std::env::temp_dir().join(format!("linux-image-layout-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("casper")).unwrap();
        fs::write(root.join("casper/minimal.squashfs"), []).unwrap();
        assert_eq!(
            detect_payload(&root).unwrap(),
            (Distro::Ubuntu, root.join("casper/minimal.squashfs"))
        );
        assert!(payload_matches(Distro::Ubuntu, "ID=pop\nNAME=Pop!_OS"));
        assert!(payload_matches(Distro::Fedora, "ID=nobara\nNAME=Nobara"));
        assert!(payload_matches(Distro::CachyOs, "ID=cachyos\nNAME=CachyOS"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preflight_rejects_ambiguous_pop_payloads() {
        let root =
            std::env::temp_dir().join(format!("linux-image-pop-layout-{}", uuid::Uuid::new_v4()));
        for release in ["one", "two"] {
            let payload = root.join(format!("casper_pop-os_{release}/filesystem.squashfs"));
            fs::create_dir_all(payload.parent().unwrap()).unwrap();
            fs::write(payload, []).unwrap();
        }
        assert!(detect_payload(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn account_settings_accept_a_named_user_and_reject_root_or_invalid_passwords() {
        let account = LinuxImageAccount {
            username: "ops-user".into(),
            password: "safe test password".into(),
            sudo_user: true,
        };
        assert!(validate_account(&account).is_ok());
        assert!(validate_account(&LinuxImageAccount {
            username: "root".into(),
            password: "safe".into(),
            sudo_user: true,
        })
        .is_err());
        assert!(validate_account(&LinuxImageAccount {
            username: "ops-user".into(),
            password: "bad\npassword".into(),
            sudo_user: false,
        })
        .is_err());
    }

    #[test]
    fn extracted_iso_symlinks_cannot_write_outside_the_image_root() {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!("linux-image-path-{}", uuid::Uuid::new_v4()));
        let root = base.join("root");
        let outside = base.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        symlink(&outside, root.join("etc")).unwrap();

        assert!(write_file_under(&root, Path::new("etc/marker"), b"not written").is_err());
        assert!(!outside.join("marker").exists());
        fs::remove_dir_all(base).unwrap();
    }
}
