use super::driver_manifest::DriverManifest;
use super::windows_inf::{inspect_inf_file, WindowsInfMetadata};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

const MEDIA_DIR: &str = "pxe/network-drivers";
const DRIVERS_DIR: &str = "drivers";
const CATALOG_FILE: &str = "index.json";
const MANIFEST_FILE: &str = "manifest.json";
const ISO_FILE: &str = "network-drivers.iso";
const STARTNET_FILE: &str = "startnet.cmd";
const TAG_FILE: &str = "DRIVERS.TAG";
const VOLUME_LABEL: &str = "DISKLESS_DRIVERS";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedDriverMetadata {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub driver_name: Option<String>,
    #[serde(default)]
    pub pnp_device_id: Option<String>,
    #[serde(default)]
    pub guid: Option<String>,
    #[serde(default)]
    pub mac_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDriverPackage {
    pub id: String,
    pub name: String,
    pub service_name: Option<String>,
    pub driver_name: Option<String>,
    pub pnp_device_id: Option<String>,
    pub guid: Option<String>,
    pub mac_address: Option<String>,
    pub inf_files: Vec<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub architectures: Vec<String>,
    #[serde(default)]
    pub hardware_ids: Vec<String>,
    #[serde(default)]
    pub compatible_ids: Vec<String>,
    #[serde(default)]
    pub service_names: Vec<String>,
    #[serde(default)]
    pub catalog_files: Vec<String>,
    pub imported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DriverCatalog {
    pub version: u32,
    pub drivers: Vec<NetworkDriverPackage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriverInjectionStatus {
    pub driver_count: usize,
    pub inf_count: usize,
    pub iso_path: String,
    pub startnet_path: String,
    pub iso_ready: bool,
}

#[derive(Debug, Clone)]
pub struct NetworkDriverInjectionPlugin {
    root_dir: PathBuf,
}

impl NetworkDriverInjectionPlugin {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    fn media_dir(&self) -> PathBuf {
        self.root_dir.join(MEDIA_DIR)
    }

    fn drivers_dir(&self) -> PathBuf {
        self.media_dir().join(DRIVERS_DIR)
    }

    fn catalog_path(&self) -> PathBuf {
        self.media_dir().join(CATALOG_FILE)
    }

    fn iso_path(&self) -> PathBuf {
        self.media_dir().join(ISO_FILE)
    }

    fn startnet_path(&self) -> PathBuf {
        self.media_dir().join(STARTNET_FILE)
    }

    pub fn package_directory(&self, id: &str) -> Result<PathBuf> {
        let parsed = Uuid::parse_str(id)
            .with_context(|| format!("invalid network driver package id '{id}'"))?;
        let canonical_id = parsed.to_string();
        let package_dir = self.drivers_dir().join(&canonical_id);
        if !package_dir.is_dir() {
            bail!("network driver package '{}' was not found", canonical_id);
        }
        Ok(package_dir)
    }

    pub fn status(&self) -> Result<DriverInjectionStatus> {
        let catalog = self.load_catalog()?;
        let inf_count = catalog
            .drivers
            .iter()
            .map(|driver| driver.inf_files.len())
            .sum();

        Ok(DriverInjectionStatus {
            driver_count: catalog.drivers.len(),
            inf_count,
            iso_path: self.iso_path().display().to_string(),
            startnet_path: self.startnet_path().display().to_string(),
            iso_ready: self.iso_path().is_file() && self.startnet_path().is_file(),
        })
    }

    pub fn list(&self) -> Result<Vec<NetworkDriverPackage>> {
        Ok(self.load_catalog()?.drivers)
    }

    pub fn import_zip(&self, source: impl AsRef<Path>) -> Result<NetworkDriverPackage> {
        let source = source.as_ref();
        if !source.is_file() {
            bail!("driver archive does not exist: {}", source.display());
        }

        fs::create_dir_all(self.drivers_dir())?;

        let id = Uuid::new_v4().to_string();
        let package_dir = self.drivers_dir().join(&id);
        let temp_dir = self.media_dir().join(format!(".import-{}", id));

        fs::create_dir_all(&temp_dir)?;

        let result = (|| -> Result<NetworkDriverPackage> {
            validate_zip_paths(source)?;
            extract_zip(source, &temp_dir)?;

            let metadata = find_metadata(&temp_dir)?;
            let package_source = find_driver_package(&temp_dir)?;
            let inf_files = find_inf_files(&package_source)?;

            if inf_files.is_empty() {
                bail!("archive contains no INF driver package");
            }

            let inspections = inf_files
                .iter()
                .map(|inf| inspect_inf_file(inf))
                .collect::<Result<Vec<_>>>()?;
            let network_infs = inspections
                .iter()
                .filter(|inspection| inspection.is_network_class)
                .collect::<Vec<_>>();
            if network_infs.is_empty() {
                bail!("archive contains INF files but none identify as network adapter drivers");
            }
            // A signed Windows package must reference a catalog file and ship
            // it alongside the INF; unsigned or damaged packages cannot enter
            // the catalog. DISM signature verification remains the install gate.
            require_package_catalog(&package_source, &inf_files, &inspections)?;

            fs::create_dir_all(&package_dir)?;
            copy_directory_contents(&package_source, &package_dir)?;

            let relative_inf_files = inf_files
                .iter()
                .map(|path| {
                    path.strip_prefix(&package_source)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| {
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        })
                })
                .collect::<Vec<_>>();

            let mut architectures = network_infs
                .iter()
                .flat_map(|inspection| inspection.architectures.iter().cloned())
                .collect::<Vec<_>>();
            let mut hardware_ids = network_infs
                .iter()
                .flat_map(|inspection| inspection.hardware_ids.iter().cloned())
                .collect::<Vec<_>>();
            let mut compatible_ids = network_infs
                .iter()
                .flat_map(|inspection| inspection.compatible_ids.iter().cloned())
                .collect::<Vec<_>>();
            let mut service_names = network_infs
                .iter()
                .flat_map(|inspection| inspection.service_names.iter().cloned())
                .collect::<Vec<_>>();
            let mut catalog_files = network_infs
                .iter()
                .flat_map(|inspection| inspection.catalog_files.iter().cloned())
                .collect::<Vec<_>>();
            for values in [
                &mut architectures,
                &mut hardware_ids,
                &mut compatible_ids,
                &mut service_names,
                &mut catalog_files,
            ] {
                values.sort_by_key(|value| value.to_ascii_lowercase());
                values.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            }

            let inferred_service = service_names.first().cloned();
            let inferred_pnp = hardware_ids.first().cloned();
            let inferred_provider = network_infs.iter().find_map(|item| item.provider.clone());
            let inferred_version = network_infs.iter().find_map(|item| item.version.clone());

            let package = NetworkDriverPackage {
                id,
                name: metadata
                    .name
                    .or(metadata.driver_name.clone())
                    .or(metadata.service_name.clone())
                    .or(inferred_provider.clone())
                    .unwrap_or_else(|| "Network Driver".to_string()),
                service_name: metadata.service_name.or(inferred_service),
                driver_name: metadata.driver_name,
                pnp_device_id: metadata.pnp_device_id.or(inferred_pnp),
                guid: metadata.guid,
                mac_address: metadata.mac_address,
                inf_files: relative_inf_files,
                provider: inferred_provider,
                version: inferred_version,
                architectures,
                hardware_ids,
                compatible_ids,
                service_names,
                catalog_files,
                imported_at: Utc::now(),
            };

            let mut catalog = self.load_catalog()?;
            catalog.drivers.retain(|existing| existing.id != package.id);
            catalog.drivers.push(package.clone());
            catalog
                .drivers
                .sort_by_key(|driver| driver.name.to_lowercase());
            self.save_catalog(&catalog)?;
            self.rebuild_media(&catalog)?;

            Ok(package)
        })();

        let _ = fs::remove_dir_all(&temp_dir);

        if result.is_err() {
            let _ = fs::remove_dir_all(&package_dir);
        }

        result
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut catalog = self.load_catalog()?;
        let before = catalog.drivers.len();
        catalog.drivers.retain(|driver| driver.id != id);

        if catalog.drivers.len() == before {
            bail!("network driver '{}' not found", id);
        }

        let package_dir = self.drivers_dir().join(id);
        if package_dir.exists() {
            fs::remove_dir_all(package_dir)?;
        }

        self.save_catalog(&catalog)?;
        self.rebuild_media(&catalog)?;
        Ok(())
    }

    pub fn rebuild(&self) -> Result<DriverInjectionStatus> {
        let catalog = self.load_catalog()?;
        self.rebuild_media(&catalog)?;
        self.status()
    }

    pub fn startnet_script(&self) -> String {
        r#"@echo off
setlocal EnableExtensions EnableDelayedExpansion

echo [Diskless] Initializing WinPE network drivers...

wpeinit

set "DRIVER_MEDIA="
for %%D in (C D E F G H I J K L M N O P Q R S T U V W Y Z) do (
    if exist "%%D:\DRIVERS.TAG" (
        set "DRIVER_MEDIA=%%D:"
        goto :drivers_found
    )
)

:drivers_found
if not defined DRIVER_MEDIA (
    echo [Diskless] Network driver media was not found.
    goto :network_init
)

echo [Diskless] Driver media found at !DRIVER_MEDIA!

for /r "!DRIVER_MEDIA!\DRIVERS" %%I in (*.inf) do (
    echo [Diskless] Loading %%~fI
    drvload "%%~fI"
)

:network_init
wpeutil InitializeNetwork
wpeutil WaitForNetwork

ipconfig

echo [Diskless] Network driver initialization complete.
endlocal
"#
        .to_string()
    }

    fn load_catalog(&self) -> Result<DriverCatalog> {
        let path = self.catalog_path();
        if !path.is_file() {
            return Ok(DriverCatalog {
                version: 2,
                drivers: Vec::new(),
            });
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read driver catalog {}", path.display()))?;
        serde_json::from_str(&content).context("invalid network driver catalog")
    }

    fn save_catalog(&self, catalog: &DriverCatalog) -> Result<()> {
        fs::create_dir_all(self.media_dir())?;
        let mut catalog = catalog.clone();
        catalog.version = 2;
        let content = serde_json::to_string_pretty(&catalog)?;
        fs::write(self.catalog_path(), format!("{content}\n"))?;

        let manifest = DriverManifest::from_packages(&catalog.drivers);
        manifest.write(&self.media_dir().join(MANIFEST_FILE))?;
        Ok(())
    }

    fn rebuild_media(&self, catalog: &DriverCatalog) -> Result<()> {
        fs::create_dir_all(self.media_dir())?;

        let tag = format!(
            "DISKLESS_MANAGER_NETWORK_DRIVERS\nCATALOG_VERSION={}\nGENERATED_AT={}\n",
            catalog.version,
            Utc::now().to_rfc3339()
        );
        fs::write(self.media_dir().join(TAG_FILE), tag)?;
        fs::write(self.startnet_path(), self.startnet_script())?;

        let iso_path = self.iso_path();
        if iso_path.exists() {
            fs::remove_file(&iso_path)?;
        }

        if catalog.drivers.is_empty() {
            return Ok(());
        }

        let xorriso = find_executable("xorriso")
            .or_else(|| find_executable("genisoimage"))
            .or_else(|| find_executable("mkisofs"))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "creating network driver media requires xorriso, genisoimage, or mkisofs"
                )
            })?;

        let output = if xorriso.file_name() == Some(OsStr::new("xorriso")) {
            Command::new(&xorriso)
                .args(["-as", "mkisofs", "-J", "-R", "-V", VOLUME_LABEL, "-o"])
                .arg(&iso_path)
                .arg(self.media_dir())
                .output()
        } else {
            Command::new(&xorriso)
                .args(["-J", "-R", "-V", VOLUME_LABEL, "-o"])
                .arg(&iso_path)
                .arg(self.media_dir())
                .output()
        }
        .with_context(|| format!("failed to execute {}", xorriso.display()))?;

        if !output.status.success() {
            bail!(
                "failed to build network driver ISO: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(())
    }
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn validate_zip_paths(source: &Path) -> Result<()> {
    let output = Command::new("unzip")
        .args(["-Z1"])
        .arg(source)
        .output()
        .context("network driver import requires unzip")?;

    if !output.status.success() {
        bail!("invalid ZIP driver archive");
    }

    for entry in String::from_utf8_lossy(&output.stdout).lines() {
        let normalized = entry.replace('\\', "/");
        if normalized.starts_with('/') || normalized.contains("../") || normalized == ".." {
            bail!("unsafe path in driver archive: {}", entry);
        }
    }

    Ok(())
}

fn extract_zip(source: &Path, destination: &Path) -> Result<()> {
    let output = Command::new("unzip")
        .args(["-q", "-o"])
        .arg(source)
        .arg("-d")
        .arg(destination)
        .output()
        .context("failed to execute unzip")?;

    if !output.status.success() {
        bail!(
            "failed to extract driver archive: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

fn find_metadata(root: &Path) -> Result<ImportedDriverMetadata> {
    let Some(path) = find_file(root, "driver-info.json")? else {
        return Ok(ImportedDriverMetadata {
            name: None,
            service_name: None,
            driver_name: None,
            pnp_device_id: None,
            guid: None,
            mac_address: None,
        });
    };

    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).context("invalid driver-info.json")
}

fn find_driver_package(root: &Path) -> Result<PathBuf> {
    if root.join("driver_package").is_dir() {
        return Ok(root.join("driver_package"));
    }

    if let Some(path) = find_directory(root, "driver_package")? {
        return Ok(path);
    }

    if !find_inf_files(root)?.is_empty() {
        return Ok(root.to_path_buf());
    }

    bail!("driver archive does not contain driver_package or INF files")
}

fn find_inf_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(root, &mut |path| {
        if path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("inf"))
        {
            files.push(path.to_path_buf());
        }
    })?;
    files.sort();
    Ok(files)
}

fn copy_directory_contents(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            fs::create_dir_all(&destination_path)?;
            copy_directory_contents(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

fn collect_files(root: &Path, callback: &mut impl FnMut(&Path)) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, callback)?;
        } else {
            callback(&path);
        }
    }
    Ok(())
}

fn find_file(root: &Path, filename: &str) -> Result<Option<PathBuf>> {
    let mut found = None;
    collect_files(root, &mut |path| {
        if found.is_none()
            && path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(filename))
        {
            found = Some(path.to_path_buf());
        }
    })?;
    Ok(found)
}

