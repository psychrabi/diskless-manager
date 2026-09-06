//! Offline Windows image network-driver injection.
//!
//! This module deliberately shells out to DISM rather than parsing or modifying
//! Windows images itself. The Linux PXE server therefore remains the owner of
//! the workflow while DISM remains the authoritative Windows image tool.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

#[derive(Debug, Clone)]
pub struct WindowsDriverInjectionRequest {
    pub image_path: PathBuf,
    pub driver_root: PathBuf,
    pub mount_root: Option<PathBuf>,
    pub recursive: bool,
    pub commit: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowsDriverInjectionResult {
    pub image_path: String,
    pub driver_root: String,
    pub mount_path: String,
    pub drivers_added: usize,
    pub committed: bool,
}

#[derive(Debug, Clone)]
pub struct WindowsDriverInjector {
    dism: PathBuf,
}

impl WindowsDriverInjector {
    pub fn new() -> Result<Self> {
        let dism = find_command("dism")
            .or_else(|| find_command("dism.exe"))
            .ok_or_else(|| anyhow::anyhow!("DISM was not found; Windows image injection must run on Windows or use a Windows DISM environment"))?;

        Ok(Self { dism })
    }

    pub fn with_dism(path: impl Into<PathBuf>) -> Self {
        Self { dism: path.into() }
    }

    pub fn inject(&self, request: WindowsDriverInjectionRequest) -> Result<WindowsDriverInjectionResult> {
        validate_request(&request)?;

        let temporary_mount = if request.mount_root.is_none() {
            Some(TempDir::new().context("failed to create temporary DISM mount directory")?)
        } else {
            None
        };

        let mount_path = request
            .mount_root
            .clone()
            .or_else(|| temporary_mount.as_ref().map(|dir| dir.path().to_path_buf()))
            .expect("mount path must exist");

        fs::create_dir_all(&mount_path)?;

        self.mount(&request.image_path, &mount_path)?;

        let result = self.add_drivers(&mount_path, &request.driver_root, request.recursive);

        let drivers_added = match result {
            Ok(count) => count,
            Err(error) => {
                let _ = self.discard_mount(&mount_path);
                return Err(error);
            }
        };

        if request.commit {
            self.commit_mount(&mount_path)?;
        } else {
            self.discard_mount(&mount_path)?;
        }

        Ok(WindowsDriverInjectionResult {
            image_path: request.image_path.display().to_string(),
            driver_root: request.driver_root.display().to_string(),
            mount_path: mount_path.display().to_string(),
            drivers_added,
            committed: request.commit,
        })
    }

    pub fn mount(&self, image_path: &Path, mount_path: &Path) -> Result<()> {
        run_dism(&self.dism, [
            "/Mount-Image".to_string(),
            format!("/ImageFile:{}", image_path.display()),
            format!("/MountDir:{}", mount_path.display()),
            "/Index:1".to_string(),
        ])
        .context("failed to mount Windows image")
    }

    pub fn add_drivers(&self, mount_path: &Path, driver_root: &Path, recursive: bool) -> Result<usize> {
        let inf_files = collect_inf_files(driver_root)?;
        if inf_files.is_empty() {
            bail!("no INF files found under {}", driver_root.display());
        }

        let mut count = 0usize;
        for inf in inf_files {
            let mut args = vec![
                "/Image:".to_string() + &mount_path.display().to_string(),
                "/Add-Driver".to_string(),
                format!("/Driver:{}", inf.display()),
            ];
            if recursive {
                args.push("/Recurse".to_string());
            }

            run_dism(&self.dism, args)
                .with_context(|| format!("failed to inject driver {}", inf.display()))?;
            count += 1;
        }

        Ok(count)
    }

    pub fn commit_mount(&self, mount_path: &Path) -> Result<()> {
        run_dism(&self.dism, [
            "/Unmount-Image".to_string(),
            format!("/MountDir:{}", mount_path.display()),
            "/Commit".to_string(),
        ])
        .context("failed to commit Windows image")
    }

    pub fn discard_mount(&self, mount_path: &Path) -> Result<()> {
        run_dism(&self.dism, [
            "/Unmount-Image".to_string(),
            format!("/MountDir:{}", mount_path.display()),
            "/Discard".to_string(),
        ])
        .context("failed to discard Windows image changes")
    }
}

fn validate_request(request: &WindowsDriverInjectionRequest) -> Result<()> {
    if !request.image_path.is_file() {
        bail!("Windows image does not exist: {}", request.image_path.display());
    }
    if !request.driver_root.is_dir() {
        bail!("driver root does not exist: {}", request.driver_root.display());
    }
    if request.mount_root.as_ref().is_some_and(|path| path.exists() && !path.is_dir()) {
        bail!("mount path is not a directory: {}", request.mount_root.as_ref().unwrap().display());
    }
    Ok(())
}

fn collect_inf_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(root, &mut |path| {
        if path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("inf")) {
            files.push(path.to_path_buf());
        }
    })?;
    files.sort();
    Ok(files)
}

fn collect_files(root: &Path, callback: &mut impl FnMut(&Path)) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, callback)?;
        } else {
            callback(&path);
        }
    }
    Ok(())
}

fn run_dism<I>(dism: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    let output = Command::new(dism)
        .args(&args)
        .output()
        .with_context(|| format!("failed to execute {}", dism.display()))?;

    if !output.status.success() {
        bail!(
            "DISM failed (exit {}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

fn find_command(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}
