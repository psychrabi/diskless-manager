use anyhow::{Context, Result};
use std::os::unix::fs::{symlink, FileTypeExt};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// The small filesystem surface also lets tests emulate configfs-created files.
pub(super) trait ConfigFs {
    fn exists(&self, path: &Path) -> Result<bool>;
    fn entries(&self, path: &Path) -> Result<Vec<PathBuf>>;
    fn read(&self, path: &Path) -> Result<String>;
    fn write(&self, path: &Path, value: &str) -> Result<()>;
    fn mkdir(&self, path: &Path) -> Result<()>;
    fn rmdir(&self, path: &Path) -> Result<()>;
    fn link(&self, source: &Path, destination: &Path) -> Result<()>;
    fn link_target(&self, path: &Path) -> Result<Option<PathBuf>>;
    fn unlink(&self, path: &Path) -> Result<()>;
    fn is_block(&self, path: &Path) -> Result<bool>;
}

pub(super) struct KernelFs;
impl ConfigFs for KernelFs {
    fn exists(&self, path: &Path) -> Result<bool> {
        Ok(path.try_exists()?)
    }
    fn entries(&self, path: &Path) -> Result<Vec<PathBuf>> {
        fs::read_dir(path)?.map(|e| Ok(e?.path())).collect()
    }
    fn read(&self, path: &Path) -> Result<String> {
        fs::read_to_string(path).with_context(|| format!("read {}", path.display()))
    }
    fn write(&self, path: &Path, value: &str) -> Result<()> {
        // Do not create ordinary files if a kernel attribute is unavailable.
        fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)?
            .write_all(value.as_bytes())
            .with_context(|| format!("write {}", path.display()))
    }
    fn mkdir(&self, path: &Path) -> Result<()> {
        Ok(fs::create_dir(path)?)
    }
    fn rmdir(&self, path: &Path) -> Result<()> {
        Ok(fs::remove_dir(path)?)
    }
    fn link(&self, source: &Path, destination: &Path) -> Result<()> {
        Ok(symlink(source, destination)?)
    }
    fn link_target(&self, path: &Path) -> Result<Option<PathBuf>> {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            Ok(Some(fs::canonicalize(path)?))
        } else {
            Ok(None)
        }
    }
    fn unlink(&self, path: &Path) -> Result<()> {
        Ok(fs::remove_file(path)?)
    }
    fn is_block(&self, path: &Path) -> Result<bool> {
        Ok(fs::metadata(path)?.file_type().is_block_device())
    }
}