fn find_directory(root: &Path, dirname: &str) -> Result<Option<PathBuf>> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(dirname))
            {
                return Ok(Some(path));
            }

            if let Some(found) = find_directory(&path, dirname)? {
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

/// Require every network-class INF to ship a catalog file alongside the
/// package. Unsigned or damaged packages (no `CatalogFile` directive, or a
/// referenced `.cat` missing from the archive) are rejected before they can
/// enter the catalog. Cryptographic signature verification itself happens at
/// install time via DISM, which is never given an unsigned-driver override.
fn require_package_catalog(
    package_source: &Path,
    inf_files: &[PathBuf],
    inspections: &[WindowsInfMetadata],
) -> Result<()> {
    for (inf, inspection) in inf_files.iter().zip(inspections.iter()) {
        if !inspection.is_network_class {
            continue;
        }
        if inspection.catalog_files.is_empty() {
            bail!(
                "network driver INF '{}' declares no catalog file; unsigned packages cannot enter the catalog",
                inf.display()
            );
        }
        let inf_dir = inf.parent().unwrap_or(package_source);
        let mut found = false;
        for catalog in &inspection.catalog_files {
            let name = catalog.rsplit(['/', '\\']).next().unwrap_or(catalog);
            for candidate in [
                inf_dir.join(catalog),
                inf_dir.join(name),
                package_source.join(catalog),
                package_source.join(name),
            ] {
                if candidate.is_file() {
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }
        if !found {
            bail!(
                "network driver INF '{}' references catalog files ({}) but none are present in the archive",
                inf.display(),
                inspection.catalog_files.join(", ")
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use std::process::Command;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let staging = tempfile::tempdir().unwrap();
        for (name, contents) in entries {
            let file_path = staging.path().join(name);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let mut file = std::fs::File::create(&file_path).unwrap();
            file.write_all(contents).unwrap();
        }
        let names: Vec<&str> = entries.iter().map(|(name, _)| *name).collect();
        let output = Command::new("zip")
            .arg("-j")
            .arg("-q")
            .arg(path)
            .args(&names)
            .current_dir(staging.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "zip failed: {output:?}");
    }

    const NETWORK_INF: &[u8] = b"[Version]\nSignature=\"$Windows NT$\"\nClass=Net\nClassGuid={4d36e972-e325-11ce-bfc1-08002be10318}\nProvider=%Vendor%\nCatalogFile=nic.cat\n[Manufacturer]\n%Vendor%=Vendor,NTamd64\n[Vendor.NTamd64]\n%Device%=Install,PCI\\VEN_8086&DEV_15F3\n[Install.NT.Services]\nAddService=NetKVM,2,NicInstall\n[Strings]\nVendor=\"Vendor\"\nDevice=\"Test NIC\"\n";
    const STORAGE_INF: &[u8] = b"[Version]\nSignature=\"$Windows NT$\"\nClass=SCSIAdapter\nClassGuid={4d36e96b-e325-11ce-bfc1-08002be10318}\nProvider=%Vendor%\nCatalogFile=storage.cat\n[Manufacturer]\n%Vendor%=Vendor,NTamd64\n[Vendor.NTamd64]\n%Device%=Install,PCI\\VEN_1234&DEV_5678\n[Install.NT.Services]\nAddService=StorDrv,2,Install\n[Strings]\nVendor=\"Vendor\"\nDevice=\"Test HBA\"\n";

    #[test]
    fn import_rejects_package_without_network_inf() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("storage.zip");
        write_zip(
            &archive,
            &[("storage.inf", STORAGE_INF), ("storage.cat", b"cat")],
        );
        let plugin = NetworkDriverInjectionPlugin::new(root.path().join("media"));
        let error = plugin.import_zip(&archive).unwrap_err();
        assert!(error.to_string().contains("network adapter"), "{error:#}");
    }

    #[test]
    fn import_rejects_network_inf_without_catalog_file() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("nocat.zip");
        let inf = String::from_utf8_lossy(NETWORK_INF).replace("CatalogFile=nic.cat\n", "");
        write_zip(&archive, &[("nic.inf", inf.as_bytes())]);
        let plugin = NetworkDriverInjectionPlugin::new(root.path().join("media"));
        let error = plugin.import_zip(&archive).unwrap_err();
        assert!(error.to_string().contains("catalog"), "{error:#}");
    }

    #[test]
    fn import_rejects_missing_catalog_payload() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("missingcat.zip");
        write_zip(&archive, &[("nic.inf", NETWORK_INF)]);
        let plugin = NetworkDriverInjectionPlugin::new(root.path().join("media"));
        let error = plugin.import_zip(&archive).unwrap_err();
        assert!(error.to_string().contains("catalog"), "{error:#}");
    }

    #[test]
    fn import_rejects_malicious_archive_paths() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("evil.zip");
        let staging = tempfile::tempdir().unwrap();
        std::fs::write(staging.path().join("nic.inf"), NETWORK_INF).unwrap();
        std::fs::write(staging.path().join("nic.cat"), b"cat").unwrap();
        let output = Command::new("python3")
            .args(["-c", "import sys,zipfile; z=zipfile.ZipFile(sys.argv[1],'w'); z.write(sys.argv[2]+'/nic.inf','nic.inf'); z.write(sys.argv[2]+'/nic.cat','nic.cat'); z.writestr('../escape.inf','evil')"])
            .arg(&archive)
            .arg(staging.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "fixture zip failed: {output:?}");
        let plugin = NetworkDriverInjectionPlugin::new(root.path().join("media"));
        let error = plugin.import_zip(&archive).unwrap_err();
        assert!(error.to_string().contains("unsafe path"), "{error:#}");
    }
}
